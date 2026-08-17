use crate::agent::AgentPayload;
use crate::browser_automation;
use crate::provider::{self, ChatMessage, ChatPayload, ChatResult, ProviderRuntime};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

pub enum FastPathOutcome {
    Handled(ChatResult),
    Continue(AgentPayload),
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ActivityEvent {
    request_id: String,
    id: String,
    tool: String,
    state: String,
    detail: String,
    file_path: Option<String>,
}

pub async fn try_run(
    app: &AppHandle,
    runtime: &ProviderRuntime,
    payload: AgentPayload,
    cancellation: CancellationToken,
) -> Result<FastPathOutcome, String> {
    let user_text = latest_user_text(&payload.messages);
    let Some(url) = extract_http_url(&user_text) else {
        return Ok(FastPathOutcome::Continue(payload));
    };
    if !looks_like_browser_request(&user_text) {
        return Ok(FastPathOutcome::Continue(payload));
    }
    if !matches!(payload.permission_profile.as_str(), "auto" | "full") {
        return Err(
            "Browser control needs Approve for me or Full access. Enable it, then ask again."
                .to_owned(),
        );
    }

    let root = browser_root(payload.workspace_path.as_deref())?;
    let wants_screenshot = asks_for_screenshot(&user_text);

    let open_output = run_browser_action(
        app,
        &payload.request_id,
        &root,
        "browser-fast-open",
        json!({"action": "open", "url": url}),
        &cancellation,
    )
    .await?;

    let snapshot_output = run_browser_action(
        app,
        &payload.request_id,
        &root,
        "browser-fast-snapshot",
        json!({"action": "snapshot"}),
        &cancellation,
    )
    .await?;

    let screenshot_output = if wants_screenshot {
        Some(
            run_browser_action(
                app,
                &payload.request_id,
                &root,
                "browser-fast-screenshot",
                json!({"action": "screenshot", "fullPage": true}),
                &cancellation,
            )
            .await?,
        )
    } else {
        None
    };

    let AgentPayload {
        request_id,
        mut config,
        messages,
        workspace_path: _,
        permission_profile: _,
    } = payload;

    config.max_output_tokens = Some(config.max_output_tokens.unwrap_or(1_200).min(1_200));

    let mut final_messages = messages
        .into_iter()
        .filter(|message| message.role == "system")
        .take(3)
        .collect::<Vec<_>>();
    final_messages.push(ChatMessage {
        role: "user".to_owned(),
        content: Value::String(format!(
            "The browser actions already ran successfully on the user's computer through Playwright. Do not say that browsing is unavailable.\n\nOriginal request:\n{user_text}\n\nOpened URL:\n{url}\n\nBrowser open result:\n{}\n\nCurrent page snapshot:\n{}\n\nScreenshot result:\n{}\n\nAnswer the user's request now using this browser evidence. Be concise but specific. If a screenshot path is present, mention that it was captured successfully.",
            truncate(&open_output, 2_000),
            truncate(&snapshot_output, 30_000),
            screenshot_output
                .as_deref()
                .map(|value| truncate(value, 4_000))
                .unwrap_or_else(|| "No screenshot was requested.".to_owned())
        )),
    });

    let result = provider::stream_chat(
        app,
        runtime,
        ChatPayload {
            request_id,
            config,
            messages: final_messages,
        },
        cancellation,
    )
    .await?;

    Ok(FastPathOutcome::Handled(result))
}

async fn run_browser_action(
    app: &AppHandle,
    request_id: &str,
    root: &std::path::Path,
    activity_id: &str,
    args: Value,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    emit_activity(
        app,
        request_id,
        activity_id,
        "running",
        browser_detail(&args),
    )?;
    match browser_automation::run(root, &args, cancellation).await {
        Ok(output) => {
            emit_activity(
                app,
                request_id,
                activity_id,
                "completed",
                truncate(output.lines().next().unwrap_or("Browser action completed"), 220),
            )?;
            Ok(output)
        }
        Err(error) => {
            emit_activity(
                app,
                request_id,
                activity_id,
                "failed",
                truncate(&error, 220),
            )?;
            Err(error)
        }
    }
}

fn emit_activity(
    app: &AppHandle,
    request_id: &str,
    id: &str,
    state: &str,
    detail: String,
) -> Result<(), String> {
    app.emit(
        "agent://activity",
        ActivityEvent {
            request_id: request_id.to_owned(),
            id: id.to_owned(),
            tool: "browser_control".to_owned(),
            state: state.to_owned(),
            detail,
            file_path: None,
        },
    )
    .map_err(|_| "Unable to deliver browser activity to the interface.".to_owned())
}

fn browser_detail(args: &Value) -> String {
    match args["action"].as_str().unwrap_or("browser") {
        "open" => format!(
            "Opening {} in Playwright",
            args["url"].as_str().unwrap_or("browser")
        ),
        "snapshot" => "Reading the current browser page".to_owned(),
        "screenshot" => "Capturing the browser page".to_owned(),
        _ => "Controlling the browser".to_owned(),
    }
}

fn browser_root(workspace_path: Option<&str>) -> Result<PathBuf, String> {
    if let Some(raw) = workspace_path.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(raw);
        if path.is_dir() {
            return path
                .canonicalize()
                .map_err(|_| "Unable to resolve the active workspace for browser control.".to_owned());
        }
    }
    let root = std::env::temp_dir().join("hawk-code-general-agent");
    fs::create_dir_all(&root)
        .map_err(|_| "HAWK could not create its browser workspace.".to_owned())?;
    root.canonicalize()
        .map_err(|_| "HAWK could not resolve its browser workspace.".to_owned())
}

fn latest_user_text(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| content_text(&message.content))
        .unwrap_or_default()
}

fn content_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_owned();
    }
    let Some(parts) = content.as_array() else {
        return String::new();
    };
    parts
        .iter()
        .filter_map(|part| {
            (part["type"].as_str() == Some("text"))
                .then(|| part["text"].as_str())
                .flatten()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn looks_like_browser_request(text: &str) -> bool {
    let lowered = text.to_lowercase();
    [
        "browser",
        "screenshot",
        "navigate",
        "visit ",
        "open http",
        "متصفح",
        "المتصفح",
        "افتح",
        "تصفح",
        "لقطة شاشة",
        "سكرين شوت",
        "حلل الصفحة",
    ]
    .iter()
    .any(|keyword| lowered.contains(keyword))
}

fn asks_for_screenshot(text: &str) -> bool {
    let lowered = text.to_lowercase();
    ["screenshot", "لقطة شاشة", "سكرين شوت", "صورة للشاشة"]
        .iter()
        .any(|keyword| lowered.contains(keyword))
}

fn extract_http_url(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let cleaned = token.trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\'' | ',' | '،' | ';' | '؛' | '!' | '?' | '؟'
                )
        });
        let lower = cleaned.to_ascii_lowercase();
        (lower.starts_with("https://") || lower.starts_with("http://")).then(|| {
            cleaned
                .trim_end_matches(|character: char| matches!(character, '.' | ':'))
                .to_owned()
        })
    })
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_owned()
    } else {
        format!(
            "{}\n... truncated by HAWK Code ...",
            value.chars().take(max_chars).collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_explicit_arabic_browser_request() {
        let text = "افتح https://example.com في المتصفح ثم خذ لقطة شاشة";
        assert!(looks_like_browser_request(text));
        assert_eq!(extract_http_url(text).as_deref(), Some("https://example.com"));
        assert!(asks_for_screenshot(text));
    }
}
