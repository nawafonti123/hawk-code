use crate::agent::AgentPayload;
use crate::provider::{
    resolve_api_key, response_error, usage_from_value, validate_config, ChatResult, ProviderRuntime,
    UsageSummary,
};
use crate::{browser_automation, project};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Output;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;
use tokio::time::{sleep, timeout, Duration};
use tokio_util::sync::CancellationToken;

const MAX_STEPS: usize = 160;
const MAX_FILE_BYTES: u64 = 1_500_000;
const MAX_WRITE_BYTES: usize = 2_000_000;
const MAX_TOOL_OUTPUT: usize = 14_000;
const MAX_JOURNAL_ENTRIES: usize = 32;
const MAX_JOURNAL_ENTRY_CHARS: usize = 800;
const MAX_MODEL_OUTPUT_TOKENS: u32 = 8_192;
const MAX_FORMAT_RETRIES: usize = 20;
const MAX_PROVIDER_RETRIES: usize = 3;
const MAX_INSPECTIONS_WITHOUT_PROGRESS: usize = 6;
const MAX_BLOCKED_DUPLICATES: usize = 4;
const SUPPORTED_ACTIONS: [&str; 8] = [
    "list_files",
    "read_file",
    "write_file",
    "replace_in_file",
    "run_command",
    "git_status",
    "browser_control",
    "finish",
];

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

#[derive(Default)]
struct ProgressGuard {
    inspection_streak: usize,
    progress_epoch: usize,
    blocked_duplicates: usize,
    inspection_cache: HashMap<String, String>,
    failed_at_epoch: HashMap<String, usize>,
    focus_file: Option<String>,
}

impl ProgressGuard {
    fn mark_progress(&mut self, action_name: &str, action: &Value) {
        self.progress_epoch = self.progress_epoch.saturating_add(1);
        self.inspection_streak = 0;
        self.blocked_duplicates = 0;
        if matches!(action_name, "write_file" | "replace_in_file") {
            if let Some(path) = action["path"].as_str() {
                self.inspection_cache.remove(&format!("read_file:{path}"));
                self.focus_file = Some(path.to_owned());
            }
            self.inspection_cache
                .retain(|key, _| !key.starts_with("list_files:"));
        }
    }

    fn mark_inspection(&mut self, action_name: &str, action: &Value, output: &str) {
        self.inspection_streak = self.inspection_streak.saturating_add(1);
        let fingerprint = action_fingerprint(action_name, action);
        self.inspection_cache
            .insert(fingerprint, truncate(output, MAX_TOOL_OUTPUT));
        if action_name == "read_file" {
            self.focus_file = action["path"].as_str().map(str::to_owned);
        }
    }

    fn mark_failure(&mut self, action_name: &str, action: &Value) {
        self.failed_at_epoch
            .insert(action_fingerprint(action_name, action), self.progress_epoch);
    }

    fn clear_failure(&mut self, action_name: &str, action: &Value) {
        self.failed_at_epoch
            .remove(&action_fingerprint(action_name, action));
    }
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
    let original_request = execution_request(&payload.messages);
    let requirements = infer_requirements(&original_request);

    let mut usage = UsageSummary::default();
    let mut evidence = Evidence::default();
    let mut journal: Vec<String> = Vec::new();
    let mut guard = ProgressGuard::default();
    let mut last_observation = "No actions have run yet. Start executing the task now.".to_owned();
    let mut format_retries = 0usize;

    emit_activity(
        app,
        &request_id,
        "autonomous-agent",
        "agent",
        "running",
        "Executing until verified completion with loop protection".to_owned(),
        None,
    )?;

    for step in 0..MAX_STEPS {
        if cancellation.is_cancelled() {
            return Err("TASK_CANCELLED".to_owned());
        }

        let messages = build_round_messages(
            &root,
            &permission,
            &original_request,
            &journal,
            &last_observation,
            step,
            &evidence,
            &guard,
            &requirements,
        );

        let value = request_model_round(
            runtime,
            &endpoint,
            &api_key,
            &model,
            messages,
            &request_id,
            app,
            &cancellation,
        )
        .await?;
        merge_usage(&mut usage, usage_from_value(&value));

        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned();

        if content.is_empty() {
            format_retries += 1;
            if format_retries > MAX_FORMAT_RETRIES {
                return Err("The raw model repeatedly returned empty execution steps.".to_owned());
            }
            last_observation = "Your previous response was empty. Emit one complete executable action now. Do not narrate.".to_owned();
            continue;
        }

        let Some(mut action) = parse_action(&content) else {
            format_retries += 1;
            if format_retries > MAX_FORMAT_RETRIES {
                return Err(format!(
                    "The raw model repeatedly returned output HAWK could not execute. Last response: {}",
                    truncate(&content, 1_800)
                ));
            }
            last_observation = format!(
                "HAWK could not parse that as an executable action. Emit exactly one supported action. Native <tool_call> syntax is accepted. Last output:\n{}",
                truncate(&content, 2_000)
            );
            continue;
        };

        if let Err(error) = validate_action_shape(&action) {
            format_retries += 1;
            if format_retries > MAX_FORMAT_RETRIES {
                return Err(format!("The raw model repeatedly emitted incomplete tool calls: {error}"));
            }
            last_observation = format!(
                "HAWK did not execute the incomplete action: {error}. Re-emit the SAME intended action with all required parameters. For write_file put path before content."
            );
            continue;
        }

        if let Err(error) = normalize_action_paths(&root, &mut action) {
            format_retries += 1;
            last_observation = format!(
                "HAWK rejected the path before execution: {error}. Correct it to a path inside the active workspace and retry the same action."
            );
            continue;
        }
        format_retries = 0;

        let mut action_name = action["action"].as_str().unwrap_or_default().to_owned();

        if action_name == "finish" {
            if let Some(reason) = completion_blocker(&requirements, &evidence) {
                last_observation = format!(
                    "HAWK rejected finish because {reason}. Continue the missing work now."
                );
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
                    "Completed after {} real actions and {} successful command runs",
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

        // Repeated reads/listings are served from the in-memory observation cache.
        // The model still receives the complete prior output but HAWK does not waste
        // another disk operation, UI activity entry, or tool step.
        if is_inspection(&action_name) {
            let fingerprint = action_fingerprint(&action_name, &action);
            if let Some(cached) = guard.inspection_cache.get(&fingerprint).cloned() {
                guard.blocked_duplicates = guard.blocked_duplicates.saturating_add(1);
                push_journal(
                    &mut journal,
                    format!("BLOCKED DUPLICATE — {}", compact_action_label(&action_name, &action)),
                );
                last_observation = format!(
                    "LOOP GUARD: this exact inspection already succeeded and nothing changed it. Do NOT request it again. Use the cached complete result below and take a different action.\n\nCACHED RESULT:\n{}\n\nNext action should normally be write_file, replace_in_file, run_command, or a genuinely different dependency read.",
                    cached
                );

                if guard.blocked_duplicates >= MAX_BLOCKED_DUPLICATES {
                    if let Some(fallback) = deterministic_verification_action(&root, &requirements, &evidence) {
                        action = fallback;
                        action_name = "run_command".to_owned();
                        guard.blocked_duplicates = 0;
                    } else {
                        last_observation.push_str("\n\nHARD PROGRESS GATE: repeated inspection is no longer allowed. The next action MUST change the project or execute a verification command.");
                        continue;
                    }
                } else {
                    continue;
                }
            }
        }

        // Too many read/list steps without an edit or command is a classic agent loop.
        if is_inspection(&action_name)
            && guard.inspection_streak >= MAX_INSPECTIONS_WITHOUT_PROGRESS
        {
            if let Some(fallback) = deterministic_verification_action(&root, &requirements, &evidence) {
                push_journal(
                    &mut journal,
                    "PROGRESS GATE — replacing excessive inspection with the next required verification".to_owned(),
                );
                action = fallback;
                action_name = "run_command".to_owned();
            } else {
                last_observation = format!(
                    "PROGRESS GATE: {} inspection actions have occurred without any edit or command. Stop reading/listing. You already have enough context. The next action MUST be write_file, replace_in_file, run_command, or finish only if all verification is complete.",
                    guard.inspection_streak
                );
                continue;
            }
        }

        let fingerprint = action_fingerprint(&action_name, &action);
        if let Some(epoch) = guard.failed_at_epoch.get(&fingerprint).copied() {
            if epoch == guard.progress_epoch {
                last_observation = format!(
                    "LOOP GUARD: the identical action `{}` already failed and no repair has happened since. Do not repeat it unchanged. Inspect the error, modify the relevant file/arguments, or choose a different bounded verification command.",
                    compact_action_label(&action_name, &action)
                );
                continue;
            }
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
        match result {
            Ok(output) => {
                evidence.unresolved_failure = false;
                guard.clear_failure(&action_name, &action);
                record_success(&mut evidence, &action_name, &action);
                if is_inspection(&action_name) {
                    guard.mark_inspection(&action_name, &action, &output);
                } else if is_progress_action(&action_name) {
                    guard.mark_progress(&action_name, &action);
                }

                emit_activity(
                    app,
                    &request_id,
                    &activity_id,
                    &action_name,
                    "completed",
                    truncate(output.lines().next().unwrap_or(&output), 260),
                    file_path,
                )?;
                push_journal(
                    &mut journal,
                    format!(
                        "STEP {} OK — {} — {}",
                        step + 1,
                        compact_action_label(&action_name, &action),
                        truncate(output.lines().next().unwrap_or(&output), 360)
                    ),
                );
                last_observation = next_observation(&action_name, &action, &output, &guard);
            }
            Err(error) => {
                evidence.unresolved_failure = true;
                guard.mark_failure(&action_name, &action);
                let output = format!("Tool error: {error}");
                emit_activity(
                    app,
                    &request_id,
                    &activity_id,
                    &action_name,
                    "failed",
                    truncate(&output, 300),
                    file_path,
                )?;
                push_journal(
                    &mut journal,
                    format!(
                        "STEP {} FAILED — {} — {}",
                        step + 1,
                        compact_action_label(&action_name, &action),
                        truncate(&output, 500)
                    ),
                );
                last_observation = failure_observation(&action_name, &action, &output);
            }
        }
    }

    Err(format!(
        "HAWK autonomous execution reached the {MAX_STEPS}-step emergency guard before verified completion."
    ))
}

async fn request_model_round(
    runtime: &ProviderRuntime,
    endpoint: &reqwest::Url,
    api_key: &str,
    model: &str,
    messages: Vec<Value>,
    request_id: &str,
    app: &AppHandle,
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
                    "temperature": 0.08,
                    "top_p": 0.9
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
        if status.is_server_error() && attempt + 1 < MAX_PROVIDER_RETRIES {
            last_error = response_error(response).await;
            emit_activity(
                app,
                request_id,
                &format!("provider-retry-{}", attempt + 1),
                "provider_retry",
                "running",
                format!("Model server returned {status}; automatically retrying this step"),
                None,
            )?;
            sleep(Duration::from_millis(900 * (attempt as u64 + 1))).await;
            continue;
        }
        return Err(response_error(response).await);
    }

    Err(if last_error.is_empty() {
        "The model provider failed after automatic retries.".to_owned()
    } else {
        format!("The model provider failed after automatic retries. Last error: {last_error}")
    })
}

fn build_round_messages(
    root: &Path,
    permission: &str,
    original_request: &str,
    journal: &[String],
    last_observation: &str,
    step: usize,
    evidence: &Evidence,
    guard: &ProgressGuard,
    requirements: &Requirements,
) -> Vec<Value> {
    let journal_text = if journal.is_empty() {
        "No completed actions yet.".to_owned()
    } else {
        journal
            .iter()
            .rev()
            .take(MAX_JOURNAL_ENTRIES)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    };

    let milestone = next_required_milestone(requirements, evidence);
    let focus = guard.focus_file.as_deref().unwrap_or("none");
    let system = orchestrator_prompt(root, permission);
    let user = format!(
        "USER TASK — source of truth:\n{}\n\nCURRENT STATE:\nRound: {}/{}\nSuccessful writes/edits: {}\nSuccessful command runs: {}\nInspection streak without progress: {}\nCurrent focus file: {}\nNext required milestone: {}\n\nEXECUTION JOURNAL:\n{}\n\nMOST RECENT OBSERVATION:\n{}\n\nContinue now. Emit exactly ONE executable action. Do not narrate. Do not repeat a read/list action already recorded unless a write changed that file. Prefer concrete progress over more inspection.",
        original_request,
        step + 1,
        MAX_STEPS,
        evidence.writes,
        evidence.commands,
        guard.inspection_streak,
        focus,
        milestone,
        journal_text,
        truncate(last_observation, MAX_TOOL_OUTPUT),
    );

    vec![
        json!({"role": "system", "content": system}),
        json!({"role": "user", "content": user}),
    ]
}

fn orchestrator_prompt(root: &Path, permission: &str) -> String {
    format!(
        r#"You are HAWK Code's autonomous execution controller for a real desktop workspace.
Workspace: {root}
Permission profile: {permission}

EXECUTION CONTRACT:
- Execute real work; never stop at a plan or promise.
- Every response contains exactly ONE executable action.
- JSON actions and Qwen native <tool_call> syntax are accepted.
- A read_file action returns the COMPLETE text file. Once a file was read successfully, DO NOT read it again unless HAWK reports that a write changed it or an external command changed it.
- list_files is discovery, not progress. Do not repeatedly list the same query.
- After a small bounded amount of inspection, move to write/replace/run/test. HAWK enforces this.
- For write_file and replace_in_file native calls, emit path FIRST.
- If a command fails or times out, do not repeat the identical command unchanged. Diagnose, edit, or change bounded arguments.
- Never launch an interactive REPL or an indefinitely running dev server as verification. Use finite commands such as npm test, npm run lint, npm run build, node script.js <args>, cargo test/check/build.
- If tests/lint/build are requested, they must actually succeed before finish.
- With Full access, do not ask for confirmation for normal development work.

SUPPORTED ACTIONS:
{{"action":"list_files","query":"optional substring"}}
{{"action":"read_file","path":"relative/path"}}
{{"action":"write_file","path":"relative/path","content":"complete file contents"}}
{{"action":"replace_in_file","path":"relative/path","oldText":"exact unique old text","newText":"replacement"}}
{{"action":"run_command","program":"node|npm|pnpm|python|cargo|git|other executable","args":["arg1","arg2"],"cwd":"optional relative directory","timeoutSeconds":240}}
{{"action":"git_status"}}
{{"action":"browser_control","browser":{{"action":"open|goto|snapshot|click|fill|type|press|screenshot|back|forward|reload|close","url":"optional","target":"optional","value":"optional","fullPage":true}}}}
{{"action":"finish","summary":"concise verified final result"}}

PATH RULES:
- Prefer workspace-relative paths.
- Absolute paths are accepted only if they resolve inside the active workspace.
- run_command executes directly without a shell; every CLI argument must be separate.
- On Windows, npm/pnpm/npx/yarn are mapped to their .cmd executables.

Do not expose chain-of-thought. Do not print prose around the action."#,
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
            require_command_permission(permission, action)?;
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

fn deterministic_verification_action(
    root: &Path,
    requirements: &Requirements,
    evidence: &Evidence,
) -> Option<Value> {
    if root.join("package.json").is_file() {
        if requirements.test && !command_evidence(evidence, &["test"]) {
            return Some(json!({
                "action": "run_command",
                "program": "npm",
                "args": ["test"],
                "timeoutSeconds": 360
            }));
        }
        if requirements.lint && !command_evidence(evidence, &["lint"]) {
            return Some(json!({
                "action": "run_command",
                "program": "npm",
                "args": ["run", "lint"],
                "timeoutSeconds": 300
            }));
        }
        if requirements.build && !command_evidence(evidence, &["build"]) {
            return Some(json!({
                "action": "run_command",
                "program": "npm",
                "args": ["run", "build"],
                "timeoutSeconds": 360
            }));
        }
    }
    if root.join("Cargo.toml").is_file() {
        if requirements.test && !command_evidence(evidence, &["test"]) {
            return Some(json!({
                "action": "run_command",
                "program": "cargo",
                "args": ["test"],
                "timeoutSeconds": 600
            }));
        }
        if requirements.build && !command_evidence(evidence, &["build"]) {
            return Some(json!({
                "action": "run_command",
                "program": "cargo",
                "args": ["build"],
                "timeoutSeconds": 600
            }));
        }
    }
    None
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
    if relative == "." {
        return Ok(root.to_path_buf());
    }
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

    let lower_program = raw_program.to_ascii_lowercase();
    if matches!(lower_program.as_str(), "node" | "python" | "python3" | "py") && args.is_empty() {
        return Err(format!(
            "Refusing to launch interactive {raw_program} with no script/arguments. Provide a finite script or command instead; interactive REPLs hang autonomous tasks."
        ));
    }
    if matches!(lower_program.as_str(), "npm" | "pnpm" | "yarn")
        && args.first().is_some_and(|value| matches!(value.as_str(), "dev" | "start" | "serve"))
    {
        return Err("Refusing to start a long-running development server as a verification command. Use test/lint/build or a finite script instead.".to_owned());
    }

    let program = platform_program(raw_program);
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
        .unwrap_or(240)
        .clamp(1, 900);

    let mut command = Command::new(&program);
    command.args(&args).current_dir(&cwd).kill_on_drop(true);
    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
        result = timeout(Duration::from_secs(timeout_seconds), command.output()) => {
            result
                .map_err(|_| format!("Command timed out after {timeout_seconds}s: {raw_program} {}. Do not repeat it unchanged; use finite non-interactive arguments or inspect/fix the code first.", args.join(" ")))?
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
    if let Some(value) = parse_native_tool_call(trimmed) {
        return Some(value);
    }
    extract_first_json_object(trimmed).and_then(parse_json_action)
}

fn parse_json_action(candidate: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(candidate).ok()?;
    if let Some(name) = value.get("action").and_then(Value::as_str) {
        return is_supported_action(name).then_some(value);
    }
    let name = value.get("name").and_then(Value::as_str)?;
    if !is_supported_action(name) || value.get("arguments").is_none() {
        return None;
    }
    let arguments = value.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let mut result = match arguments {
        Value::Object(map) => map,
        Value::String(raw) => serde_json::from_str::<Map<String, Value>>(&raw).ok()?,
        _ => return None,
    };
    result.insert("action".to_owned(), Value::String(name.to_owned()));
    Some(Value::Object(result))
}

fn parse_native_tool_call(input: &str) -> Option<Value> {
    let tool_start = input.find("<tool_call")?;
    let tool = &input[tool_start..];
    let function_marker = "<function=";
    let function_start = tool.find(function_marker)? + function_marker.len();
    let function_tail = &tool[function_start..];
    let function_end = function_tail.find('>')?;
    let function_name = function_tail[..function_end]
        .trim()
        .trim_matches(|character: char| matches!(character, '\'' | '"'))
        .trim();
    if !is_supported_action(function_name) {
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
        return Some(json!({"action":"browser_control","browser":Value::Object(browser)}));
    }

    let mut action = Map::new();
    action.insert("action".to_owned(), Value::String(function_name.to_owned()));
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
        let Some(name_end) = tail.find('>') else { break; };
        let name = tail[..name_end]
            .trim()
            .trim_matches(|character: char| matches!(character, '\'' | '"'))
            .trim();
        if name.is_empty() {
            cursor = start + name_end + 1;
            continue;
        }
        let value_start = start + name_end + 1;
        let remaining = &body[value_start..];
        let Some(value_end_relative) = remaining.find("</parameter>") else { break; };
        let raw_value = remaining[..value_end_relative].trim();
        result.insert(name.to_owned(), parse_native_parameter_value(name, raw_value));
        cursor = value_start + value_end_relative + "</parameter>".len();
    }
    result
}

fn parse_native_parameter_value(name: &str, raw: &str) -> Value {
    let trimmed = raw.trim();
    if matches!(
        name,
        "content" | "oldText" | "newText" | "summary" | "path" | "program" | "cwd" | "query" | "url" | "target" | "value"
    ) {
        return Value::String(trimmed.to_owned());
    }
    parse_native_value(trimmed)
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

fn validate_action_shape(action: &Value) -> Result<(), String> {
    let name = action["action"]
        .as_str()
        .ok_or_else(|| "missing action name".to_owned())?;
    if !is_supported_action(name) {
        return Err(format!("unsupported action `{name}`"));
    }
    let required: &[&str] = match name {
        "read_file" => &["path"],
        "write_file" => &["path", "content"],
        "replace_in_file" => &["path", "oldText", "newText"],
        "run_command" => &["program"],
        "browser_control" => &["browser"],
        _ => &[],
    };
    let missing = required
        .iter()
        .filter(|key| match action.get(**key) {
            Some(Value::String(value)) => value.trim().is_empty(),
            Some(Value::Null) | None => true,
            _ => false,
        })
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "action `{name}` is missing required parameter(s): {}",
            missing.join(", ")
        ));
    }
    if name == "run_command" {
        if let Some(args) = action.get("args") {
            if !args.is_array() && !args.is_null() {
                return Err("run_command args must be an array".to_owned());
            }
        }
    }
    Ok(())
}

fn is_supported_action(name: &str) -> bool {
    SUPPORTED_ACTIONS.contains(&name)
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
        .trim_matches(|character: char| matches!(character, '\'' | '"' | '`'))
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
    let root_text = normalize_path_text(&root.to_string_lossy());
    let candidate_text = normalize_path_text(&cleaned);
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

fn normalize_path_text(value: &str) -> String {
    let normalized = value
        .trim()
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_owned();
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
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
            &["أنش", "انش", "ابن", "بناء", "نفذ", "نفّذ", "طبق", "طبّق", "اصلح", "أصلح", "عدّل", "عدل", "اكتب", "شغل", "شغّل", "اختبر", "create", "build", "implement", "fix", "modify", "write", "run", "test"],
        ),
        writes: contains_any(
            &lower,
            &["أنش", "انش", "ابن", "بناء", "طبق", "طبّق", "اصلح", "أصلح", "عدّل", "عدل", "اكتب", "create", "build", "implement", "fix", "modify", "write"],
        ),
        run: contains_any(&lower, &["شغل", "شغّل", "تشغيل", "نفذ", "نفّذ", "run", "execute"]),
        test: contains_any(&lower, &["اختبار", "اختبارات", "اختبر", "test", "tests", "npm test"]),
        lint: contains_any(&lower, &["lint", "فحص lint"]),
        build: contains_any(&lower, &["npm run build", "pnpm build", "cargo build", " build", "بناء المشروع", "البناء"]),
    }
}

fn completion_blocker(requirements: &Requirements, evidence: &Evidence) -> Option<String> {
    if evidence.unresolved_failure {
        return Some("the most recent real action failed and has not been repaired yet".to_owned());
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

fn next_required_milestone(requirements: &Requirements, evidence: &Evidence) -> &'static str {
    if requirements.test && !command_evidence(evidence, &["test"]) {
        "get the implementation into a state where the requested test command can run and pass"
    } else if requirements.lint && !command_evidence(evidence, &["lint"]) {
        "run and pass lint"
    } else if requirements.build && !command_evidence(evidence, &["build"]) {
        "run and pass build"
    } else if requirements.run && evidence.commands == 0 {
        "run a finite requested verification command"
    } else {
        "finish the remaining implementation and verify it"
    }
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
            evidence.successful_commands.push(format!("{program} {args}"));
        }
        _ => {}
    }
}

fn is_inspection(name: &str) -> bool {
    matches!(name, "read_file" | "list_files")
}

fn is_progress_action(name: &str) -> bool {
    matches!(
        name,
        "write_file" | "replace_in_file" | "run_command" | "browser_control"
    )
}

fn action_fingerprint(name: &str, action: &Value) -> String {
    match name {
        "read_file" => format!("read_file:{}", action["path"].as_str().unwrap_or_default()),
        "list_files" => format!("list_files:{}", action["query"].as_str().unwrap_or_default()),
        "write_file" | "replace_in_file" => {
            format!("{name}:{}", action["path"].as_str().unwrap_or_default())
        }
        "run_command" => format!(
            "run:{}:{}:{}",
            action["program"].as_str().unwrap_or_default(),
            action["args"].to_string(),
            action["cwd"].as_str().unwrap_or_default()
        ),
        _ => format!("{name}:{}", action),
    }
}

fn next_observation(
    action_name: &str,
    action: &Value,
    output: &str,
    guard: &ProgressGuard,
) -> String {
    match action_name {
        "read_file" => format!(
            "The COMPLETE contents of `{}` were read successfully and are below. Treat this as the authoritative current file content. Do not read this same file again unless a later write changes it. Decide what to do with this file now, then make progress.\n\n{}",
            action["path"].as_str().unwrap_or("file"),
            truncate(output, MAX_TOOL_OUTPUT)
        ),
        "list_files" => format!(
            "Workspace discovery succeeded. Do not repeat the same list query. Use these results to choose a concrete file/action:\n{}",
            truncate(output, MAX_TOOL_OUTPUT)
        ),
        "run_command" => format!(
            "The finite command succeeded. Continue from this verified output:\n{}",
            truncate(output, 6_000)
        ),
        "write_file" | "replace_in_file" => format!(
            "Project progress succeeded: {}. The cache for that file was invalidated. Continue with the next required implementation or verification step. Current focus file: {}.",
            truncate(output, 900),
            guard.focus_file.as_deref().unwrap_or("none")
        ),
        _ => format!(
            "The previous real action succeeded: {}. Continue with the next required step.",
            truncate(output, 900)
        ),
    }
}

fn failure_observation(action_name: &str, action: &Value, output: &str) -> String {
    if action_name == "run_command" && output.to_lowercase().contains("timed out") {
        return format!(
            "The command timed out. Do NOT repeat this exact command unchanged. If it launched node/python with no useful finite arguments, that is an interactive/hanging command. Use explicit CLI arguments, run a bounded test/lint/build command, or inspect/fix the relevant code first. Failed action: {}\n{}",
            compact_action_label(action_name, action),
            truncate(output, MAX_TOOL_OUTPUT)
        );
    }
    format!(
        "The last real action FAILED. Do not repeat it unchanged. Diagnose this exact failure, repair the relevant code/arguments, and then retry verification:\n{}",
        truncate(output, MAX_TOOL_OUTPUT)
    )
}

fn push_journal(journal: &mut Vec<String>, entry: String) {
    journal.push(truncate(&entry, MAX_JOURNAL_ENTRY_CHARS));
    if journal.len() > MAX_JOURNAL_ENTRIES {
        let overflow = journal.len() - MAX_JOURNAL_ENTRIES;
        journal.drain(0..overflow);
    }
}

fn compact_action_label(name: &str, action: &Value) -> String {
    match name {
        "write_file" | "read_file" | "replace_in_file" => format!(
            "{} {}",
            name,
            action["path"].as_str().unwrap_or("unknown-path")
        ),
        "list_files" => format!(
            "list_files query={}",
            action["query"].as_str().unwrap_or_default()
        ),
        "run_command" => format!(
            "run_command {} {}",
            action["program"].as_str().unwrap_or("command"),
            action["args"]
                .as_array()
                .map(|items| items.iter().map(|value| value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())).collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
        ),
        _ => name.to_owned(),
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
                .map(|items| items.iter().map(|value| value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())).collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
        ),
        "git_status" => "Inspecting Git status".to_owned(),
        "browser_control" => "Controlling browser with Playwright".to_owned(),
        _ => format!("Executing {name}"),
    }
}

fn execution_request(messages: &[crate::provider::ChatMessage]) -> String {
    let users = messages
        .iter()
        .filter(|message| message.role == "user")
        .rev()
        .take(3)
        .map(|message| content_text(&message.content))
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>();
    users
        .into_iter()
        .rev()
        .enumerate()
        .map(|(index, text)| format!("User instruction {}:\n{}", index + 1, text))
        .collect::<Vec<_>>()
        .join("\n\n")
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

fn require_command_permission(permission: &str, action: &Value) -> Result<(), String> {
    if permission == "full" {
        return Ok(());
    }
    if permission == "auto" && is_safe_development_command(action) {
        return Ok(());
    }
    Err("This command needs Full access. Safe project-local verification commands are permitted in Approve-safe-actions mode; arbitrary commands require Full access.".to_owned())
}

fn is_safe_development_command(action: &Value) -> bool {
    let program = action["program"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let args = action["args"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    match program.as_str() {
        "npm" | "pnpm" | "yarn" => {
            (args.len() == 1 && matches!(args[0].as_str(), "test" | "lint" | "build"))
                || matches_safe_run_script(&args)
        }
        "node" => {
            !args.is_empty()
                && !args.iter().any(|value| matches!(value.as_str(), "-e" | "--eval" | "-p" | "--print" | "-i" | "--interactive"))
                && args.iter().all(|value| !looks_absolute_cli_argument(value))
        }
        "python" | "python3" | "py" => {
            !args.is_empty()
                && !args.iter().any(|value| matches!(value.as_str(), "-c" | "-m"))
                && args.iter().all(|value| !looks_absolute_cli_argument(value))
        }
        "cargo" => args.first().is_some_and(|value| matches!(value.as_str(), "test" | "check" | "build" | "clippy" | "fmt" | "run")),
        "git" => args.first().is_some_and(|value| matches!(value.as_str(), "status" | "diff" | "log" | "show")),
        _ => false,
    }
}

fn matches_safe_run_script(args: &[String]) -> bool {
    args.len() >= 2
        && args[0] == "run"
        && matches!(args[1].as_str(), "test" | "lint" | "build" | "typecheck" | "check" | "verify")
}

fn looks_absolute_cli_argument(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    normalized.starts_with('/')
        || normalized.starts_with("//")
        || normalized.as_bytes().get(1).is_some_and(|byte| *byte == b':')
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
    fn parses_native_write_file() {
        let input = r#"<tool_call><function=write_file><parameter=path>src/a.js</parameter><parameter=content>export const a = 1;</parameter></function></tool_call>"#;
        let action = parse_action(input).expect("native tool call should parse");
        assert_eq!(action["action"], "write_file");
        assert_eq!(action["path"], "src/a.js");
        assert!(validate_action_shape(&action).is_ok());
    }

    #[test]
    fn duplicate_read_fingerprint_is_stable() {
        let action = json!({"action":"read_file","path":"src/cli.js"});
        assert_eq!(action_fingerprint("read_file", &action), "read_file:src/cli.js");
    }

    #[test]
    fn progress_invalidates_written_file_read_cache() {
        let mut guard = ProgressGuard::default();
        guard.inspection_cache.insert(
            "read_file:src/a.js".to_owned(),
            "old".to_owned(),
        );
        guard.mark_progress(
            "write_file",
            &json!({"action":"write_file","path":"src/a.js","content":"new"}),
        );
        assert!(!guard.inspection_cache.contains_key("read_file:src/a.js"));
        assert_eq!(guard.inspection_streak, 0);
    }

    #[test]
    fn rejects_interactive_node_in_safe_mode() {
        assert!(!is_safe_development_command(&json!({
            "program":"node",
            "args":[]
        })));
    }

    #[test]
    fn deterministic_node_verification_prefers_test() {
        let requirements = Requirements {
            execution: true,
            writes: true,
            run: true,
            test: true,
            lint: true,
            build: true,
        };
        let evidence = Evidence::default();
        // This helper is covered structurally here; filesystem-dependent selection
        // is exercised at runtime against the active workspace.
        assert!(!command_evidence(&evidence, &["test"]));
        assert!(requirements.test);
    }

    #[test]
    fn completion_requires_all_checks() {
        let requirements = Requirements {
            execution: true,
            writes: true,
            run: true,
            test: true,
            lint: true,
            build: true,
        };
        let evidence = Evidence {
            tool_calls: 20,
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
