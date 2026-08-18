use crate::agent::AgentPayload;
use crate::provider::{
    resolve_api_key, response_error, usage_from_value, validate_config, ChatResult, ProviderRuntime,
    UsageSummary,
};
use crate::{browser_automation, project};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Output;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

const MAX_STEPS: usize = 128;
const MAX_FILE_BYTES: u64 = 1_500_000;
const MAX_WRITE_BYTES: usize = 2_000_000;
const MAX_TOOL_OUTPUT: usize = 30_000;
const MAX_NARRATION_RETRIES: usize = 16;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeltaEvent {
    request_id: String,
    delta: String,
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

#[derive(Default)]
struct Evidence {
    tool_calls: usize,
    writes: usize,
    commands: usize,
    successful_commands: Vec<String>,
    unresolved_failure: bool,
}

#[derive(Default)]
struct Requirements {
    execution: bool,
    writes: bool,
    run: bool,
    test: bool,
    lint: bool,
    build: bool,
}

pub async fn run(
    app: &AppHandle,
    runtime: &ProviderRuntime,
    payload: AgentPayload,
    cancellation: CancellationToken,
) -> Result<ChatResult, String> {
    if payload.messages.is_empty() {
        return Err("The agent conversation is empty.".to_owned());
    }

    let root = workspace_root(payload.workspace_path.as_deref())?;
    let endpoint = validate_config(&payload.config)?;
    let (api_key, _) = resolve_api_key()?;
    let model = payload.config.model.clone();
    let request_id = payload.request_id.clone();
    let permission = payload.permission_profile.clone();
    let original_request = latest_user_text(&payload.messages);
    let requirements = infer_requirements(&original_request);

    let mut messages = vec![json!({
        "role": "system",
        "content": orchestrator_prompt(&root, &permission)
    })];
    let retained = payload.messages.len().saturating_sub(10);
    for message in payload.messages.into_iter().skip(retained) {
        messages.push(json!({"role": message.role, "content": message.content}));
    }

    let mut usage = UsageSummary::default();
    let mut evidence = Evidence::default();
    let mut narration_retries = 0usize;

    emit_activity(
        app,
        &request_id,
        "autonomous-agent",
        "agent",
        "running",
        "Executing the task until verifiable completion".to_owned(),
        None,
    )?;

    for step in 0..MAX_STEPS {
        if cancellation.is_cancelled() {
            return Err("TASK_CANCELLED".to_owned());
        }
        compact_messages(&mut messages);

        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
            result = runtime.client
                .post(endpoint.clone())
                .bearer_auth(&api_key)
                .json(&json!({
                    "model": model,
                    "messages": messages,
                    "stream": false,
                    "max_tokens": 12_288,
                    "temperature": 0.1
                }))
                .send() => result.map_err(|error| format!("Unable to contact HAWK model: {error}"))?,
        };
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        let value = response
            .json::<Value>()
            .await
            .map_err(|_| "HAWK model returned invalid JSON from the provider.".to_owned())?;
        merge_usage(&mut usage, usage_from_value(&value));
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned();

        if content.is_empty() {
            narration_retries += 1;
            messages.push(json!({
                "role": "user",
                "content": "Your previous response was empty. Emit one executable action now. You may use HAWK JSON or your native <tool_call> format. Do not narrate."
            }));
            if narration_retries > MAX_NARRATION_RETRIES {
                return Err("The raw model repeatedly returned empty execution steps.".to_owned());
            }
            continue;
        }

        let Some(mut action) = parse_action(&content) else {
            narration_retries += 1;
            if narration_retries > MAX_NARRATION_RETRIES {
                return Err(format!(
                    "The raw model repeatedly returned non-executable output. Last response: {}",
                    truncate(&content, 1800)
                ));
            }
            messages.push(json!({"role": "assistant", "content": content}));
            messages.push(json!({
                "role": "user",
                "content": "That output was not executable. Do not explain what you will do. Emit exactly one action using either a JSON object with an `action` field OR your native <tool_call><function=...><parameter=...> syntax. Supported actions: list_files, read_file, write_file, replace_in_file, run_command, git_status, browser_control, finish."
            }));
            continue;
        };
        narration_retries = 0;
        normalize_action_paths(&root, &mut action)?;
        let action_name = action["action"].as_str().unwrap_or_default().to_owned();

        if action_name == "finish" {
            if let Some(reason) = completion_blocker(&requirements, &evidence) {
                messages.push(json!({"role": "assistant", "content": content}));
                messages.push(json!({
                    "role": "user",
                    "content": format!(
                        "You are not allowed to finish yet: {reason}. Continue executing immediately. Emit one executable action, not prose."
                    )
                }));
                continue;
            }
            let summary = action["summary"]
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("Task completed with verified execution evidence.")
                .trim();
            emit_activity(
                app,
                &request_id,
                "autonomous-agent",
                "agent",
                "completed",
                format!(
                    "Completed after {} tool actions and {} command runs",
                    evidence.tool_calls, evidence.commands
                ),
                None,
            )?;
            app.emit(
                "qwen://delta",
                DeltaEvent {
                    request_id: request_id.clone(),
                    delta: summary.to_owned(),
                },
            )
            .map_err(|_| "Unable to deliver the final agent result.".to_owned())?;
            return Ok(ChatResult {
                request_id,
                model,
                usage,
            });
        }

        let activity_id = format!("auto-{step}-{action_name}");
        let file_path = action["path"].as_str().map(str::to_owned);
        emit_activity(
            app,
            &request_id,
            &activity_id,
            &action_name,
            "running",
            action_detail(&action_name, &action),
            file_path.clone(),
        )?;

        let result = execute_action(
            &root,
            &permission,
            &action_name,
            &action,
            &cancellation,
        )
        .await;
        evidence.tool_calls += 1;
        let (state, output) = match result {
            Ok(output) => {
                evidence.unresolved_failure = false;
                record_success(&mut evidence, &action_name, &action);
                ("completed", output)
            }
            Err(error) => {
                evidence.unresolved_failure = true;
                ("failed", format!("Tool error: {error}"))
            }
        };
        emit_activity(
            app,
            &request_id,
            &activity_id,
            &action_name,
            state,
            truncate(output.lines().next().unwrap_or(&output), 260),
            file_path,
        )?;

        messages.push(json!({"role": "assistant", "content": content}));
        messages.push(json!({
            "role": "user",
            "content": format!(
                "HAWK TOOL RESULT [{state}] for `{action_name}`:\n{}\n\nContinue autonomously from this real result. If it failed, diagnose it and repair it. Do not stop or summarize until the requested implementation and requested checks have actually succeeded. Emit exactly one next executable action in JSON or native <tool_call> syntax.",
                truncate(&output, MAX_TOOL_OUTPUT)
            )
        }));
    }

    Err(format!(
        "HAWK autonomous execution reached the {MAX_STEPS}-step emergency guard before verified completion."
    ))
}

fn orchestrator_prompt(root: &Path, permission: &str) -> String {
    format!(
        r#"You are the execution controller for HAWK Code. The user has a real desktop workspace at: {root}
Permission profile: {permission}.

CRITICAL EXECUTION CONTRACT:
- You are executing real work, not narrating a future plan.
- Never stop with phrases such as "I will", "سأقوم", "سأبدأ", "دعني أبدأ".
- Every turn must contain exactly ONE executable action.
- HAWK accepts either the JSON action format below OR your model-native `<tool_call><function=...><parameter=...>` format. If your native tool syntax is more natural, USE IT. HAWK will parse it.
- Keep working step by step until implementation, fixes, commands, tests, lint, build, and requested verification are truly complete.
- If a tool or command fails, inspect the returned error, repair the project, rerun the failed verification, and continue automatically.
- `finish` is only valid after real execution evidence satisfies the user's requirements.
- With Full access, do not ask the user for confirmation for ordinary project edits or development commands.

Supported actions and parameters:
{{"action":"list_files","query":"optional substring"}}
{{"action":"read_file","path":"relative/path"}}
{{"action":"write_file","path":"relative/path","content":"complete file contents"}}
{{"action":"replace_in_file","path":"relative/path","oldText":"exact unique old text","newText":"replacement"}}
{{"action":"run_command","program":"node|npm|pnpm|python|cargo|git|other executable","args":["arg1","arg2"],"cwd":"optional relative directory","timeoutSeconds":120}}
{{"action":"git_status"}}
{{"action":"browser_control","browser":{{"action":"open|goto|snapshot|click|fill|type|press|screenshot|back|forward|reload|close","url":"optional","target":"optional","value":"optional","fullPage":true}}}}
{{"action":"finish","summary":"concise verified final result"}}

Important path rule: prefer workspace-relative paths. If your native tool format emits the absolute workspace path shown above, HAWK will safely convert it to a relative path only when it is inside this workspace.
`run_command` executes directly without a shell; every CLI argument must be a separate item in `args`. On Windows HAWK maps npm/pnpm/npx/yarn to their .cmd executables automatically.
Do not emit chain-of-thought. Do not expose internal reasoning. Execute one action at a time.
"#,
        root = root.display(),
        permission = permission,
    )
}

async fn execute_action(
    root: &Path,
    permission: &str,
    name: &str,
    action: &Value,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    match name {
        "list_files" => list_files(root, action["query"].as_str()),
        "read_file" => read_file(root, required_string(action, "path")?),
        "write_file" => {
            require_edit(permission)?;
            write_file(
                root,
                required_string(action, "path")?,
                required_string(action, "content")?,
            )
        }
        "replace_in_file" => {
            require_edit(permission)?;
            replace_in_file(
                root,
                required_string(action, "path")?,
                required_string(action, "oldText")?,
                required_string(action, "newText")?,
            )
        }
        "run_command" => {
            require_full(permission)?;
            run_command(root, action, cancellation).await
        }
        "git_status" => serde_json::to_string_pretty(&project::git_status(
            root.to_string_lossy().as_ref(),
        )?)
        .map_err(|_| "Unable to serialize Git status.".to_owned()),
        "browser_control" => {
            require_edit(permission)?;
            let args = action
                .get("browser")
                .ok_or_else(|| "browser_control requires a browser object.".to_owned())?;
            browser_automation::run(root, args, cancellation).await
        }
        _ => Err(format!("Unknown autonomous action: {name}")),
    }
}

fn workspace_root(path: Option<&str>) -> Result<PathBuf, String> {
    if let Some(raw) = path.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(raw);
        if !path.is_dir() {
            return Err("The active workspace is unavailable.".to_owned());
        }
        return path
            .canonicalize()
            .map_err(|_| "Unable to resolve the active workspace.".to_owned());
    }
    let root = std::env::temp_dir().join("hawk-code-general-agent");
    fs::create_dir_all(&root)
        .map_err(|_| "Unable to create the general HAWK workspace.".to_owned())?;
    root.canonicalize()
        .map_err(|_| "Unable to resolve the general HAWK workspace.".to_owned())
}

fn safe_path(root: &Path, relative: &str, allow_missing: bool) -> Result<PathBuf, String> {
    let relative = relative.trim().replace('\\', "/");
    let candidate = Path::new(&relative);
    if relative.is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Path must stay inside the active workspace.".to_owned());
    }
    let joined = root.join(candidate);
    if joined.exists() {
        let canonical = joined
            .canonicalize()
            .map_err(|_| "Unable to resolve the requested path.".to_owned())?;
        if !canonical.starts_with(root) {
            return Err("Path escapes the active workspace.".to_owned());
        }
        return Ok(canonical);
    }
    if !allow_missing {
        return Err(format!("File does not exist: {relative}"));
    }
    let mut parent = joined.parent().unwrap_or(root);
    while !parent.exists() {
        parent = parent
            .parent()
            .ok_or_else(|| "Unable to resolve destination parent.".to_owned())?;
    }
    let parent = parent
        .canonicalize()
        .map_err(|_| "Unable to resolve destination parent.".to_owned())?;
    if !parent.starts_with(root) {
        return Err("Destination escapes the active workspace.".to_owned());
    }
    Ok(joined)
}

fn list_files(root: &Path, query: Option<&str>) -> Result<String, String> {
    let query = query.unwrap_or_default().trim().to_lowercase();
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("Unable to list {}: {error}", directory.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if matches!(
                    name.as_str(),
                    ".git" | "node_modules" | "target" | ".next" | "dist" | "build"
                ) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if let Ok(relative) = path.strip_prefix(root) {
                let text = relative.to_string_lossy().replace('\\', "/");
                if query.is_empty() || text.to_lowercase().contains(&query) {
                    found.push(text);
                    if found.len() >= 700 {
                        break;
                    }
                }
            }
        }
        if found.len() >= 700 {
            break;
        }
    }
    found.sort();
    Ok(if found.is_empty() {
        "No matching workspace files.".to_owned()
    } else {
        found.join("\n")
    })
}

fn read_file(root: &Path, relative: &str) -> Result<String, String> {
    let path = safe_path(root, relative, false)?;
    let metadata = path
        .metadata()
        .map_err(|_| "Unable to read file metadata.".to_owned())?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(format!("File must be text and under {MAX_FILE_BYTES} bytes."));
    }
    fs::read_to_string(path).map_err(|_| "File is not valid UTF-8 text.".to_owned())
}

fn write_file(root: &Path, relative: &str, content: &str) -> Result<String, String> {
    if content.len() > MAX_WRITE_BYTES {
        return Err("File content exceeds the 2 MB write limit.".to_owned());
    }
    let path = safe_path(root, relative, true)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Unable to create parent directory: {error}"))?;
    }
    fs::write(&path, content).map_err(|error| format!("Unable to write {relative}: {error}"))?;
    Ok(format!("Wrote {relative} ({} bytes).", content.len()))
}

fn replace_in_file(
    root: &Path,
    relative: &str,
    old_text: &str,
    new_text: &str,
) -> Result<String, String> {
    if old_text.is_empty() {
        return Err("oldText must not be empty.".to_owned());
    }
    let path = safe_path(root, relative, false)?;
    let content = fs::read_to_string(&path)
        .map_err(|_| "Target file is not valid UTF-8 text.".to_owned())?;
    let count = content.matches(old_text).count();
    if count != 1 {
        return Err(format!("oldText must match exactly once; found {count} matches."));
    }
    let updated = content.replacen(old_text, new_text, 1);
    if updated.len() > MAX_WRITE_BYTES {
        return Err("Edited file exceeds the 2 MB write limit.".to_owned());
    }
    fs::write(&path, updated)
        .map_err(|error| format!("Unable to save {relative}: {error}"))?;
    Ok(format!("Updated {relative}."))
}

async fn run_command(
    root: &Path,
    action: &Value,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let raw_program = required_string(action, "program")?.trim();
    if raw_program.is_empty() || raw_program.contains('/') || raw_program.contains('\\') {
        return Err("program must be an executable name available on PATH.".to_owned());
    }
    let program = platform_program(raw_program);
    let args = action["args"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| item.to_string())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let cwd = match action["cwd"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(relative) => {
            let path = safe_path(root, relative, false)?;
            if !path.is_dir() {
                return Err("run_command cwd must be a directory.".to_owned());
            }
            path
        }
        None => root.to_path_buf(),
    };
    let timeout_seconds = action["timeoutSeconds"]
        .as_u64()
        .unwrap_or(180)
        .clamp(1, 600);

    let mut command = Command::new(&program);
    command.args(&args).current_dir(&cwd).kill_on_drop(true);
    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
        result = timeout(Duration::from_secs(timeout_seconds), command.output()) => {
            result
                .map_err(|_| format!("Command timed out after {timeout_seconds}s: {raw_program}"))?
                .map_err(|error| format!("Unable to start {raw_program}: {error}"))?
        }
    };
    command_result(raw_program, &args, output)
}

fn command_result(program: &str, args: &[String], output: Output) -> Result<String, String> {
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.success() {
        Ok(format!(
            "Command succeeded: {} {}\n{}",
            program,
            args.join(" "),
            truncate(&combined, MAX_TOOL_OUTPUT)
        ))
    } else {
        Err(format!(
            "Command failed (exit {:?}): {} {}\n{}",
            output.status.code(),
            program,
            args.join(" "),
            truncate(&combined, MAX_TOOL_OUTPUT)
        ))
    }
}

fn platform_program(program: &str) -> String {
    if cfg!(windows) {
        match program.to_ascii_lowercase().as_str() {
            "npm" => "npm.cmd".to_owned(),
            "pnpm" => "pnpm.cmd".to_owned(),
            "npx" => "npx.cmd".to_owned(),
            "yarn" => "yarn.cmd".to_owned(),
            other => other.to_owned(),
        }
    } else {
        program.to_owned()
    }
}

fn parse_action(content: &str) -> Option<Value> {
    let trimmed = content.trim();
    if let Some(value) = parse_json_action(trimmed) {
        return Some(value);
    }

    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim);
    if let Some(candidate) = unfenced {
        if let Some(value) = parse_json_action(candidate) {
            return Some(value);
        }
    }

    if let Some(candidate) = extract_first_json_object(trimmed) {
        if let Some(value) = parse_json_action(candidate) {
            return Some(value);
        }
    }

    parse_native_tool_call(trimmed)
}

fn parse_json_action(candidate: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(candidate).ok()?;
    if value.get("action").and_then(Value::as_str).is_some() {
        return Some(value);
    }
    let name = value.get("name").and_then(Value::as_str)?;
    let arguments = value.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let mut result = match arguments {
        Value::Object(map) => map,
        Value::String(raw) => serde_json::from_str::<Map<String, Value>>(&raw).ok()?,
        _ => Map::new(),
    };
    result.insert("action".to_owned(), Value::String(name.to_owned()));
    Some(Value::Object(result))
}

fn parse_native_tool_call(input: &str) -> Option<Value> {
    let tool_start = input.find("<tool_call")?;
    let tool = &input[tool_start..];

    if let Some(open_end) = tool.find('>') {
        let after_open = &tool[open_end + 1..];
        if let Some(close) = after_open.find("</tool_call>") {
            let body = after_open[..close].trim();
            if body.starts_with('{') {
                if let Some(value) = parse_json_action(body) {
                    return Some(value);
                }
            }
        }
    }

    let function_marker = "<function=";
    let function_start = tool.find(function_marker)? + function_marker.len();
    let function_tail = &tool[function_start..];
    let function_end = function_tail.find('>')?;
    let function_name = function_tail[..function_end]
        .trim()
        .trim_matches(['\'', '"'])
        .trim();
    if function_name.is_empty() {
        return None;
    }

    let function_body = &function_tail[function_end + 1..];
    let body_end = function_body.find("</function>").unwrap_or(function_body.len());
    let body = &function_body[..body_end];
    let parameters = parse_native_parameters(body);

    if function_name == "browser_control" {
        let mut browser = Map::new();
        for (key, value) in parameters {
            browser.insert(key, value);
        }
        return Some(json!({"action": "browser_control", "browser": Value::Object(browser)}));
    }

    let mut action = Map::new();
    action.insert(
        "action".to_owned(),
        Value::String(function_name.to_owned()),
    );
    for (key, value) in parameters {
        action.insert(key, value);
    }
    Some(Value::Object(action))
}

fn parse_native_parameters(body: &str) -> Map<String, Value> {
    let mut result = Map::new();
    let marker = "<parameter=";
    let mut cursor = 0usize;
    while let Some(relative_start) = body[cursor..].find(marker) {
        let start = cursor + relative_start + marker.len();
        let tail = &body[start..];
        let Some(name_end) = tail.find('>') else {
            break;
        };
        let name = tail[..name_end]
            .trim()
            .trim_matches(['\'', '"'])
            .trim();
        if name.is_empty() {
            cursor = start + name_end + 1;
            continue;
        }
        let value_start = start + name_end + 1;
        let remaining = &body[value_start..];
        let Some(value_end_relative) = remaining.find("</parameter>") else {
            break;
        };
        let raw_value = remaining[..value_end_relative].trim();
        result.insert(name.to_owned(), parse_native_value(raw_value));
        cursor = value_start + value_end_relative + "</parameter>".len();
    }
    result
}

fn parse_native_value(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Value::String(String::new());
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return value;
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if let Ok(value) = trimmed.parse::<u64>() {
        return Value::Number(value.into());
    }
    Value::String(trimmed.to_owned())
}

fn normalize_action_paths(root: &Path, action: &mut Value) -> Result<(), String> {
    for key in ["path", "cwd"] {
        let Some(raw) = action.get(key).and_then(Value::as_str).map(str::to_owned) else {
            continue;
        };
        let normalized = normalize_workspace_relative(root, &raw)?;
        if let Some(object) = action.as_object_mut() {
            object.insert(key.to_owned(), Value::String(normalized));
        }
    }
    Ok(())
}

fn normalize_workspace_relative(root: &Path, raw: &str) -> Result<String, String> {
    let cleaned = raw
        .trim()
        .trim_matches(['\'', '"', '`'])
        .trim()
        .to_owned();
    if cleaned.is_empty() {
        return Err("Path is empty.".to_owned());
    }
    let candidate = PathBuf::from(&cleaned);
    if !candidate.is_absolute() {
        return Ok(cleaned.replace('\\', "/"));
    }

    if candidate.exists() {
        if let Ok(canonical) = candidate.canonicalize() {
            if let Ok(relative) = canonical.strip_prefix(root) {
                return Ok(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    let root_text = normalize_windows_path_text(&root.to_string_lossy());
    let candidate_text = normalize_windows_path_text(&cleaned);
    let prefix = format!("{root_text}/");
    if candidate_text == root_text {
        return Ok(".".to_owned());
    }
    if let Some(relative) = candidate_text.strip_prefix(&prefix) {
        if !relative.is_empty() {
            return Ok(relative.to_owned());
        }
    }
    Err("The model emitted an absolute path outside the active workspace.".to_owned())
}

fn normalize_windows_path_text(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

fn extract_first_json_object(input: &str) -> Option<&str> {
    let start = input.find('{')?;
    let bytes = input.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for index in start..bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return input.get(start..=index);
                }
            }
            _ => {}
        }
    }
    None
}

fn infer_requirements(text: &str) -> Requirements {
    let lower = text.to_lowercase();
    Requirements {
        execution: contains_any(
            &lower,
            &[
                "أنش", "انش", "ابن", "بناء", "نفذ", "نفّذ", "طبق", "طبّق", "اصلح",
                "أصلح", "عدّل", "عدل", "اكتب", "شغل", "شغّل", "اختبر", "create", "build",
                "implement", "fix", "modify", "write", "run", "test",
            ],
        ),
        writes: contains_any(
            &lower,
            &[
                "أنش", "انش", "ابن", "بناء", "طبق", "طبّق", "اصلح", "أصلح", "عدّل",
                "عدل", "اكتب", "create", "build", "implement", "fix", "modify", "write",
            ],
        ),
        run: contains_any(&lower, &["شغل", "شغّل", "تشغيل", "نفذ", "نفّذ", "run", "execute"]),
        test: contains_any(&lower, &["اختبار", "اختبارات", "اختبر", "test", "tests", "npm test"]),
        lint: contains_any(&lower, &["lint", "فحص lint"]),
        build: contains_any(
            &lower,
            &["npm run build", "pnpm build", "cargo build", " build", "بناء المشروع", "البناء"],
        ),
    }
}

fn completion_blocker(requirements: &Requirements, evidence: &Evidence) -> Option<String> {
    if evidence.unresolved_failure {
        return Some("the most recent tool action failed and has not been repaired yet".to_owned());
    }
    if requirements.execution && evidence.tool_calls == 0 {
        return Some("no real tool action has been executed".to_owned());
    }
    if requirements.writes && evidence.writes == 0 {
        return Some("the request requires project changes but no file was written".to_owned());
    }
    if requirements.run && evidence.commands == 0 {
        return Some("the request requires running something but no command has succeeded".to_owned());
    }
    if requirements.test && !command_evidence(evidence, &["test"]) {
        return Some("tests were requested but no successful test command is recorded".to_owned());
    }
    if requirements.lint && !command_evidence(evidence, &["lint"]) {
        return Some("lint was requested but no successful lint command is recorded".to_owned());
    }
    if requirements.build && !command_evidence(evidence, &["build"]) {
        return Some("a build was requested but no successful build command is recorded".to_owned());
    }
    None
}

fn command_evidence(evidence: &Evidence, needles: &[&str]) -> bool {
    evidence.successful_commands.iter().any(|command| {
        let lower = command.to_lowercase();
        needles.iter().any(|needle| lower.contains(needle))
    })
}

fn record_success(evidence: &mut Evidence, name: &str, action: &Value) {
    match name {
        "write_file" | "replace_in_file" => evidence.writes += 1,
        "run_command" => {
            evidence.commands += 1;
            let program = action["program"].as_str().unwrap_or_default();
            let args = action["args"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|value| value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            evidence
                .successful_commands
                .push(format!("{program} {args}"));
        }
        _ => {}
    }
}

fn action_detail(name: &str, action: &Value) -> String {
    match name {
        "list_files" => "Inspecting workspace files".to_owned(),
        "read_file" => format!("Reading {}", action["path"].as_str().unwrap_or("file")),
        "write_file" => format!("Writing {}", action["path"].as_str().unwrap_or("file")),
        "replace_in_file" => format!("Editing {}", action["path"].as_str().unwrap_or("file")),
        "run_command" => format!(
            "Running {} {}",
            action["program"].as_str().unwrap_or("command"),
            action["args"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|value| value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default()
        ),
        "git_status" => "Inspecting Git status".to_owned(),
        "browser_control" => "Controlling browser with Playwright".to_owned(),
        _ => format!("Executing {name}"),
    }
}

fn latest_user_text(messages: &[crate::provider::ChatMessage]) -> String {
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
    content
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| {
                    (part["type"].as_str() == Some("text"))
                        .then(|| part["text"].as_str())
                        .flatten()
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn compact_messages(messages: &mut Vec<Value>) {
    const KEEP: usize = 44;
    if messages.len() <= KEEP + 3 {
        return;
    }
    let mut compacted = messages.iter().take(3).cloned().collect::<Vec<_>>();
    compacted.push(json!({
        "role": "system",
        "content": "Earlier execution history was compacted by HAWK. Continue from the recent real tool evidence below. Do not redo completed work unless verification requires it."
    }));
    compacted.extend(messages.iter().skip(messages.len() - KEEP).cloned());
    *messages = compacted;
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("Missing required string parameter: {key}"))
}

fn require_edit(permission: &str) -> Result<(), String> {
    if matches!(permission, "auto" | "full") {
        Ok(())
    } else {
        Err("This action requires edit permission.".to_owned())
    }
}

fn require_full(permission: &str) -> Result<(), String> {
    if permission == "full" {
        Ok(())
    } else {
        Err("Running arbitrary project commands requires Full access.".to_owned())
    }
}

fn contains_any(text: &str, values: &[&str]) -> bool {
    values.iter().any(|value| text.contains(value))
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

fn merge_usage(total: &mut UsageSummary, next: UsageSummary) {
    total.prompt_tokens += next.prompt_tokens;
    total.completion_tokens += next.completion_tokens;
    total.total_tokens += next.total_tokens;
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
    fn parses_plain_json_action() {
        let action = parse_action(r#"{"action":"write_file","path":"package.json","content":"{}"}"#)
            .expect("JSON action should parse");
        assert_eq!(action["action"], "write_file");
        assert_eq!(action["path"], "package.json");
    }

    #[test]
    fn parses_qwen_native_write_file_call() {
        let input = r#"<tool_call>
<function=write_file>
<parameter=content>
{
  "name": "hawk-task-engine",
  "version": "1.0.0"
}
</parameter>
<parameter=path>
C:\Users\HCES\Desktop\اختبااررر\package.json
</parameter>
</function>
</tool_call>"#;
        let action = parse_action(input).expect("native tool call should parse");
        assert_eq!(action["action"], "write_file");
        assert_eq!(action["path"], r#"C:\Users\HCES\Desktop\اختبااررر\package.json"#);
        assert!(action["content"].as_str().unwrap_or_default().contains("hawk-task-engine"));
    }

    #[test]
    fn parses_native_run_command_arguments_as_json() {
        let input = r#"<tool_call><function=run_command><parameter=program>npm</parameter><parameter=args>["test"]</parameter><parameter=timeoutSeconds>240</parameter></function></tool_call>"#;
        let action = parse_action(input).expect("native command should parse");
        assert_eq!(action["action"], "run_command");
        assert_eq!(action["program"], "npm");
        assert_eq!(action["args"][0], "test");
        assert_eq!(action["timeoutSeconds"], 240);
    }

    #[test]
    fn converts_absolute_windows_workspace_path_to_relative() {
        let root = PathBuf::from(r#"C:\Users\HCES\Desktop\اختبااررر"#);
        let relative = normalize_workspace_relative(
            &root,
            r#"C:\Users\HCES\Desktop\اختبااررر\package.json"#,
        )
        .expect("path should stay in workspace");
        assert_eq!(relative, "package.json");
    }

    #[test]
    fn rejects_absolute_path_outside_workspace() {
        let root = PathBuf::from(r#"C:\Users\HCES\Desktop\اختبااررر"#);
        assert!(normalize_workspace_relative(&root, r#"C:\Windows\win.ini"#).is_err());
    }

    #[test]
    fn completion_requires_requested_checks() {
        let requirements = Requirements {
            execution: true,
            writes: true,
            run: true,
            test: true,
            lint: true,
            build: true,
        };
        let evidence = Evidence {
            tool_calls: 12,
            writes: 8,
            commands: 3,
            successful_commands: vec![
                "npm test".to_owned(),
                "npm run lint".to_owned(),
                "npm run build".to_owned(),
            ],
            unresolved_failure: false,
        };
        assert!(completion_blocker(&requirements, &evidence).is_none());
    }
}
