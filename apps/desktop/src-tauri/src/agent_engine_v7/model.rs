use super::protocol::{action_schema, parse_action, AgentAction};
use crate::provider::{response_error, usage_from_value, ProviderRuntime, UsageSummary};
use reqwest::Url;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

const MAX_PROVIDER_RETRIES: usize = 3;
const MAX_PARSE_RETRIES: usize = 2;
const MAX_MODEL_OUTPUT_TOKENS: u32 = 8_192;

pub async fn next_action(
    runtime: &ProviderRuntime,
    endpoint: &Url,
    api_key: &str,
    model: &str,
    messages: Vec<Value>,
    cancellation: &CancellationToken,
) -> Result<(AgentAction, UsageSummary, String), String> {
    let mut messages = messages;
    let mut total_usage = UsageSummary::default();
    let mut last_parse_error = String::new();

    for parse_attempt in 0..=MAX_PARSE_RETRIES {
        let value = request_once(
            runtime,
            endpoint,
            api_key,
            model,
            &messages,
            cancellation,
        )
        .await?;
        merge_usage(&mut total_usage, usage_from_value(&value));
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned();

        match parse_action(&content) {
            Ok(action) => return Ok((action, total_usage, content)),
            Err(error) => {
                last_parse_error = error;
                if parse_attempt >= MAX_PARSE_RETRIES {
                    break;
                }
                messages.push(json!({
                    "role": "user",
                    "content": "Your last output was not a valid HAWK action. Return exactly one JSON object that matches the supplied action schema. Do not emit prose or XML tool-call markup."
                }));
            }
        }
    }

    Err(format!(
        "The model did not produce a valid schema-constrained HAWK action after retries: {last_parse_error}"
    ))
}

async fn request_once(
    runtime: &ProviderRuntime,
    endpoint: &Url,
    api_key: &str,
    model: &str,
    messages: &[Value],
    cancellation: &CancellationToken,
) -> Result<Value, String> {
    let mut last_error = String::new();
    for attempt in 0..MAX_PROVIDER_RETRIES {
        if cancellation.is_cancelled() {
            return Err("TASK_CANCELLED".to_owned());
        }
        let max_tokens = match attempt {
            0 => MAX_MODEL_OUTPUT_TOKENS,
            1 => 6_144,
            _ => 4_096,
        };
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
            result = runtime.client
                .post(endpoint.clone())
                .bearer_auth(api_key)
                .json(&json!({
                    "model": model,
                    "messages": messages,
                    "stream": false,
                    "max_tokens": max_tokens,
                    "temperature": 0.1,
                    "top_p": 0.9,
                    "response_format": {
                        "type": "json_object",
                        "schema": action_schema()
                    }
                }))
                .send() => result.map_err(|error| format!("Unable to contact HAWK model: {error}"))?,
        };

        if response.status().is_success() {
            return response
                .json::<Value>()
                .await
                .map_err(|_| "HAWK model returned invalid JSON from the provider.".to_owned());
        }

        let status = response.status();
        let error = response_error(response).await;
        if status.is_server_error() && attempt + 1 < MAX_PROVIDER_RETRIES {
            last_error = error;
            sleep(Duration::from_millis(800 * (attempt as u64 + 1))).await;
            continue;
        }
        return Err(error);
    }
    Err(if last_error.is_empty() {
        "The model provider failed after automatic retries.".to_owned()
    } else {
        format!("The model provider failed after automatic retries. Last error: {last_error}")
    })
}

fn merge_usage(total: &mut UsageSummary, next: UsageSummary) {
    total.prompt_tokens += next.prompt_tokens;
    total.completion_tokens += next.completion_tokens;
    total.total_tokens += next.total_tokens;
}
