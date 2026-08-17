use futures_util::StreamExt;
use keyring::Entry;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

const CREDENTIAL_SERVICE: &str = "com.hawkstudio.code";
const CREDENTIAL_USER: &str = "hawk-ai-provider-token";
const ALLOWED_MODELS: [&str; 4] = [
    "qwen3.7-max",
    "qwen3.7-plus",
    "qwen3.6-flash",
    "qwen3-coder-30b-a3b-instruct",
];

pub struct ProviderRuntime {
    cancellation: Mutex<Option<CancellationToken>>,
    pub(crate) client: Client,
}

impl ProviderRuntime {
    pub fn new() -> Self {
        Self {
            cancellation: Mutex::new(None),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(180))
                .build()
                .expect("failed to create the provider HTTP client"),
        }
    }

    pub fn replace_cancellation(&self) -> CancellationToken {
        let token = CancellationToken::new();
        let mut guard = self
            .cancellation
            .lock()
            .expect("provider cancellation lock poisoned");
        if let Some(previous) = guard.replace(token.clone()) {
            previous.cancel();
        }
        token
    }

    pub fn stop_all(&self) -> bool {
        let mut guard = self
            .cancellation
            .lock()
            .expect("provider cancellation lock poisoned");
        if let Some(token) = guard.take() {
            token.cancel();
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub context_size: Option<u32>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPayload {
    pub request_id: String,
    pub config: ProviderConfig,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub configured: bool,
    pub source: String,
    pub masked_key: Option<String>,
}

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StreamEvent {
    request_id: String,
    delta: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResult {
    pub request_id: String,
    pub model: String,
    pub usage: UsageSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionResult {
    pub model: String,
    pub latency_ms: u128,
    pub usage: UsageSummary,
}

fn credential_entry() -> Result<Entry, String> {
    Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER)
        .map_err(|_| "تعذر الوصول إلى Windows Credential Manager.".to_owned())
}

pub(crate) fn resolve_api_key() -> Result<(String, String), String> {
    if let Ok(key) = env::var("MODAL_PROXY_TOKEN") {
        if !key.trim().is_empty() {
            return Ok((key, "environment".to_owned()));
        }
    }
    if let Ok(key) = env::var("QWEN_API_KEY") {
        if !key.trim().is_empty() {
            return Ok((key, "environment".to_owned()));
        }
    }
    if let Ok(key) = credential_entry()?.get_password() {
        if !key.trim().is_empty() {
            return Ok((key, "credential-manager".to_owned()));
        }
    }
    // Keep the legacy Alibaba variable as a last-resort fallback. Modal and
    // other hosted providers must use the dedicated credential above.
    if let Ok(key) = env::var("DASHSCOPE_API_KEY") {
        if !key.trim().is_empty() {
            return Ok((key, "environment".to_owned()));
        }
    }
    Err("لم يتم إعداد مفتاح المزود. افتح الإعدادات ← مزود الذكاء الاصطناعي.".to_owned())
}

fn mask_key(key: &str) -> String {
    let suffix: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("••••{suffix}")
}

pub fn provider_status() -> ProviderStatus {
    match resolve_api_key() {
        Ok((key, source)) => ProviderStatus {
            configured: true,
            source,
            masked_key: Some(mask_key(&key)),
        },
        Err(_) => ProviderStatus {
            configured: false,
            source: "none".to_owned(),
            masked_key: None,
        },
    }
}

pub fn save_api_key(api_key: &str) -> Result<ProviderStatus, String> {
    let trimmed = api_key.trim();
    if trimmed.len() < 16 || trimmed.chars().any(char::is_whitespace) {
        return Err("مفتاح المزود غير صالح: يجب أن يكون طويلاً وألا يحتوي مسافات.".to_owned());
    }
    credential_entry()?
        .set_password(trimmed)
        .map_err(|_| "تعذر حفظ المفتاح داخل Windows Credential Manager.".to_owned())?;
    Ok(ProviderStatus {
        configured: true,
        source: "credential-manager".to_owned(),
        masked_key: Some(mask_key(trimmed)),
    })
}

pub fn delete_api_key() -> Result<bool, String> {
    let entry = credential_entry()?;
    match entry.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(_) => Err("تعذر حذف المفتاح من Windows Credential Manager.".to_owned()),
    }
}

pub(crate) fn validate_config(config: &ProviderConfig) -> Result<Url, String> {
    if !ALLOWED_MODELS.contains(&config.model.as_str()) {
        return Err("النموذج المحدد غير موجود في سجل Qwen الموثوق.".to_owned());
    }
    let mut base = Url::parse(config.base_url.trim())
        .map_err(|_| "عنوان المزود Base URL غير صالح.".to_owned())?;
    if base.scheme() != "https" || !base.username().is_empty() || base.password().is_some() {
        return Err("عنوان المزود يجب أن يستخدم HTTPS وألا يحتوي بيانات دخول.".to_owned());
    }
    let host = base.host_str().unwrap_or_default();
    if !(host == "dashscope.aliyuncs.com"
        || host == "dashscope-intl.aliyuncs.com"
        || host == "dashscope-us.aliyuncs.com"
        || host.ends_with(".maas.aliyuncs.com"))
        && !(host.ends_with(".modal.run") || host.ends_with(".modal.com"))
    {
        return Err(
            "حماية المفتاح منعت الإرسال: استخدم نطاق Alibaba Cloud أو Modal الرسمي فقط."
                .to_owned(),
        );
    }
    if !base.path().ends_with('/') {
        base.set_path(&format!("{}/", base.path()));
    }
    base.join("chat/completions")
        .map_err(|_| "تعذر إنشاء عنوان Chat Completions.".to_owned())
}

pub(crate) fn usage_from_value(value: &Value) -> UsageSummary {
    let usage = &value["usage"];
    UsageSummary {
        prompt_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
        completion_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
        total_tokens: usage["total_tokens"].as_u64().unwrap_or(0),
    }
}

pub(crate) async fn response_error(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|value| value["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "لم يُرجع المزود تفاصيل إضافية.".to_owned());
    format!(
        "فشل طلب مزود الذكاء الاصطناعي ({status}): {}",
        detail.chars().take(500).collect::<String>()
    )
}

pub async fn test_connection(
    runtime: &ProviderRuntime,
    config: ProviderConfig,
) -> Result<ConnectionResult, String> {
    let endpoint = validate_config(&config)?;
    let (api_key, _) = resolve_api_key()?;
    let started = std::time::Instant::now();
    let response = runtime
        .client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&json!({
            "model": config.model,
            "messages": [{"role": "user", "content": "Reply exactly OK"}],
            "max_tokens": config.max_output_tokens.unwrap_or(8),
            "enable_thinking": false,
            "stream": false
        }))
        .send()
        .await
        .map_err(|error| format!("تعذر الاتصال بمزود الذكاء الاصطناعي: {error}"))?;
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|_| "استجابة المزود ليست JSON صالحاً.".to_owned())?;
    Ok(ConnectionResult {
        model: value["model"].as_str().unwrap_or(&config.model).to_owned(),
        latency_ms: started.elapsed().as_millis(),
        usage: usage_from_value(&value),
    })
}

pub async fn stream_chat(
    app: &AppHandle,
    runtime: &ProviderRuntime,
    payload: ChatPayload,
    cancellation: CancellationToken,
) -> Result<ChatResult, String> {
    if payload.messages.is_empty() || payload.messages.len() > 100 {
        return Err("يجب أن تحتوي المحادثة على رسالة واحدة وحتى 100 رسالة.".to_owned());
    }
    if payload.messages.iter().any(|message| {
        !matches!(message.role.as_str(), "system" | "user" | "assistant")
            || serde_json::to_vec(&message.content)
                .map(|content| content.len() > 40_000_000)
                .unwrap_or(true)
    }) {
        return Err("تحتوي المحادثة على دور أو حجم رسالة غير صالح.".to_owned());
    }

    let endpoint = validate_config(&payload.config)?;
    let (api_key, _) = resolve_api_key()?;
    let response = runtime
        .client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&json!({
            "model": payload.config.model,
            "messages": payload.messages,
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": payload.config.max_output_tokens.unwrap_or(16_384)
        }))
        .send()
        .await
        .map_err(|error| format!("تعذر الاتصال بمزود الذكاء الاصطناعي: {error}"))?;
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut usage = UsageSummary::default();

    loop {
        let next = tokio::select! {
            _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
            item = stream.next() => item,
        };
        let Some(chunk) = next else { break };
        let bytes = chunk.map_err(|error| format!("انقطع بث Qwen: {error}"))?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(boundary) = buffer.find("\n\n") {
            let event = buffer[..boundary].to_owned();
            buffer.drain(..boundary + 2);
            for line in event.lines() {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" || data.is_empty() {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                let delta = value["choices"][0]["delta"]["content"]
                    .as_str()
                    .unwrap_or_default();
                if !delta.is_empty() {
                    app.emit(
                        "qwen://delta",
                        StreamEvent {
                            request_id: payload.request_id.clone(),
                            delta: delta.to_owned(),
                        },
                    )
                    .map_err(|_| "تعذر إرسال بث Qwen إلى الواجهة.".to_owned())?;
                }
                let event_usage = usage_from_value(&value);
                if event_usage.total_tokens > 0 {
                    usage = event_usage;
                }
            }
        }
    }

    Ok(ChatResult {
        request_id: payload.request_id,
        model: payload.config.model,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_alibaba_provider_hosts() {
        let result = validate_config(&ProviderConfig {
            base_url: "https://example.com/v1".to_owned(),
            model: "qwen3.7-max".to_owned(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn accepts_latest_qwen_model_on_official_host() {
        let result = validate_config(&ProviderConfig {
            base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1".to_owned(),
            model: "qwen3.7-max".to_owned(),
        });
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires a configured Qwen credential and network access"]
    async fn live_qwen_connection_uses_the_production_provider_bridge() {
        let runtime = ProviderRuntime::new();
        let result = test_connection(
            &runtime,
            ProviderConfig {
                base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1".to_owned(),
                model: "qwen3.7-max".to_owned(),
            },
        )
        .await
        .expect("the configured Qwen account should accept a production request");

        assert!(result.model.starts_with("qwen3.7-max"));
        assert!(result.usage.total_tokens > 0);
    }
}
