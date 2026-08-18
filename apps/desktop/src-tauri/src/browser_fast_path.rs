use crate::agent::AgentPayload;
use crate::provider::{self, ChatMessage, ChatPayload, ChatResult, ProviderRuntime, UsageSummary};
use crate::{attachments, browser_automation};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Output;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
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

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeltaEvent {
    request_id: String,
    delta: String,
}

struct GeneratedScreenshot {
    name: String,
    path: String,
}

struct SimplePythonTask {
    file_name: String,
    printed_text: String,
    run_after_write: bool,
}

pub async fn try_run(
    app: &AppHandle,
    runtime: &ProviderRuntime,
    payload: AgentPayload,
    cancellation: CancellationToken,
) -> Result<FastPathOutcome, String> {
    let user_text = latest_user_text(&payload.messages);
    let recent_context = recent_user_context(&payload.messages, 3);

    if let Some(result) = try_run_simple_python_task(
        app,
        &payload,
        &user_text,
        &recent_context,
        &cancellation,
    )
    .await?
    {
        return Ok(FastPathOutcome::Handled(result));
    }

    let Some(url) = extract_http_url(&user_text) else {
        return Ok(FastPathOutcome::Continue(with_execution_contract(
            payload,
            &user_text,
        )));
    };
    if !looks_like_browser_request(&user_text) {
        return Ok(FastPathOutcome::Continue(with_execution_contract(
            payload,
            &user_text,
        )));
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

    let generated_screenshot = screenshot_output
        .as_deref()
        .and_then(|output| emit_generated_screenshot(app, &root, output).ok().flatten());

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
            "The browser actions already ran successfully on the user's computer through Playwright. Do not say that browsing is unavailable.\n\nOriginal request:\n{user_text}\n\nOpened URL:\n{url}\n\nBrowser open result:\n{}\n\nCurrent page snapshot:\n{}\n\nScreenshot result:\n{}\n\nRegistered in-app screenshot preview:\n{}\n\nAnswer the user's request now using this browser evidence. Be concise but specific. The HAWK interface will append its own clickable screenshot preview control after your answer, so do not invent a file link or claim the preview is unavailable.",
            truncate(&open_output, 2_000),
            truncate(&snapshot_output, 30_000),
            screenshot_output
                .as_deref()
                .map(|value| truncate(value, 4_000))
                .unwrap_or_else(|| "No screenshot was requested.".to_owned()),
            generated_screenshot
                .as_ref()
                .map(|item| item.name.as_str())
                .unwrap_or("No preview was registered.")
        )),
    });

    let result = provider::stream_chat(
        app,
        runtime,
        ChatPayload {
            request_id: request_id.clone(),
            config,
            messages: final_messages,
        },
        cancellation,
    )
    .await?;

    if let Some(screenshot) = generated_screenshot {
        let encoded_path = URL_SAFE_NO_PAD.encode(screenshot.path.as_bytes());
        app.emit(
            "qwen://delta",
            DeltaEvent {
                request_id,
                delta: format!(
                    "\n\n[📷 {}](#hawk-attachment-{encoded_path})",
                    screenshot.name
                ),
            },
        )
        .map_err(|_| "Unable to append the screenshot preview control.".to_owned())?;
    }

    Ok(FastPathOutcome::Handled(result))
}

async fn try_run_simple_python_task(
    app: &AppHandle,
    payload: &AgentPayload,
    latest_text: &str,
    recent_context: &str,
    cancellation: &CancellationToken,
) -> Result<Option<ChatResult>, String> {
    let Some(task) = parse_simple_python_task(latest_text, recent_context) else {
        return Ok(None);
    };
    let Some(workspace) = payload
        .workspace_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if !matches!(payload.permission_profile.as_str(), "auto" | "full") {
        return Err(
            "Creating files needs Approve for me or Full access. Enable it, then ask again."
                .to_owned(),
        );
    }
    if task.run_after_write && payload.permission_profile != "full" {
        return Err(
            "Running arbitrary project commands needs Full access. Enable Full access, then ask again."
                .to_owned(),
        );
    }

    let root = PathBuf::from(workspace)
        .canonicalize()
        .map_err(|_| "Unable to resolve the active workspace.".to_owned())?;
    let relative = safe_relative_path(&task.file_name)?;
    let path = root.join(&relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "Unable to create the destination directory.".to_owned())?;
    }
    let python_literal = serde_json::to_string(&task.printed_text)
        .map_err(|_| "Unable to encode the requested Python text.".to_owned())?;
    let content = format!("print({python_literal})\n");

    emit_activity(
        app,
        &payload.request_id,
        "direct-python-write",
        "write_file",
        "running",
        format!("Writing {}", task.file_name),
        Some(task.file_name.clone()),
    )?;
    if let Err(error) = fs::write(&path, content) {
        emit_activity(
            app,
            &payload.request_id,
            "direct-python-write",
            "write_file",
            "failed",
            format!("Unable to write {}: {error}", task.file_name),
            Some(task.file_name.clone()),
        )?;
        return Err(format!("Unable to write {}: {error}", task.file_name));
    }
    emit_activity(
        app,
        &payload.request_id,
        "direct-python-write",
        "write_file",
        "completed",
        format!("Created {}", task.file_name),
        Some(task.file_name.clone()),
    )?;

    let mut final_text = format!("تم إنشاء `{}` فعليًا داخل المشروع.", task.file_name);
    if task.run_after_write {
        emit_activity(
            app,
            &payload.request_id,
            "direct-python-run",
            "run_command",
            "running",
            format!("Running {} with Python", task.file_name),
            Some(task.file_name.clone()),
        )?;
        let output = run_python(&root, &task.file_name, cancellation).await;
        let output = match output {
            Ok(output) if output.status.success() => output,
            Ok(output) => {
                let combined = command_text(&output);
                emit_activity(
                    app,
                    &payload.request_id,
                    "direct-python-run",
                    "run_command",
                    "failed",
                    truncate(&combined, 240),
                    Some(task.file_name.clone()),
                )?;
                return Err(format!(
                    "{} was created, but Python returned an error:\n{}",
                    task.file_name,
                    truncate(&combined, 4_000)
                ));
            }
            Err(error) => {
                emit_activity(
                    app,
                    &payload.request_id,
                    "direct-python-run",
                    "run_command",
                    "failed",
                    truncate(&error, 240),
                    Some(task.file_name.clone()),
                )?;
                return Err(error);
            }
        };
        let combined = command_text(&output);
        if !combined.contains(&task.printed_text) {
            emit_activity(
                app,
                &payload.request_id,
                "direct-python-run",
                "run_command",
                "failed",
                "Python ran, but the requested output was not observed".to_owned(),
                Some(task.file_name.clone()),
            )?;
            return Err(format!(
                "{} ran, but HAWK did not observe the expected output `{}`. Actual output:\n{}",
                task.file_name,
                task.printed_text,
                truncate(&combined, 4_000)
            ));
        }
        emit_activity(
            app,
            &payload.request_id,
            "direct-python-run",
            "run_command",
            "completed",
            format!("Verified output: {}", task.printed_text),
            Some(task.file_name.clone()),
        )?;
        final_text.push_str(&format!(
            " ثم شغّلته وتأكدت أن التنفيذ نجح. الناتج:\n\n`{}`",
            task.printed_text
        ));
    }

    app.emit(
        "qwen://delta",
        DeltaEvent {
            request_id: payload.request_id.clone(),
            delta: final_text,
        },
    )
    .map_err(|_| "Unable to deliver the completed task result to the interface.".to_owned())?;

    Ok(Some(ChatResult {
        request_id: payload.request_id.clone(),
        model: payload.config.model.clone(),
        usage: UsageSummary::default(),
    }))
}

fn parse_simple_python_task(latest_text: &str, recent_context: &str) -> Option<SimplePythonTask> {
    let latest_lower = latest_text.to_lowercase();
    let context_lower = recent_context.to_lowercase();
    let latest_requests_creation = looks_like_file_creation(latest_text);
    let latest_continues = [
        "ابدأ",
        "ابدا",
        "ابدء",
        "نفذ",
        "نفّذ",
        "ابدأ بالانشاء",
        "ابدا بالانشاء",
        "start",
        "do it",
        "go ahead",
    ]
    .iter()
    .any(|value| latest_lower.contains(value));
    if !latest_requests_creation && !latest_continues {
        return None;
    }
    if !looks_like_file_creation(recent_context) {
        return None;
    }

    let file_name = extract_file_name(recent_context)?;
    if !file_name.to_ascii_lowercase().ends_with(".py") {
        return None;
    }
    let printed_text = extract_printed_text(recent_context)?;
    if printed_text.is_empty() || printed_text.len() > 500 {
        return None;
    }
    let run_after_write = [
        "شغله",
        "شغّله",
        "شغل الملف",
        "شغّل الملف",
        "ثم شغ",
        "وتشغيله",
        "run it",
        "then run",
        "execute it",
    ]
    .iter()
    .any(|value| context_lower.contains(value));

    Some(SimplePythonTask {
        file_name,
        printed_text,
        run_after_write,
    })
}

fn looks_like_file_creation(text: &str) -> bool {
    let lowered = text.to_lowercase();
    [
        "أنشئ ملف",
        "انشئ ملف",
        "إنشاء ملف",
        "انشاء ملف",
        "اكتب ملف",
        "create file",
        "write file",
    ]
    .iter()
    .any(|value| lowered.contains(value))
}

fn extract_file_name(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let cleaned = token.trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | '،' | ';' | '؛' | ':' | '!' | '?' | '؟'
                )
        });
        let lower = cleaned.to_ascii_lowercase();
        [".py", ".js", ".ts", ".tsx", ".jsx", ".rs", ".json", ".md", ".txt"]
            .iter()
            .any(|extension| lower.ends_with(extension))
            .then(|| cleaned.replace('\\', "/"))
    })
}

fn extract_printed_text(text: &str) -> Option<String> {
    let lowered = text.to_lowercase();
    for marker in ["يطبع ", "اطبع ", "prints ", "print "] {
        if let Some(index) = lowered.find(marker) {
            let start = index + marker.len();
            let tail = &text[start..];
            let end = [" ثم ", " وبعد", " and then ", " then ", ".", "\n"]
                .iter()
                .filter_map(|separator| tail.find(separator))
                .min()
                .unwrap_or(tail.len());
            let value = tail[..end]
                .trim()
                .trim_matches(|character: char| {
                    matches!(character, '`' | '"' | '\'' | ',' | '،' | ';' | '؛' | ':' | '!' | '?' | '؟')
                })
                .trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn safe_relative_path(relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative.trim());
    if relative.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("The requested file path is not a safe workspace-relative path.".to_owned());
    }
    Ok(path.to_path_buf())
}

async fn run_python(
    root: &Path,
    relative: &str,
    cancellation: &CancellationToken,
) -> Result<Output, String> {
    match run_program(root, "python", &[relative], cancellation).await {
        Ok(output) => Ok(output),
        Err(first_error) if cfg!(windows) => run_program(root, "py", &["-3", relative], cancellation)
            .await
            .map_err(|second_error| {
                format!(
                    "Unable to run Python. `python` failed with: {first_error}. `py -3` failed with: {second_error}"
                )
            }),
        Err(error) => Err(error),
    }
}

async fn run_program(
    root: &Path,
    program: &str,
    args: &[&str],
    cancellation: &CancellationToken,
) -> Result<Output, String> {
    let mut command = Command::new(program);
    command.args(args).current_dir(root).kill_on_drop(true);
    tokio::select! {
        _ = cancellation.cancelled() => Err("TASK_CANCELLED".to_owned()),
        result = timeout(Duration::from_secs(120), command.output()) => {
            result
                .map_err(|_| format!("{program} timed out after 120 seconds."))?
                .map_err(|error| format!("Unable to start {program}: {error}"))
        }
    }
}

fn command_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .trim()
    .to_owned()
}

fn with_execution_contract(mut payload: AgentPayload, user_text: &str) -> AgentPayload {
    if !looks_like_action_request(user_text) || payload.messages.len() >= 100 {
        return payload;
    }
    payload.messages.insert(
        0,
        ChatMessage {
            role: "system".to_owned(),
            content: Value::String(
                "HAWK EXECUTION CONTRACT: This turn asks for real work on the user's computer or project. A prose promise such as 'I will do it' or 'I am going to create it' is not a completed answer. Call the available tools immediately, keep using tools until every requested edit/action and verification step has actually completed, and only then return the final response. If the request says to run, test, verify, inspect, or confirm something, do not finish before that verification has really run. Never report success for an action that has not produced a successful tool result."
                    .to_owned(),
            ),
        },
    );
    payload
}

fn looks_like_action_request(text: &str) -> bool {
    let lowered = text.to_lowercase();
    [
        "أنشئ",
        "انشئ",
        "اكتب",
        "عدل",
        "عدّل",
        "اصلح",
        "أصلح",
        "شغل",
        "شغّل",
        "اختبر",
        "نفذ",
        "نفّذ",
        "ابدأ",
        "ابدا",
        "create",
        "write",
        "edit",
        "fix",
        "run",
        "test",
        "execute",
        "implement",
    ]
    .iter()
    .any(|keyword| lowered.contains(keyword))
}

async fn run_browser_action(
    app: &AppHandle,
    request_id: &str,
    root: &Path,
    activity_id: &str,
    args: Value,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    emit_activity(
        app,
        request_id,
        activity_id,
        "browser_control",
        "running",
        browser_detail(&args),
        None,
    )?;
    match browser_automation::run(root, &args, cancellation).await {
        Ok(output) => {
            emit_activity(
                app,
                request_id,
                activity_id,
                "browser_control",
                "completed",
                truncate(output.lines().next().unwrap_or("Browser action completed"), 220),
                None,
            )?;
            Ok(output)
        }
        Err(error) => {
            emit_activity(
                app,
                request_id,
                activity_id,
                "browser_control",
                "failed",
                truncate(&error, 220),
                None,
            )?;
            Err(error)
        }
    }
}

fn emit_generated_screenshot(
    app: &AppHandle,
    root: &Path,
    output: &str,
) -> Result<Option<GeneratedScreenshot>, String> {
    let Some(path) = screenshot_path_from_output(root, output) else {
        return Ok(None);
    };
    let mut prepared = attachments::prepare(attachments::AttachmentPayload {
        paths: vec![path.to_string_lossy().into_owned()],
    })?;
    let Some(attachment) = prepared.pop() else {
        return Ok(None);
    };
    let generated = GeneratedScreenshot {
        name: attachment.name.clone(),
        path: attachment.path.clone(),
    };
    let payload = serde_json::to_value(&attachment)
        .map_err(|_| "Unable to serialize the generated screenshot preview.".to_owned())?;
    app.emit("attachment://generated", payload)
        .map_err(|_| "Unable to deliver the generated screenshot to the interface.".to_owned())?;
    Ok(Some(generated))
}

fn screenshot_path_from_output(root: &Path, output: &str) -> Option<PathBuf> {
    for line in output.lines().rev() {
        let lower = line.to_ascii_lowercase();
        let Some(png_end) = lower.rfind(".png") else {
            continue;
        };
        let end = png_end + 4;
        let start = lower
            .find("playwright-cli")
            .or_else(|| {
                lower[..png_end]
                    .rfind(|character: char| character.is_whitespace())
                    .map(|index| index + 1)
            })
            .unwrap_or(0);
        let raw = line[start..end]
            .trim()
            .trim_matches(|character: char| {
                matches!(character, '`' | '"' | '\'' | '(' | ')' | '[' | ']')
            });
        if raw.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(raw);
        let candidate = if candidate.is_absolute() {
            candidate
        } else {
            root.join(candidate)
        };
        if let Ok(canonical) = candidate.canonicalize() {
            if canonical.is_file()
                && canonical
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            {
                return Some(canonical);
            }
        }
    }
    None
}

fn emit_activity(
    app: &AppHandle,
    request_id: &str,
    id: &str,
    tool: &str,
    state: &str,
    detail: String,
    file_path: Option<String>,
) -> Result<(), String> {
    app.emit(
        "agent://activity",
        ActivityEvent {
            request_id: request_id.to_owned(),
            id: id.to_owned(),
            tool: tool.to_owned(),
            state: state.to_owned(),
            detail,
            file_path,
        },
    )
    .map_err(|_| "Unable to deliver agent activity to the interface.".to_owned())
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

fn recent_user_context(messages: &[ChatMessage], max_messages: usize) -> String {
    let mut values = messages
        .iter()
        .rev()
        .filter(|message| message.role == "user")
        .take(max_messages)
        .map(|message| content_text(&message.content))
        .collect::<Vec<_>>();
    values.reverse();
    values.join("\n\n")
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

    #[test]
    fn recognizes_simple_python_create_run_task() {
        let text = "أنشئ ملف test.py داخل المشروع واكتب فيه برنامج يطبع Hello from HAWK ثم شغله وتأكد أنه يعمل";
        let task = parse_simple_python_task(text, text).expect("task should be recognized");
        assert_eq!(task.file_name, "test.py");
        assert_eq!(task.printed_text, "Hello from HAWK");
        assert!(task.run_after_write);
    }

    #[test]
    fn resumes_simple_task_from_short_follow_up() {
        let previous = "أنشئ ملف test.py داخل المشروع واكتب فيه برنامج يطبع Hello from HAWK ثم شغله";
        let context = format!("{previous}\n\nابدا بالانشاء");
        let task = parse_simple_python_task("ابدا بالانشاء", &context)
            .expect("follow-up should resume the concrete task");
        assert_eq!(task.file_name, "test.py");
        assert!(task.run_after_write);
    }
}