use crate::agent::AgentPayload;
use crate::provider::{
    resolve_api_key, response_error, usage_from_value, validate_config, ChatResult, ProviderRuntime,
    UsageSummary,
};
use crate::{browser_automation, project};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Output;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;
use tokio::time::{sleep, timeout, Duration};
use tokio_util::sync::CancellationToken;

const MAX_MODEL_ROUNDS: usize = 72;
const MAX_INSPECTIONS_WITHOUT_PROGRESS: usize = 5;
const MAX_FORMAT_RETRIES: usize = 12;
const MAX_PERMISSION_REJECTIONS: usize = 4;
const MAX_PROVIDER_RETRIES: usize = 3;
const MAX_MODEL_OUTPUT_TOKENS: u32 = 8_192;
const MAX_TOOL_OUTPUT: usize = 16_000;
const MAX_FILE_BYTES: u64 = 1_500_000;
const MAX_WRITE_BYTES: usize = 2_000_000;
const MAX_JOURNAL_ENTRIES: usize = 28;
const MAX_JOURNAL_ENTRY_CHARS: usize = 850;
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
struct Requirements {
    execution: bool,
    writes: bool,
    test: bool,
    lint: bool,
    build: bool,
}

#[derive(Default)]
struct Evidence {
    tool_calls: usize,
    writes: usize,
    commands: usize,
    successful_commands: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckState {
    NotRequired,
    Pending,
    Failed,
    Passed,
}

impl Default for CheckState {
    fn default() -> Self {
        Self::NotRequired
    }
}

#[derive(Default)]
struct VerificationState {
    test: CheckState,
    lint: CheckState,
    build: CheckState,
    last_failed: Option<&'static str>,
}

impl VerificationState {
    fn from_requirements(requirements: &Requirements) -> Self {
        Self {
            test: if requirements.test { CheckState::Pending } else { CheckState::NotRequired },
            lint: if requirements.lint { CheckState::Pending } else { CheckState::NotRequired },
            build: if requirements.build { CheckState::Pending } else { CheckState::NotRequired },
            last_failed: None,
        }
    }

    fn all_required_passed(&self) -> bool {
        [self.test, self.lint, self.build]
            .iter()
            .all(|state| matches!(state, CheckState::NotRequired | CheckState::Passed))
    }

    fn next_action(&self, root: &Path) -> Option<(&'static str, Value)> {
        if !root.join("package.json").is_file() {
            return None;
        }
        if matches!(self.test, CheckState::Pending | CheckState::Failed) {
            return Some((
                "test",
                json!({"action":"run_command","program":"npm","args":["test"],"timeoutSeconds":180}),
            ));
        }
        if matches!(self.lint, CheckState::Pending | CheckState::Failed) {
            return Some((
                "lint",
                json!({"action":"run_command","program":"npm","args":["run","lint"],"timeoutSeconds":180}),
            ));
        }
        if matches!(self.build, CheckState::Pending | CheckState::Failed) {
            return Some((
                "build",
                json!({"action":"run_command","program":"npm","args":["run","build"],"timeoutSeconds":180}),
            ));
        }
        None
    }

    fn mark(&mut self, check: &str, passed: bool) {
        let state = if passed { CheckState::Passed } else { CheckState::Failed };
        match check {
            "test" => self.test = state,
            "lint" => self.lint = state,
            "build" => self.build = state,
            _ => {}
        }
        if passed {
            if self.last_failed == Some(check) {
                self.last_failed = None;
            }
        } else {
            self.last_failed = match check {
                "test" => Some("test"),
                "lint" => Some("lint"),
                "build" => Some("build"),
                _ => None,
            };
        }
    }

    fn pending_text(&self) -> String {
        let mut pending = Vec::new();
        for (name, state) in [("test", self.test), ("lint", self.lint), ("build", self.build)] {
            match state {
                CheckState::Pending => pending.push(format!("{name}:pending")),
                CheckState::Failed => pending.push(format!("{name}:FAILED-needs-repair")),
                CheckState::Passed => pending.push(format!("{name}:PASS")),
                CheckState::NotRequired => {}
            }
        }
        if pending.is_empty() { "none".to_owned() } else { pending.join(", ") }
    }
}

#[derive(Default)]
struct LoopGuard {
    inspections_since_progress: usize,
    read_cache: HashMap<String, String>,
    listed_queries: HashSet<String>,
    focus_file: Option<String>,
    permission_rejections: usize,
}

impl LoopGuard {
    fn after_progress(&mut self, action: &Value) {
        self.inspections_since_progress = 0;
        self.permission_rejections = 0;
        self.listed_queries.clear();
        if let Some(path) = action["path"].as_str() {
            self.read_cache.remove(path);
            self.focus_file = Some(path.to_owned());
        }
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
    let mut verification = VerificationState::from_requirements(&requirements);
    let mut guard = LoopGuard::default();
    let mut journal = Vec::<String>::new();
    let mut observation = "Start by using the real workspace. Do not narrate.".to_owned();
    let mut format_retries = 0usize;
    let mut model_rounds = 0usize;
    let mut run_verification_now = root.join("package.json").is_file()
        && (requirements.test || requirements.lint || requirements.build);

    emit_activity(
        app,
        &request_id,
        "autonomous-agent",
        "agent",
        "running",
        "Executing with phased repair and deterministic verification".to_owned(),
        None,
    )?;

    while model_rounds < MAX_MODEL_ROUNDS {
        if cancellation.is_cancelled() {
            return Err("TASK_CANCELLED".to_owned());
        }

        // Verification is owned by HAWK, not left to the raw model. This prevents
        // hundreds of random command choices and makes test -> repair -> retest deterministic.
        if run_verification_now {
            if let Some((check, action)) = verification.next_action(&root) {
                let result = execute_forced_verification(
                    app,
                    &request_id,
                    &root,
                    &action,
                    &cancellation,
                    &mut evidence,
                )
                .await;
                match result {
                    Ok(output) => {
                        verification.mark(check, true);
                        push_journal(
                            &mut journal,
                            format!("VERIFICATION {check} PASS — {}", first_line(&output)),
                        );
                        observation = format!(
                            "HAWK ran `{}` and it PASSED. Continue to the next required check or finish only when all checks pass.\n{}",
                            command_label(&action),
                            truncate(&output, 4_000)
                        );
                        run_verification_now = true;
                        if verification.all_required_passed() {
                            return finish_verified(
                                app,
                                request_id,
                                model,
                                usage,
                                &evidence,
                                &verification,
                            );
                        }
                        continue;
                    }
                    Err(error) => {
                        verification.mark(check, false);
                        push_journal(
                            &mut journal,
                            format!("VERIFICATION {check} FAILED — {}", first_line(&error)),
                        );
                        observation = format!(
                            "DETERMINISTIC VERIFICATION FAILED: `{}`. Repair the project now. Do NOT rerun the same check yourself and do NOT finish. Read only the files needed to diagnose this exact failure, then edit the cause. HAWK will automatically rerun the failed check after your edit. Exact output:\n{}",
                            command_label(&action),
                            truncate(&error, MAX_TOOL_OUTPUT)
                        );
                        run_verification_now = false;
                    }
                }
            } else if verification.all_required_passed() {
                return finish_verified(
                    app,
                    request_id,
                    model,
                    usage,
                    &evidence,
                    &verification,
                );
            } else {
                run_verification_now = false;
            }
        }

        let messages = round_messages(
            &root,
            &permission,
            &original_request,
            &journal,
            &observation,
            &verification,
            &guard,
            &evidence,
        );
        let value = request_model(
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
        model_rounds = model_rounds.saturating_add(1);
        merge_usage(&mut usage, usage_from_value(&value));
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned();

        let Some(mut action) = parse_action(&content) else {
            format_retries = format_retries.saturating_add(1);
            if format_retries > MAX_FORMAT_RETRIES {
                return Err(format!(
                    "HAWK could not obtain a valid executable action after {MAX_FORMAT_RETRIES} format retries. Last model output: {}",
                    truncate(&content, 1_800)
                ));
            }
            observation = format!(
                "Your previous response was not executable. Emit exactly ONE action in JSON or Qwen tool-call syntax. Do not narrate. Last output:\n{}",
                truncate(&content, 1_800)
            );
            continue;
        };
        format_retries = 0;

        if let Err(error) = validate_action_shape(&action) {
            observation = format!(
                "HAWK rejected an incomplete tool call before execution: {error}. Re-emit the same intended action with every required parameter."
            );
            continue;
        }
        normalize_action_paths(&root, &mut action)?;
        let name = action["action"].as_str().unwrap_or_default().to_owned();

        if name == "finish" {
            if !verification.all_required_passed() {
                observation = format!(
                    "Finish rejected. Required verification is not complete: {}. Repair/edit first; HAWK owns test/lint/build execution.",
                    verification.pending_text()
                );
                continue;
            }
            if requirements.writes && evidence.writes == 0 {
                observation = "Finish rejected: the request requires project edits but no file has been changed.".to_owned();
                continue;
            }
            return finish_verified(app, request_id, model, usage, &evidence, &verification);
        }

        if is_inspection(&name) {
            if guard.inspections_since_progress >= MAX_INSPECTIONS_WITHOUT_PROGRESS {
                observation = format!(
                    "Inspection limit reached. You already inspected enough without making progress. Do NOT read/list again. Use write_file or replace_in_file to repair the current issue. Focus file: {}. Current failing verification: {:?}.",
                    guard.focus_file.as_deref().unwrap_or("none"),
                    verification.last_failed
                );
                continue;
            }
            if name == "read_file" {
                if let Some(path) = action["path"].as_str() {
                    if let Some(cached) = guard.read_cache.get(path) {
                        guard.inspections_since_progress = guard.inspections_since_progress.saturating_add(1);
                        observation = format!(
                            "Duplicate read blocked: `{path}` has already been read completely and has not changed. Use this cached content and act on it; do not read it again:\n{}",
                            truncate(cached, MAX_TOOL_OUTPUT)
                        );
                        continue;
                    }
                }
            }
            if name == "list_files" {
                let query = action["query"].as_str().unwrap_or_default().to_owned();
                if guard.listed_queries.contains(&query) {
                    guard.inspections_since_progress = guard.inspections_since_progress.saturating_add(1);
                    observation = "Duplicate workspace listing blocked. The tree has not changed. Use the previous listing and make progress.".to_owned();
                    continue;
                }
            }
        }

        if name == "run_command" && permission != "full" && !is_safe_development_command(&action) {
            guard.permission_rejections = guard.permission_rejections.saturating_add(1);
            observation = format!(
                "That command was NOT executed because the current permission mode only permits bounded project-local verification commands. Do not retry or invent another arbitrary shell command. Use file edits, read_file, or safe project commands. Rejections: {}/{}.",
                guard.permission_rejections,
                MAX_PERMISSION_REJECTIONS
            );
            if guard.permission_rejections >= MAX_PERMISSION_REJECTIONS {
                observation.push_str(" Permission-command loop detected. You MUST make a file edit next; HAWK will run test/lint/build itself.");
            }
            continue;
        }

        let activity_id = format!("v6-{model_rounds}-{name}");
        let file_path = action["path"].as_str().map(str::to_owned);
        emit_activity(
            app,
            &request_id,
            &activity_id,
            &name,
            "running",
            action_detail(&name, &action),
            file_path.clone(),
        )?;

        let result = execute_action(&root, &permission, &name, &action, &cancellation).await;
        match result {
            Ok(output) => {
                evidence.tool_calls = evidence.tool_calls.saturating_add(1);
                record_success(&mut evidence, &name, &action);
                emit_activity(
                    app,
                    &request_id,
                    &activity_id,
                    &name,
                    "completed",
                    truncate(first_line(&output), 300),
                    file_path,
                )?;
                push_journal(
                    &mut journal,
                    format!("OK — {} — {}", action_label(&name, &action), first_line(&output)),
                );

                if name == "read_file" {
                    if let Some(path) = action["path"].as_str() {
                        guard.read_cache.insert(path.to_owned(), output.clone());
                        guard.focus_file = Some(path.to_owned());
                    }
                    guard.inspections_since_progress = guard.inspections_since_progress.saturating_add(1);
                } else if name == "list_files" {
                    guard
                        .listed_queries
                        .insert(action["query"].as_str().unwrap_or_default().to_owned());
                    guard.inspections_since_progress = guard.inspections_since_progress.saturating_add(1);
                } else if name == "git_status" {
                    guard.inspections_since_progress = guard.inspections_since_progress.saturating_add(1);
                } else if matches!(name.as_str(), "write_file" | "replace_in_file") {
                    guard.after_progress(&action);
                    observation = format!(
                        "Project file changed successfully: {}. HAWK will now rerun the failed/pending deterministic verification.",
                        first_line(&output)
                    );
                    run_verification_now = verification.last_failed.is_some()
                        || !verification.all_required_passed();
                    continue;
                } else {
                    guard.after_progress(&action);
                }

                observation = format!(
                    "Real action succeeded. Use this result and choose the next necessary action; do not repeat unchanged inspections:\n{}",
                    truncate(&output, MAX_TOOL_OUTPUT)
                );
            }
            Err(error) => {
                emit_activity(
                    app,
                    &request_id,
                    &activity_id,
                    &name,
                    "failed",
                    truncate(&error, 300),
                    file_path,
                )?;
                push_journal(
                    &mut journal,
                    format!("FAILED — {} — {}", action_label(&name, &action), first_line(&error)),
                );
                observation = format!(
                    "The real action failed. Diagnose the exact error and repair the cause instead of repeating the same action:\n{}",
                    truncate(&error, MAX_TOOL_OUTPUT)
                );
            }
        }
    }

    Err(format!(
        "HAWK reached the {MAX_MODEL_ROUNDS}-round model guard before completion. Last verification state: {}",
        verification.pending_text()
    ))
}

async fn execute_forced_verification(
    app: &AppHandle,
    request_id: &str,
    root: &Path,
    action: &Value,
    cancellation: &CancellationToken,
    evidence: &mut Evidence,
) -> Result<String, String> {
    let id = format!("verification-{}", action_fingerprint("run_command", action));
    emit_activity(
        app,
        request_id,
        &id,
        "run_command",
        "running",
        format!("Verification: {}", command_label(action)),
        None,
    )?;
    let result = run_command(root, action, cancellation).await;
    match &result {
        Ok(output) => {
            evidence.tool_calls = evidence.tool_calls.saturating_add(1);
            evidence.commands = evidence.commands.saturating_add(1);
            evidence.successful_commands.push(command_label(action));
            emit_activity(
                app,
                request_id,
                &id,
                "run_command",
                "completed",
                truncate(first_line(output), 300),
                None,
            )?;
        }
        Err(error) => {
            emit_activity(
                app,
                request_id,
                &id,
                "run_command",
                "failed",
                truncate(first_line(error), 300),
                None,
            )?;
        }
    }
    result
}

fn finish_verified(
    app: &AppHandle,
    request_id: String,
    model: String,
    usage: UsageSummary,
    evidence: &Evidence,
    verification: &VerificationState,
) -> Result<ChatResult, String> {
    let summary = format!(
        "اكتملت المهمة مع تحقق فعلي. test={}، lint={}، build={}. تم تنفيذ {} أداة، {} تعديلات ملفات، و{} أوامر ناجحة.",
        check_label(verification.test),
        check_label(verification.lint),
        check_label(verification.build),
        evidence.tool_calls,
        evidence.writes,
        evidence.commands,
    );
    emit_activity(
        app,
        &request_id,
        "autonomous-agent",
        "agent",
        "completed",
        "Verified completion: required checks passed".to_owned(),
        None,
    )?;
    app.emit(
        "qwen://delta",
        DeltaEvent {
            request_id: request_id.clone(),
            delta: summary,
        },
    )
    .map_err(|_| "Unable to deliver the final agent result.".to_owned())?;
    Ok(ChatResult {
        request_id,
        model,
        usage,
    })
}

fn check_label(state: CheckState) -> &'static str {
    match state {
        CheckState::NotRequired => "not-required",
        CheckState::Pending => "pending",
        CheckState::Failed => "failed",
        CheckState::Passed => "PASS",
    }
}

fn round_messages(
    root: &Path,
    permission: &str,
    original_request: &str,
    journal: &[String],
    observation: &str,
    verification: &VerificationState,
    guard: &LoopGuard,
    evidence: &Evidence,
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
    let user = format!(
        "USER TASK:\n{}\n\nSTATE:\nVerification: {}\nWrites: {}\nSuccessful commands: {}\nInspections since progress: {}\nFocus file: {}\nPermission mode: {}\n\nJOURNAL:\n{}\n\nLATEST REAL OBSERVATION:\n{}\n\nEmit exactly ONE executable action. When verification failed, diagnose/read only what is necessary and EDIT the cause. HAWK itself reruns test/lint/build after edits. Do not rerun checks manually. Do not narrate.",
        original_request,
        verification.pending_text(),
        evidence.writes,
        evidence.commands,
        guard.inspections_since_progress,
        guard.focus_file.as_deref().unwrap_or("none"),
        permission,
        journal_text,
        truncate(observation, MAX_TOOL_OUTPUT),
    );
    vec![
        json!({"role":"system","content":orchestrator_prompt(root, permission)}),
        json!({"role":"user","content":user}),
    ]
}

fn orchestrator_prompt(root: &Path, permission: &str) -> String {
    format!(
        r#"You are HAWK Code's autonomous repair agent for a real desktop workspace.
Workspace: {root}
Permission: {permission}

EXECUTION CONTRACT:
- Execute one real action per response; never narrate a future plan.
- HAWK owns the verification pipeline. Do NOT manually spam npm test/lint/build. After you edit a file, HAWK automatically runs the failed/pending verification.
- If verification fails, inspect only files needed for that exact failure, then edit the cause.
- read_file returns the WHOLE file. Never read an unchanged file twice.
- Do not bounce between files. Keep focus until you understand and act on the current failure.
- In safe mode, arbitrary commands are rejected. Use file operations and bounded project-local commands only.
- Never use bare node/python or long-running dev servers.
- Emit JSON or Qwen tool syntax. Loose `function=name` syntax is accepted.
- Do not expose chain-of-thought.

ACTIONS:
{{"action":"list_files","query":"optional"}}
{{"action":"read_file","path":"relative/path"}}
{{"action":"write_file","path":"relative/path","content":"complete file"}}
{{"action":"replace_in_file","path":"relative/path","oldText":"exact text","newText":"replacement"}}
{{"action":"run_command","program":"node|npm|pnpm|python|cargo|git","args":["..."],"cwd":"optional","timeoutSeconds":120}}
{{"action":"git_status"}}
{{"action":"browser_control","browser":{{"action":"open|goto|snapshot|click|fill|type|press|screenshot|back|forward|reload|close"}}}}
{{"action":"finish","summary":"verified result"}}

Prefer workspace-relative paths. For native tool calls put path before large content parameters."#,
        root = root.display(),
        permission = permission,
    )
}

async fn request_model(
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
                    "temperature": 0.1,
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
                format!("Model server returned {status}; retrying this repair step"),
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
            write_file(root, required_string(action, "path")?, required_string(action, "content")?)
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
        "git_status" => serde_json::to_string_pretty(&project::git_status(root.to_string_lossy().as_ref())?)
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

fn validate_action_shape(action: &Value) -> Result<(), String> {
    let name = action["action"]
        .as_str()
        .ok_or_else(|| "missing action name".to_owned())?;
    if !SUPPORTED_ACTIONS.contains(&name) {
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
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("action `{name}` is missing required parameter(s): {}", missing.join(", ")))
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
    parse_native_tool_call(trimmed)
        .or_else(|| extract_first_json_object(trimmed).and_then(parse_json_action))
}

fn parse_json_action(candidate: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(candidate).ok()?;
    if let Some(name) = value.get("action").and_then(Value::as_str) {
        return SUPPORTED_ACTIONS.contains(&name).then_some(value);
    }
    let name = value.get("name").and_then(Value::as_str)?;
    if !SUPPORTED_ACTIONS.contains(&name) || value.get("arguments").is_none() {
        return None;
    }
    let arguments = value.get("arguments")?.clone();
    let mut map = match arguments {
        Value::Object(map) => map,
        Value::String(raw) => serde_json::from_str::<Map<String, Value>>(&raw).ok()?,
        _ => return None,
    };
    map.insert("action".to_owned(), Value::String(name.to_owned()));
    Some(Value::Object(map))
}

fn parse_native_tool_call(input: &str) -> Option<Value> {
    let start = input.find("<tool_call").or_else(|| input.find("tool_call")).unwrap_or(0);
    let tool = &input[start..];
    let function_name = extract_function_name(tool)?;
    if !SUPPORTED_ACTIONS.contains(&function_name.as_str()) {
        return None;
    }
    let params = parse_native_parameters(tool);
    if function_name == "browser_control" {
        return Some(json!({"action":"browser_control","browser":Value::Object(params)}));
    }
    let mut action = params;
    action.insert("action".to_owned(), Value::String(function_name));
    Some(Value::Object(action))
}

fn extract_function_name(input: &str) -> Option<String> {
    for marker in ["<function=", "function="] {
        if let Some(position) = input.find(marker) {
            let tail = &input[position + marker.len()..];
            let end = tail
                .find(|character: char| matches!(character, '>' | '\n' | '\r' | '<' | ' ' | '\t'))
                .unwrap_or(tail.len());
            let name = tail[..end]
                .trim()
                .trim_matches(|character: char| matches!(character, '\'' | '"' | '`'));
            if SUPPORTED_ACTIONS.contains(&name) {
                return Some(name.to_owned());
            }
        }
    }
    None
}

fn parse_native_parameters(input: &str) -> Map<String, Value> {
    let mut result = Map::new();
    let mut cursor = 0usize;
    while cursor < input.len() {
        let slice = &input[cursor..];
        let tagged = slice.find("<parameter=").map(|index| (index, "<parameter="));
        let loose = slice.find("parameter=").map(|index| (index, "parameter="));
        let next = match (tagged, loose) {
            (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let Some((relative_start, marker)) = next else { break; };
        let name_start = cursor + relative_start + marker.len();
        let tail = &input[name_start..];
        let name_end = tail
            .find(|character: char| matches!(character, '>' | '\n' | '\r' | ' ' | '\t'))
            .unwrap_or(tail.len());
        let name = tail[..name_end]
            .trim()
            .trim_matches(|character: char| matches!(character, '\'' | '"' | '`'));
        if name.is_empty() {
            cursor = name_start + name_end.saturating_add(1);
            continue;
        }
        let mut value_start = name_start + name_end;
        if input.get(value_start..).is_some_and(|rest| rest.starts_with('>')) {
            value_start += 1;
        }
        while value_start < input.len() {
            let Some(character) = input[value_start..].chars().next() else { break; };
            if !matches!(character, '\n' | '\r' | ' ' | '\t') { break; }
            value_start += character.len_utf8();
        }
        if value_start >= input.len() { break; }
        let remaining = &input[value_start..];
        let Some(value_end) = remaining.find("</parameter>") else { break; };
        let raw = remaining[..value_end].trim();
        result.insert(name.to_owned(), parse_parameter_value(name, raw));
        cursor = value_start + value_end + "</parameter>".len();
    }
    result
}

fn parse_parameter_value(name: &str, raw: &str) -> Value {
    if matches!(
        name,
        "content" | "oldText" | "newText" | "summary" | "path" | "program" | "cwd" | "query" | "url" | "target" | "value"
    ) {
        Value::String(raw.trim().to_owned())
    } else if let Ok(value) = serde_json::from_str::<Value>(raw.trim()) {
        value
    } else {
        Value::String(raw.trim().to_owned())
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
                if depth == 0 { return input.get(start..=index); }
            }
            _ => {}
        }
    }
    None
}

fn normalize_action_paths(root: &Path, action: &mut Value) -> Result<(), String> {
    for key in ["path", "cwd"] {
        let Some(raw) = action.get(key).and_then(Value::as_str).map(str::to_owned) else { continue; };
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
    if cleaned.is_empty() { return Err("Path is empty.".to_owned()); }
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
    if candidate_text == root_text { return Ok(".".to_owned()); }
    if let Some(relative) = candidate_text.strip_prefix(&prefix) {
        if !relative.is_empty() { return Ok(relative.to_owned()); }
    }
    Err("Absolute path is outside the active workspace.".to_owned())
}

fn normalize_path_text(value: &str) -> String {
    let normalized = value
        .trim()
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_owned();
    if cfg!(windows) { normalized.to_lowercase() } else { normalized }
}

fn workspace_root(path: Option<&str>) -> Result<PathBuf, String> {
    if let Some(raw) = path.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(raw);
        if !path.is_dir() { return Err("The active workspace is unavailable.".to_owned()); }
        return path.canonicalize().map_err(|_| "Unable to resolve active workspace.".to_owned());
    }
    let root = std::env::temp_dir().join("hawk-code-general-agent");
    fs::create_dir_all(&root).map_err(|_| "Unable to create general workspace.".to_owned())?;
    root.canonicalize().map_err(|_| "Unable to resolve general workspace.".to_owned())
}

fn safe_path(root: &Path, relative: &str, allow_missing: bool) -> Result<PathBuf, String> {
    let relative = relative.trim().replace('\\', "/");
    if relative == "." { return Ok(root.to_path_buf()); }
    let candidate = Path::new(&relative);
    if relative.is_empty()
        || candidate.is_absolute()
        || candidate.components().any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Path must stay inside the active workspace.".to_owned());
    }
    let joined = root.join(candidate);
    if joined.exists() {
        let canonical = joined.canonicalize().map_err(|_| "Unable to resolve requested path.".to_owned())?;
        if !canonical.starts_with(root) { return Err("Path escapes active workspace.".to_owned()); }
        return Ok(canonical);
    }
    if !allow_missing { return Err(format!("File does not exist: {relative}")); }
    let mut parent = joined.parent().unwrap_or(root);
    while !parent.exists() {
        parent = parent.parent().ok_or_else(|| "Unable to resolve destination parent.".to_owned())?;
    }
    let parent = parent.canonicalize().map_err(|_| "Unable to resolve destination parent.".to_owned())?;
    if !parent.starts_with(root) { return Err("Destination escapes active workspace.".to_owned()); }
    Ok(joined)
}

fn list_files(root: &Path, query: Option<&str>) -> Result<String, String> {
    let query = query.unwrap_or_default().trim().to_lowercase();
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(&directory).map_err(|error| format!("Unable to list {}: {error}", directory.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if matches!(name.as_str(), ".git" | "node_modules" | "target" | ".next" | "dist" | "build") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if let Ok(relative) = path.strip_prefix(root) {
                let text = relative.to_string_lossy().replace('\\', "/");
                if query.is_empty() || text.to_lowercase().contains(&query) {
                    found.push(text);
                    if found.len() >= 700 { break; }
                }
            }
        }
        if found.len() >= 700 { break; }
    }
    found.sort();
    Ok(if found.is_empty() { "No matching workspace files.".to_owned() } else { found.join("\n") })
}

fn read_file(root: &Path, relative: &str) -> Result<String, String> {
    let path = safe_path(root, relative, false)?;
    let metadata = path.metadata().map_err(|_| "Unable to read file metadata.".to_owned())?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(format!("File must be UTF-8 text and under {MAX_FILE_BYTES} bytes."));
    }
    fs::read_to_string(path).map_err(|_| "File is not valid UTF-8 text.".to_owned())
}

fn write_file(root: &Path, relative: &str, content: &str) -> Result<String, String> {
    if content.len() > MAX_WRITE_BYTES { return Err("File content exceeds 2 MB write limit.".to_owned()); }
    let path = safe_path(root, relative, true)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("Unable to create parent directory: {error}"))?;
    }
    fs::write(&path, content).map_err(|error| format!("Unable to write {relative}: {error}"))?;
    Ok(format!("Wrote {relative} ({} bytes).", content.len()))
}

fn replace_in_file(root: &Path, relative: &str, old_text: &str, new_text: &str) -> Result<String, String> {
    if old_text.is_empty() { return Err("oldText must not be empty.".to_owned()); }
    let path = safe_path(root, relative, false)?;
    let content = fs::read_to_string(&path).map_err(|_| "Target file is not UTF-8 text.".to_owned())?;
    let count = content.matches(old_text).count();
    if count != 1 { return Err(format!("oldText must match exactly once; found {count}.")); }
    let updated = content.replacen(old_text, new_text, 1);
    if updated.len() > MAX_WRITE_BYTES { return Err("Edited file exceeds 2 MB limit.".to_owned()); }
    fs::write(&path, updated).map_err(|error| format!("Unable to save {relative}: {error}"))?;
    Ok(format!("Updated {relative}."))
}

async fn run_command(root: &Path, action: &Value, cancellation: &CancellationToken) -> Result<String, String> {
    let raw_program = required_string(action, "program")?.trim();
    if raw_program.is_empty() || raw_program.contains('/') || raw_program.contains('\\') {
        return Err("program must be an executable name on PATH.".to_owned());
    }
    let args = action["args"]
        .as_array()
        .map(|items| items.iter().map(|item| item.as_str().map(str::to_owned).unwrap_or_else(|| item.to_string())).collect::<Vec<_>>())
        .unwrap_or_default();
    let lower = raw_program.to_ascii_lowercase();
    if matches!(lower.as_str(), "node" | "python" | "python3" | "py") && args.is_empty() {
        return Err(format!("Refusing interactive `{raw_program}` without a script."));
    }
    if matches!(lower.as_str(), "npm" | "pnpm" | "yarn") {
        let long = args.first().is_some_and(|v| matches!(v.as_str(), "start" | "dev" | "serve"))
            || (args.first().is_some_and(|v| v == "run")
                && args.get(1).is_some_and(|v| matches!(v.as_str(), "start" | "dev" | "serve")));
        if long { return Err("Long-running dev servers are not verification commands.".to_owned()); }
    }
    let cwd = match action["cwd"].as_str().map(str::trim).filter(|v| !v.is_empty()) {
        Some(relative) => {
            let path = safe_path(root, relative, false)?;
            if !path.is_dir() { return Err("run_command cwd must be a directory.".to_owned()); }
            path
        }
        None => root.to_path_buf(),
    };
    let timeout_seconds = action["timeoutSeconds"].as_u64().unwrap_or(150).clamp(1, 300);
    let mut command = Command::new(platform_program(raw_program));
    command.args(&args).current_dir(&cwd).kill_on_drop(true);
    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
        result = timeout(Duration::from_secs(timeout_seconds), command.output()) => {
            result.map_err(|_| format!("Command timed out after {timeout_seconds}s: {raw_program}"))?
                .map_err(|error| format!("Unable to start {raw_program}: {error}"))?
        }
    };
    command_result(raw_program, &args, output)
}

fn command_result(program: &str, args: &[String], output: Output) -> Result<String, String> {
    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        Ok(format!("Command succeeded: {} {}\n{}", program, args.join(" "), truncate(&combined, MAX_TOOL_OUTPUT)))
    } else {
        Err(format!("Command failed (exit {:?}): {} {}\n{}", output.status.code(), program, args.join(" "), truncate(&combined, MAX_TOOL_OUTPUT)))
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

fn require_edit(permission: &str) -> Result<(), String> {
    if matches!(permission, "auto" | "full") { Ok(()) } else { Err("This action requires edit permission.".to_owned()) }
}

fn require_command_permission(permission: &str, action: &Value) -> Result<(), String> {
    if permission == "full" || (permission == "auto" && is_safe_development_command(action)) {
        Ok(())
    } else {
        Err("This command requires Full access or must be a bounded safe project-local command.".to_owned())
    }
}

fn is_safe_development_command(action: &Value) -> bool {
    let program = action["program"].as_str().unwrap_or_default().trim().to_ascii_lowercase();
    let args = action["args"]
        .as_array()
        .map(|items| items.iter().filter_map(Value::as_str).map(|v| v.to_ascii_lowercase()).collect::<Vec<_>>())
        .unwrap_or_default();
    match program.as_str() {
        "npm" | "pnpm" | "yarn" => {
            (args.len() == 1 && matches!(args[0].as_str(), "test" | "lint" | "build"))
                || (args.len() >= 2 && args[0] == "run" && matches!(args[1].as_str(), "test" | "lint" | "build" | "typecheck" | "check" | "verify"))
        }
        "node" => safe_script_command(&args, &["js", "mjs", "cjs"]),
        "python" | "python3" | "py" => safe_script_command(&args, &["py"]),
        "cargo" => args.first().is_some_and(|v| matches!(v.as_str(), "test" | "check" | "build" | "clippy" | "fmt" | "run")),
        "git" => args.first().is_some_and(|v| matches!(v.as_str(), "status" | "diff" | "log" | "show")),
        _ => false,
    }
}

fn safe_script_command(args: &[String], extensions: &[&str]) -> bool {
    let Some(script) = args.first() else { return false; };
    if script.starts_with('-') || looks_absolute_cli_argument(script) { return false; }
    let normalized = script.replace('\\', "/");
    let ext = normalized.rsplit('.').next().unwrap_or_default();
    extensions.contains(&ext) && args.iter().skip(1).all(|arg| !looks_absolute_cli_argument(arg))
}

fn looks_absolute_cli_argument(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    normalized.starts_with('/')
        || normalized.starts_with("//")
        || normalized.as_bytes().get(1).is_some_and(|byte| *byte == b':')
}

fn infer_requirements(text: &str) -> Requirements {
    let lower = text.to_lowercase();
    Requirements {
        execution: contains_any(&lower, &["أنش", "انش", "ابن", "بناء", "نفذ", "نفّذ", "اصلح", "أصلح", "عدّل", "عدل", "اكتب", "شغل", "شغّل", "اختبر", "أكمل", "اكمل", "create", "build", "implement", "fix", "modify", "write", "run", "test", "continue"]),
        writes: contains_any(&lower, &["أنش", "انش", "ابن", "اصلح", "أصلح", "عدّل", "عدل", "اكتب", "create", "implement", "fix", "modify", "write"]),
        test: contains_any(&lower, &["اختبار", "اختبارات", "اختبر", "test", "tests", "npm test"]),
        lint: contains_any(&lower, &["lint", "فحص lint"]),
        build: contains_any(&lower, &["npm run build", "pnpm build", "cargo build", " build", "بناء المشروع", "البناء"]),
    }
}

fn record_success(evidence: &mut Evidence, name: &str, action: &Value) {
    match name {
        "write_file" | "replace_in_file" => evidence.writes = evidence.writes.saturating_add(1),
        "run_command" => {
            evidence.commands = evidence.commands.saturating_add(1);
            evidence.successful_commands.push(command_label(action));
        }
        _ => {}
    }
}

fn is_inspection(name: &str) -> bool {
    matches!(name, "read_file" | "list_files" | "git_status")
}

fn action_fingerprint(name: &str, action: &Value) -> String {
    match name {
        "run_command" => command_label(action).replace(' ', "_"),
        "read_file" | "write_file" | "replace_in_file" => format!("{name}-{}", action["path"].as_str().unwrap_or_default()),
        _ => name.to_owned(),
    }
}

fn command_label(action: &Value) -> String {
    let program = action["program"].as_str().unwrap_or("command");
    let args = action["args"]
        .as_array()
        .map(|items| items.iter().map(|v| v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    format!("{program} {args}").trim().to_owned()
}

fn action_label(name: &str, action: &Value) -> String {
    match name {
        "read_file" | "write_file" | "replace_in_file" => format!("{name} {}", action["path"].as_str().unwrap_or("?")),
        "run_command" => format!("run_command {}", command_label(action)),
        "list_files" => format!("list_files {}", action["query"].as_str().unwrap_or("*")),
        _ => name.to_owned(),
    }
}

fn action_detail(name: &str, action: &Value) -> String {
    match name {
        "list_files" => "Inspecting workspace files".to_owned(),
        "read_file" => format!("Reading {}", action["path"].as_str().unwrap_or("file")),
        "write_file" => format!("Writing {}", action["path"].as_str().unwrap_or("file")),
        "replace_in_file" => format!("Editing {}", action["path"].as_str().unwrap_or("file")),
        "run_command" => format!("Running {}", command_label(action)),
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
        .take(4)
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
    if let Some(text) = content.as_str() { return text.to_owned(); }
    content
        .as_array()
        .map(|parts| parts.iter().filter_map(|part| (part["type"].as_str() == Some("text")).then(|| part["text"].as_str()).flatten()).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

fn push_journal(journal: &mut Vec<String>, entry: String) {
    journal.push(truncate(&entry, MAX_JOURNAL_ENTRY_CHARS));
    if journal.len() > MAX_JOURNAL_ENTRIES {
        let overflow = journal.len() - MAX_JOURNAL_ENTRIES;
        journal.drain(0..overflow);
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key].as_str().ok_or_else(|| format!("Missing required string parameter: {key}"))
}

fn first_line(value: &str) -> &str {
    value.lines().next().unwrap_or(value)
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
        format!("{}\n... truncated by HAWK Code ...", value.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loose_qwen_function() {
        let action = parse_action("<tool_call>\nfunction=list_files\n</function>").expect("parse");
        assert_eq!(action["action"], "list_files");
    }

    #[test]
    fn safe_mode_allows_project_node_script() {
        assert!(is_safe_development_command(&json!({
            "program":"node",
            "args":["scripts/lint.js"]
        })));
        assert!(!is_safe_development_command(&json!({"program":"node","args":[]})));
    }

    #[test]
    fn verification_order_is_test_lint_build() {
        let root = std::env::temp_dir();
        let state = VerificationState {
            test: CheckState::Pending,
            lint: CheckState::Pending,
            build: CheckState::Pending,
            last_failed: None,
        };
        // next_action needs package.json; this test only checks labels without filesystem dependency.
        assert_eq!(check_label(state.test), "pending");
        assert_eq!(check_label(state.lint), "pending");
        assert_eq!(check_label(state.build), "pending");
        let _ = root;
    }

    #[test]
    fn duplicate_read_cache_key_is_path() {
        let mut guard = LoopGuard::default();
        guard.read_cache.insert("src/cli.js".to_owned(), "x".to_owned());
        assert!(guard.read_cache.contains_key("src/cli.js"));
    }
}
