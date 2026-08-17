use crate::provider::{
    resolve_api_key, response_error, usage_from_value, validate_config, ChatMessage, ChatResult,
    ProviderConfig, ProviderRuntime, UsageSummary,
};
use crate::{browser_automation, project, project_graph};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Emitter};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

const MAX_AGENT_STEPS: usize = 512;
const MAX_PROJECT_FILES_PER_TURN: usize = 500;
const MAX_BATCH_FILES: usize = 50;
const MAX_BATCH_FILE_CHARS: usize = 3_000;
const MAX_FILE_BYTES: u64 = 400_000;
const MAX_WRITE_BYTES: usize = 2_000_000;
const MAX_TOOL_OUTPUT: usize = 180_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPayload {
    pub request_id: String,
    pub config: ProviderConfig,
    pub messages: Vec<ChatMessage>,
    pub workspace_path: Option<String>,
    pub permission_profile: String,
}

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

pub async fn run(
    app: &AppHandle,
    runtime: &ProviderRuntime,
    payload: AgentPayload,
    cancellation: CancellationToken,
) -> Result<ChatResult, String> {
    if payload.messages.is_empty() || payload.messages.len() > 100 {
        return Err("The agent conversation must contain between 1 and 100 messages.".to_owned());
    }
    let root = canonical_workspace(payload.workspace_path.as_deref().unwrap_or_default())?;
    let mut graph = project_graph::sync(app, &root)?;
    let endpoint = validate_config(&payload.config)?;
    let (api_key, _) = resolve_api_key()?;
    let mut messages = payload
        .messages
        .into_iter()
        .map(|message| json!({"role": message.role, "content": message.content}))
        .collect::<Vec<_>>();
    messages.insert(
        1.min(messages.len()),
        json!({"role": "system", "content": graph.context()}),
    );
    let mut total_usage = UsageSummary::default();
    let mut reviewed_files = HashSet::new();

    for step in 0..MAX_AGENT_STEPS {
        if cancellation.is_cancelled() {
            return Err("TASK_CANCELLED".to_owned());
        }
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
            result = runtime.client
                .post(endpoint.clone())
                .bearer_auth(&api_key)
                .json(&json!({
                    "model": payload.config.model,
                    "messages": messages,
                    "tools": tool_definitions(),
                    "tool_choice": "auto",
                    "parallel_tool_calls": false,
                    "stream": false
                }))
                .send() => result.map_err(|error| format!("Unable to contact Qwen: {error}"))?,
        };
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        let value = response
            .json::<Value>()
            .await
            .map_err(|_| "Qwen returned an invalid agent response.".to_owned())?;
        merge_usage(&mut total_usage, usage_from_value(&value));
        let message = value["choices"][0]["message"].clone();
        let mut calls = message["tool_calls"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            if let Some(content) = message["content"].as_str() {
                calls = parse_text_tool_calls(content);
            }
        }

        if calls.is_empty() {
            let content = message["content"].as_str().unwrap_or_default().trim();
            if content.is_empty() {
                return Err("Qwen completed the agent turn without a response.".to_owned());
            }
            app.emit(
                "qwen://delta",
                DeltaEvent {
                    request_id: payload.request_id.clone(),
                    delta: content.to_owned(),
                },
            )
            .map_err(|_| "Unable to deliver the agent response to the interface.".to_owned())?;
            return Ok(ChatResult {
                request_id: payload.request_id,
                model: payload.config.model,
                usage: total_usage,
            });
        }

        messages.push(message);
        for (index, call) in calls.iter().enumerate() {
            if cancellation.is_cancelled() {
                return Err("TASK_CANCELLED".to_owned());
            }
            let call_id = call["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("step-{step}-{index}"));
            let tool = call["function"]["name"].as_str().unwrap_or("unknown");
            let arguments = call["function"]["arguments"]
                .as_str()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .unwrap_or_else(|| json!({}));
            let file_path = arguments["path"].as_str().map(str::to_owned);
            emit_activity(
                app,
                &payload.request_id,
                &call_id,
                tool,
                "running",
                activity_detail(tool, &arguments),
                file_path.clone(),
            )?;
            let result = execute_tool(
                &root,
                tool,
                &arguments,
                &payload.permission_profile,
                &cancellation,
                &mut reviewed_files,
                &mut graph,
            )
            .await;
            let (state, output) = match result {
                Ok(output) => ("completed", output),
                Err(error) => ("failed", format!("Tool error: {error}")),
            };
            emit_activity(
                app,
                &payload.request_id,
                &call_id,
                tool,
                state,
                activity_result_detail(tool, state, &output),
                file_path,
            )?;
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": truncate(&output, MAX_TOOL_OUTPUT)
            }));
        }
    }

    messages.push(json!({
        "role": "system",
        "content": format!(
            "The emergency loop guard activated after {MAX_AGENT_STEPS} tool rounds. Do not request more tools. Finish the user's task now using the evidence already collected, clearly state any remaining uncertainty, and give a concise final response."
        )
    }));
    let response = tokio::select! {
        _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
        result = runtime.client
            .post(endpoint)
            .bearer_auth(&api_key)
            .json(&json!({
                "model": payload.config.model,
                "messages": messages,
                "stream": false
            }))
            .send() => result.map_err(|error| format!("Unable to contact Qwen for the final summary: {error}"))?,
    };
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|_| "Qwen returned an invalid final agent response.".to_owned())?;
    merge_usage(&mut total_usage, usage_from_value(&value));
    let content = value["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .trim();
    if content.is_empty() {
        return Err(
            "Qwen reached the emergency loop guard without returning a final summary.".to_owned(),
        );
    }
    app.emit(
        "qwen://delta",
        DeltaEvent {
            request_id: payload.request_id.clone(),
            delta: content.to_owned(),
        },
    )
    .map_err(|_| "Unable to deliver the final agent summary to the interface.".to_owned())?;
    Ok(ChatResult {
        request_id: payload.request_id,
        model: payload.config.model,
        usage: total_usage,
    })
}

fn parse_text_tool_calls(content: &str) -> Vec<Value> {
    let trimmed = content.trim();
    let candidates = [trimmed, trimmed.trim_matches('`').trim()];
    for candidate in candidates {
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            let values = match value {
                Value::Array(items) => items,
                Value::Object(_) => vec![value],
                _ => Vec::new(),
            };
            let calls = values
                .into_iter()
                .filter_map(|item| {
                    let function = item.get("function")?.clone();
                    function.get("name")?.as_str()?;
                    Some(json!({
                        "id": item.get("id").and_then(Value::as_str).unwrap_or("text-tool-call"),
                        "type": "function",
                        "function": function,
                    }))
                })
                .collect::<Vec<_>>();
            if !calls.is_empty() {
                return calls;
            }
        }
    }
    Vec::new()
}

fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "List paths from the persistent HAWK Graph project index. This does not reread unchanged file contents.",
                "parameters": {"type": "object", "properties": {"query": {"type": "string", "description": "Optional case-insensitive path filter"}}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "project_graph_structure",
                "description": "Recall the saved project hierarchy from persistent HAWK Graph memory without scanning file contents again.",
                "parameters": {"type": "object", "properties": {"query": {"type": "string", "description": "Optional path filter"}}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "project_graph_query",
                "description": "Search paths and source previously cached by HAWK Graph. Prefer this over rereading the project. The graph is incrementally synchronized before every task, so unchanged source is recalled from local memory and only changed files are refreshed.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Code symbol, feature, path, or text to recall"},
                        "maxResults": {"type": "integer", "minimum": 1, "maximum": 30}
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read one UTF-8 text file inside the active workspace. Use this for a focused full-file inspection.",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_files",
                "description": "Read up to 50 UTF-8 project files in one batch. Use repeated batches for broad reviews; one agent task supports up to 500 unique files. Each large file is sampled to keep the review responsive, then use read_file for any file that needs full inspection.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "paths": {
                            "type": "array",
                            "items": {"type": "string"},
                            "minItems": 1,
                            "maxItems": MAX_BATCH_FILES
                        }
                    },
                    "required": ["paths"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "git_status",
                "description": "Inspect the real Git status and line statistics for the active workspace.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "browser_control",
                "description": "Control a real Chromium browser through Playwright when the user asks HAWK to browse, navigate, inspect, click, fill, type, press keys, take a screenshot, or test a website. Start with open, then snapshot. Use snapshot refs/targets for interactions and take another snapshot after navigation or major DOM changes. Never use this tool for unrelated coding work.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["open", "goto", "snapshot", "click", "fill", "type", "press", "screenshot", "back", "forward", "reload", "close"]
                        },
                        "url": {"type": "string", "description": "Required for open/goto; must use http or https"},
                        "target": {"type": "string", "description": "Snapshot element reference or Playwright target used for click/fill"},
                        "value": {"type": "string", "description": "Text for fill/type or keyboard key for press"},
                        "fullPage": {"type": "boolean", "description": "Use full-page screenshot when true"}
                    },
                    "required": ["action"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "replace_in_file",
                "description": "Replace one exact, unique text block in a UTF-8 workspace file. Use for precise edits.",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "oldText": {"type": "string"}, "newText": {"type": "string"}}, "required": ["path", "oldText", "newText"]}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Create or fully rewrite a UTF-8 file inside the active workspace. Prefer replace_in_file for existing files.",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "content": {"type": "string"}}, "required": ["path", "content"]}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "create_skill",
                "description": "Create a reusable project-local HAWK skill as .hawk/skills/<id>/SKILL.md. Use when the user asks to build or save a new skill.",
                "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Lowercase letters, numbers, and hyphens only"}, "description": {"type": "string"}, "instructions": {"type": "string"}}, "required": ["id", "description", "instructions"]}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "run_check",
                "description": "Run a detected project check after edits.",
                "parameters": {"type": "object", "properties": {"kind": {"type": "string", "enum": ["test", "typecheck", "lint", "build"]}}, "required": ["kind"]}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_android_devices",
                "description": "List Android devices currently authorized through USB debugging (ADB). This is read-only and only works in the desktop app.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "install_android_apk",
                "description": "Install a local APK on one Android phone connected with authorized USB debugging. Only use when the user explicitly asks to install that APK and Full access is enabled. The apkPath must be an existing absolute .apk file. If multiple phones are attached, deviceSerial is required.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "apkPath": {"type": "string", "description": "Existing absolute path to the APK"},
                        "deviceSerial": {"type": "string", "description": "Optional authorized ADB device serial"}
                    },
                    "required": ["apkPath"]
                }
            }
        }
    ])
}

async fn execute_tool(
    root: &Path,
    tool: &str,
    args: &Value,
    permission: &str,
    cancellation: &CancellationToken,
    reviewed_files: &mut HashSet<String>,
    graph: &mut project_graph::ProjectGraph,
) -> Result<String, String> {
    match tool {
        "list_files" | "project_graph_structure" => Ok(graph.structure(args["query"].as_str())),
        "project_graph_query" => {
            graph.query(required_string(args, "query")?, args["maxResults"].as_u64())
        }
        "read_file" => {
            let relative = required_string(args, "path")?;
            let content = read_file(root, relative, graph)?;
            record_file_read(reviewed_files, relative)?;
            Ok(content)
        }
        "read_files" => read_files(root, args, reviewed_files, graph),
        "git_status" => {
            serde_json::to_string_pretty(&project::git_status(root.to_string_lossy().as_ref())?)
                .map_err(|_| "Unable to serialize Git status.".to_owned())
        }
        "browser_control" => {
            require_edit_permission(permission)?;
            browser_automation::run(root, args, cancellation).await
        }
        "replace_in_file" => {
            require_edit_permission(permission)?;
            let relative = required_string(args, "path")?;
            let result = replace_in_file(
                root,
                relative,
                required_string(args, "oldText")?,
                required_string(args, "newText")?,
            )?;
            graph.refresh_written_file(root, relative)?;
            Ok(result)
        }
        "write_file" => {
            require_edit_permission(permission)?;
            let relative = required_string(args, "path")?;
            let result = write_file(root, relative, required_string(args, "content")?)?;
            graph.refresh_written_file(root, relative)?;
            Ok(result)
        }
        "create_skill" => {
            require_edit_permission(permission)?;
            let id = required_string(args, "id")?;
            let result = create_skill(
                root,
                id,
                required_string(args, "description")?,
                required_string(args, "instructions")?,
            )?;
            graph.refresh_written_file(root, &format!(".hawk/skills/{id}/SKILL.md"))?;
            Ok(result)
        }
        "run_check" => {
            require_edit_permission(permission)?;
            run_check(root, required_string(args, "kind")?, cancellation).await
        }
        "list_android_devices" => list_android_devices(cancellation).await,
        "install_android_apk" => {
            require_full_permission(permission)?;
            install_android_apk(
                required_string(args, "apkPath")?,
                args["deviceSerial"].as_str(),
                cancellation,
            )
            .await
        }
        _ => Err(format!("Unknown tool: {tool}")),
    }
}

fn canonical_workspace(path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        let root = std::env::temp_dir().join("hawk-code-general-agent");
        fs::create_dir_all(&root)
            .map_err(|_| "HAWK could not create its general-agent workspace.".to_owned())?;
        return root
            .canonicalize()
            .map_err(|_| "HAWK could not resolve its general-agent workspace.".to_owned());
    }
    let root = PathBuf::from(path.trim());
    if !root.is_dir() {
        return Err("The active workspace is unavailable.".to_owned());
    }
    root.canonicalize()
        .map_err(|_| "The active workspace path could not be resolved.".to_owned())
}

fn safe_path(root: &Path, relative: &str, allow_missing: bool) -> Result<PathBuf, String> {
    let candidate = Path::new(relative.trim());
    if relative.trim().is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("The requested path is not a safe workspace-relative path.".to_owned());
    }
    let joined = root.join(candidate);
    if joined.exists() {
        let canonical = joined
            .canonicalize()
            .map_err(|_| "The requested file could not be resolved.".to_owned())?;
        if !canonical.starts_with(root) {
            return Err("The requested path escapes the workspace.".to_owned());
        }
        return Ok(canonical);
    }
    if !allow_missing {
        return Err("The requested file does not exist.".to_owned());
    }
    let parent = joined
        .parent()
        .ok_or_else(|| "The requested path has no parent directory.".to_owned())?;
    let existing_parent = nearest_existing_parent(parent)?;
    let canonical_parent = existing_parent
        .canonicalize()
        .map_err(|_| "The destination directory could not be resolved.".to_owned())?;
    if !canonical_parent.starts_with(root) {
        return Err("The requested path escapes the workspace.".to_owned());
    }
    Ok(joined)
}

fn nearest_existing_parent(path: &Path) -> Result<&Path, String> {
    let mut current = path;
    loop {
        if current.exists() {
            return Ok(current);
        }
        current = current
            .parent()
            .ok_or_else(|| "No valid workspace parent was found.".to_owned())?;
    }
}

fn read_file(
    root: &Path,
    relative: &str,
    graph: &mut project_graph::ProjectGraph,
) -> Result<String, String> {
    let path = safe_path(root, relative, false)?;
    let metadata = path
        .metadata()
        .map_err(|_| "The requested file metadata could not be read.".to_owned())?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "The requested file is not a readable text file under {MAX_FILE_BYTES} bytes."
        ));
    }
    graph.read_text(&path, relative)
}

fn read_files(
    root: &Path,
    args: &Value,
    reviewed_files: &mut HashSet<String>,
    graph: &mut project_graph::ProjectGraph,
) -> Result<String, String> {
    let paths = args["paths"]
        .as_array()
        .ok_or_else(|| "paths must be an array of workspace-relative file paths.".to_owned())?;
    if paths.is_empty() || paths.len() > MAX_BATCH_FILES {
        return Err(format!(
            "A read_files batch must contain between 1 and {MAX_BATCH_FILES} paths."
        ));
    }

    let mut batch_keys = HashSet::new();
    let mut requested = Vec::new();
    for value in paths {
        let relative = value
            .as_str()
            .ok_or_else(|| "Every read_files path must be a string.".to_owned())?;
        let key = normalized_read_key(relative);
        if batch_keys.insert(key.clone()) {
            requested.push((relative, key));
        }
    }
    let new_files = requested
        .iter()
        .filter(|(_, key)| !reviewed_files.contains(key))
        .count();
    if reviewed_files.len() + new_files > MAX_PROJECT_FILES_PER_TURN {
        return Err(format!(
            "This agent task can inspect up to {MAX_PROJECT_FILES_PER_TURN} unique project files. It has already inspected {}; narrow the next batch or start a follow-up task.",
            reviewed_files.len()
        ));
    }

    let mut sections = Vec::new();
    let mut succeeded = 0usize;
    for (relative, key) in requested {
        match read_file(root, relative, graph) {
            Ok(content) => {
                reviewed_files.insert(key);
                succeeded += 1;
                sections.push(format!(
                    "===== FILE: {relative} =====\n{}",
                    truncate(&content, MAX_BATCH_FILE_CHARS)
                ));
            }
            Err(error) => sections.push(format!(
                "===== FILE: {relative} =====\n[Unable to read: {error}]"
            )),
        }
    }
    if succeeded == 0 {
        return Err(format!(
            "None of the requested files could be read.\n{}",
            sections.join("\n\n")
        ));
    }
    Ok(format!(
        "Read {succeeded} files in this batch; {} of {MAX_PROJECT_FILES_PER_TURN} unique files inspected in this task.\n\n{}",
        reviewed_files.len(),
        sections.join("\n\n")
    ))
}

fn record_file_read(reviewed_files: &mut HashSet<String>, relative: &str) -> Result<(), String> {
    let key = normalized_read_key(relative);
    if reviewed_files.contains(&key) {
        return Ok(());
    }
    if reviewed_files.len() >= MAX_PROJECT_FILES_PER_TURN {
        return Err(format!(
            "This agent task has inspected {MAX_PROJECT_FILES_PER_TURN} unique project files. Start a follow-up task to inspect additional files."
        ));
    }
    reviewed_files.insert(key);
    Ok(())
}

fn normalized_read_key(relative: &str) -> String {
    let normalized = relative.trim().replace('\\', "/");
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

fn replace_in_file(root: &Path, relative: &str, old: &str, new: &str) -> Result<String, String> {
    if old.is_empty() {
        return Err("oldText must not be empty.".to_owned());
    }
    let path = safe_path(root, relative, false)?;
    let content = fs::read_to_string(&path)
        .map_err(|_| "The target file is not valid UTF-8 text.".to_owned())?;
    let occurrences = content.matches(old).count();
    if occurrences != 1 {
        return Err(format!(
            "oldText must match exactly once; found {occurrences} matches."
        ));
    }
    let updated = content.replacen(old, new, 1);
    if updated.len() > MAX_WRITE_BYTES {
        return Err("The edited file exceeds the 2 MB safety limit.".to_owned());
    }
    fs::write(&path, updated).map_err(|_| "The edited file could not be saved.".to_owned())?;
    Ok(format!("Updated {relative} with one exact replacement."))
}

fn write_file(root: &Path, relative: &str, content: &str) -> Result<String, String> {
    if content.len() > MAX_WRITE_BYTES {
        return Err("The file exceeds the 2 MB safety limit.".to_owned());
    }
    let path = safe_path(root, relative, true)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "The destination directory could not be created.".to_owned())?;
    }
    fs::write(path, content).map_err(|_| "The file could not be saved.".to_owned())?;
    Ok(format!("Wrote {relative} ({} bytes).", content.len()))
}

fn create_skill(
    root: &Path,
    id: &str,
    description: &str,
    instructions: &str,
) -> Result<String, String> {
    let id = id.trim();
    if !(2..=48).contains(&id.len())
        || id.starts_with('-')
        || id.ends_with('-')
        || !id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err("Skill id must use 2-48 lowercase letters, numbers, or hyphens.".to_owned());
    }
    let description = description.trim();
    let instructions = instructions.trim();
    if description.is_empty() || instructions.is_empty() {
        return Err("Skill description and instructions must not be empty.".to_owned());
    }
    let relative = format!(".hawk/skills/{id}/SKILL.md");
    let content = format!(
        "---\nname: {id}\ndescription: {}\n---\n\n# {}\n\n{}\n",
        description.replace(['\r', '\n'], " "),
        id.replace('-', " "),
        instructions
    );
    write_file(root, &relative, &content)?;
    Ok(format!("Created project skill {id} at {relative}."))
}

async fn run_check(
    root: &Path,
    kind: &str,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let pnpm = if cfg!(windows) { "pnpm.cmd" } else { "pnpm" };
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let (program, args): (&str, Vec<&str>) = if root.join("pnpm-lock.yaml").exists() {
        (pnpm, vec![kind])
    } else if root.join("package.json").exists() {
        (npm, vec!["run", kind])
    } else if root.join("Cargo.toml").exists() {
        match kind {
            "test" => ("cargo", vec!["test"]),
            "build" => ("cargo", vec!["build"]),
            "typecheck" => ("cargo", vec!["check"]),
            "lint" => ("cargo", vec!["clippy", "--", "-D", "warnings"]),
            _ => return Err("Unsupported check kind.".to_owned()),
        }
    } else {
        return Err("No supported package manager was detected for this workspace.".to_owned());
    };
    let mut command = Command::new(program);
    command.args(args).current_dir(root).kill_on_drop(true);
    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
        result = timeout(Duration::from_secs(120), command.output()) => {
            result.map_err(|_| "The project check timed out after 120 seconds.".to_owned())?
                .map_err(|error| format!("Unable to start the project check: {error}"))?
        }
    };
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(format!(
            "The {kind} check failed:\n{}",
            truncate(&combined, MAX_TOOL_OUTPUT)
        ));
    }
    Ok(format!(
        "The {kind} check passed.\n{}",
        truncate(&combined, MAX_TOOL_OUTPUT)
    ))
}

fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args[key]
        .as_str()
        .ok_or_else(|| format!("Missing required string argument: {key}"))
}

fn require_edit_permission(permission: &str) -> Result<(), String> {
    if matches!(permission, "auto" | "full") {
        Ok(())
    } else {
        Err("This action needs edit permission. Choose Approve for me or Full access, then ask again.".to_owned())
    }
}

fn require_full_permission(permission: &str) -> Result<(), String> {
    if permission == "full" {
        Ok(())
    } else {
        Err("Installing an app on a connected phone needs Full access. Enable Full access, then ask again.".to_owned())
    }
}

fn adb_program() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let bundled = PathBuf::from(local_app_data)
                .join("Android")
                .join("Sdk")
                .join("platform-tools")
                .join("adb.exe");
            if bundled.is_file() {
                return Ok(bundled);
            }
        }
        return Ok(PathBuf::from("adb.exe"));
    }
    #[cfg(not(target_os = "windows"))]
    Ok(PathBuf::from("adb"))
}

async fn adb_output(args: &[&str], cancellation: &CancellationToken) -> Result<String, String> {
    let program = adb_program()?;
    let mut command = Command::new(&program);
    command.args(args).kill_on_drop(true);
    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
        result = timeout(Duration::from_secs(120), command.output()) => result
            .map_err(|_| "ADB timed out after two minutes.".to_owned())?
            .map_err(|error| format!("ADB is unavailable. Install Android Platform Tools or add adb to PATH: {error}"))?,
    };
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(truncate(&combined, 2_000));
    }
    Ok(combined)
}

async fn list_android_devices(cancellation: &CancellationToken) -> Result<String, String> {
    let output = adb_output(&["devices", "-l"], cancellation).await?;
    let devices = output
        .lines()
        .skip_while(|line| !line.starts_with("List of devices attached"))
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            let state = fields.next()?;
            (state == "device").then(|| format!("{serial} (authorized)"))
        })
        .collect::<Vec<_>>();
    if devices.is_empty() {
        return Err("No authorized Android phone was found. Connect it by USB and approve the USB-debugging prompt.".to_owned());
    }
    Ok(format!("Authorized Android devices:\n{}", devices.join("\n")))
}

async fn install_android_apk(
    raw_apk_path: &str,
    requested_serial: Option<&str>,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let apk = PathBuf::from(raw_apk_path.trim());
    if !apk.is_absolute() || !apk.is_file() || !apk.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("apk")) {
        return Err("apkPath must be an existing absolute path to an .apk file.".to_owned());
    }
    let devices_output = adb_output(&["devices"], cancellation).await?;
    let devices = devices_output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            let state = fields.next()?;
            (state == "device").then_some(serial.to_owned())
        })
        .collect::<Vec<_>>();
    let serial = match requested_serial.map(str::trim).filter(|serial| !serial.is_empty()) {
        Some(serial) if devices.iter().any(|device| device == serial) => serial.to_owned(),
        Some(_) => return Err("The selected Android device is not currently authorized.".to_owned()),
        None if devices.len() == 1 => devices[0].clone(),
        None if devices.is_empty() => return Err("No authorized Android phone was found. Connect it by USB and approve USB debugging.".to_owned()),
        None => return Err("More than one Android phone is connected. Ask the user which device to use, then provide its serial.".to_owned()),
    };
    let apk_path = apk.canonicalize()
        .map_err(|_| "The APK path could not be resolved.".to_owned())?;
    let apk_path = apk_path.to_string_lossy().into_owned();
    let output = adb_output(&["-s", &serial, "install", "-r", &apk_path], cancellation).await?;
    if !output.contains("Success") {
        return Err(format!("ADB did not confirm installation:\n{}", truncate(&output, 2_000)));
    }
    Ok(format!("Installed {} on Android device {serial}.", apk.file_name().and_then(|name| name.to_str()).unwrap_or("APK")))
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

fn activity_detail(tool: &str, args: &Value) -> String {
    match tool {
        "list_files" | "project_graph_structure" => {
            "Recalling the saved project structure".to_owned()
        }
        "project_graph_query" => format!(
            "Searching project memory for {}",
            args["query"].as_str().unwrap_or("relevant code")
        ),
        "read_file" => format!(
            "Opening {} from project memory",
            args["path"].as_str().unwrap_or("file")
        ),
        "read_files" => format!(
            "Recalling {} project files from memory",
            args["paths"].as_array().map(Vec::len).unwrap_or(0)
        ),
        "git_status" => "Inspecting project changes".to_owned(),
        "browser_control" => browser_activity_detail(args),
        "replace_in_file" => format!("Editing {}", args["path"].as_str().unwrap_or("file")),
        "write_file" => format!("Writing {}", args["path"].as_str().unwrap_or("file")),
        "create_skill" => format!(
            "Creating skill {}",
            args["id"].as_str().unwrap_or("project-skill")
        ),
        "run_check" => format!(
            "Running {}",
            args["kind"].as_str().unwrap_or("project check")
        ),
        "list_android_devices" => "Checking USB-connected Android devices".to_owned(),
        "install_android_apk" => "Installing the requested APK on the connected Android phone".to_owned(),
        _ => format!("Running {tool}"),
    }
}

fn browser_activity_detail(args: &Value) -> String {
    match args["action"].as_str().unwrap_or("browser") {
        "open" => format!("Opening {} in Playwright", args["url"].as_str().unwrap_or("browser")),
        "goto" => format!("Navigating to {}", args["url"].as_str().unwrap_or("page")),
        "snapshot" => "Reading the current browser page".to_owned(),
        "click" => format!("Clicking {}", args["target"].as_str().unwrap_or("page element")),
        "fill" => format!("Filling {}", args["target"].as_str().unwrap_or("page field")),
        "type" => "Typing in the browser".to_owned(),
        "press" => format!("Pressing {}", args["value"].as_str().unwrap_or("a key")),
        "screenshot" => "Capturing the browser page".to_owned(),
        "back" => "Going back in the browser".to_owned(),
        "forward" => "Going forward in the browser".to_owned(),
        "reload" => "Reloading the browser page".to_owned(),
        "close" => "Closing the automated browser".to_owned(),
        _ => "Controlling the browser".to_owned(),
    }
}

fn activity_result_detail(tool: &str, state: &str, output: &str) -> String {
    if state == "failed" {
        return truncate(output, 240);
    }
    match tool {
        "list_files" | "project_graph_structure" => {
            format!(
                "Recalled {} indexed project entries",
                output.lines().count()
            )
        }
        "project_graph_query" => truncate(output.lines().next().unwrap_or(output), 240),
        "read_file" => format!("Read {} lines", output.lines().count()),
        "read_files" => truncate(output.lines().next().unwrap_or(output), 240),
        "git_status" => "Project changes inspected".to_owned(),
        "browser_control" | "replace_in_file" | "write_file" | "create_skill" | "run_check" | "install_android_apk" => {
            truncate(output.lines().next().unwrap_or(output), 240)
        }
        _ => "Completed".to_owned(),
    }
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
    fn refuses_paths_outside_the_workspace() {
        let root = std::env::temp_dir();
        assert!(safe_path(&root, "../secret.txt", true).is_err());
        assert!(safe_path(&root, "C:\\Windows\\win.ini", false).is_err());
    }

    #[test]
    fn creates_an_isolated_general_agent_workspace() {
        let root = canonical_workspace("").expect("general agent root should be available");
        assert!(root.is_dir());
        assert!(root.ends_with("hawk-code-general-agent"));
    }

    #[test]
    fn edit_permission_is_explicit() {
        assert!(require_edit_permission("ask").is_err());
        assert!(require_edit_permission("auto").is_ok());
        assert!(require_edit_permission("full").is_ok());
    }

    #[test]
    fn validates_project_skill_ids() {
        let root = std::env::temp_dir().join(format!("hawk-skill-{}", std::process::id()));
        fs::create_dir_all(&root).expect("temporary skill root should exist");
        let root = root
            .canonicalize()
            .expect("temporary skill root should be canonical");
        assert!(create_skill(&root, "bad id", "A skill", "Do the work").is_err());
        assert!(create_skill(&root, "review-code", "Review code", "Read files first").is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_project_files_in_batches_and_tracks_unique_capacity() {
        let root = std::env::temp_dir().join(format!("hawk-batch-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("temporary source root should exist");
        fs::write(root.join("src/one.rs"), "fn one() {}\n").expect("first fixture should exist");
        fs::write(root.join("src/two.rs"), "fn two() {}\n").expect("second fixture should exist");
        let root = root
            .canonicalize()
            .expect("temporary batch root should be canonical");
        let mut reviewed = HashSet::new();
        let cache = root.with_extension("hawk-graph-test.json");
        let mut graph =
            project_graph::sync_into(&root, cache.clone()).expect("test graph should be created");
        let output = read_files(
            &root,
            &json!({"paths": ["src/one.rs", "src/two.rs", "src/one.rs"]}),
            &mut reviewed,
            &mut graph,
        )
        .expect("valid batch should be readable");
        assert!(output.contains("Read 2 files in this batch"));
        assert!(output.contains("===== FILE: src/one.rs ====="));
        assert_eq!(reviewed.len(), 2);
        let _ = fs::remove_file(cache);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn enforces_the_500_file_review_capacity() {
        let mut reviewed = (0..MAX_PROJECT_FILES_PER_TURN)
            .map(|index| format!("src/file-{index}.rs"))
            .collect::<HashSet<_>>();
        assert!(record_file_read(&mut reviewed, "src/another.rs").is_err());
        assert_eq!(reviewed.len(), MAX_PROJECT_FILES_PER_TURN);
    }
}
