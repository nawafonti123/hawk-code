mod model;
mod protocol;
mod runtime;
mod state;

use crate::agent::AgentPayload;
use crate::provider::{resolve_api_key, validate_config, ChatResult, ProviderRuntime, UsageSummary};
use model::next_action;
use protocol::AgentAction;
use runtime::WorkspaceRuntime;
use serde::Serialize;
use serde_json::{json, Value};
use state::{
    action_to_json, node_verification_action, CheckState, EventRecorder, Phase, Requirements,
    RunState,
};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

const MAX_MODEL_ROUNDS: usize = 72;
const MAX_INSPECTIONS_WITHOUT_PROGRESS: usize = 5;
const MAX_DUPLICATE_INSPECTIONS: usize = 3;

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
    provider: &ProviderRuntime,
    payload: AgentPayload,
    cancellation: CancellationToken,
) -> Result<ChatResult, String> {
    if payload.messages.is_empty() {
        return Err("The agent conversation is empty.".to_owned());
    }

    let workspace =
        WorkspaceRuntime::new(payload.workspace_path.as_deref(), &payload.permission_profile)?;
    let endpoint = validate_config(&payload.config)?;
    let (api_key, _) = resolve_api_key()?;
    let model_name = payload.config.model.clone();
    let request_id = payload.request_id.clone();
    let original_request = execution_request(&payload.messages);
    let requirements = Requirements::infer(&original_request);
    let mut run_state = RunState::new(requirements);
    let recorder = EventRecorder::new(&request_id);
    let mut usage = UsageSummary::default();
    let mut duplicate_inspections = 0usize;

    recorder.text(run_state.phase, "task", &original_request);
    emit_activity(
        app,
        &request_id,
        "agent-v7",
        "agent",
        "running",
        "HAWK v7: deterministic controller started".to_owned(),
        None,
    )?;

    if workspace.root().join("package.json").is_file()
        && run_state.verification.next_check().is_some()
    {
        run_state.phase = Phase::Verify;
    } else {
        run_state.phase = Phase::Inspect;
    }

    loop {
        if cancellation.is_cancelled() {
            recorder.text(run_state.phase, "cancelled", "TASK_CANCELLED");
            return Err("TASK_CANCELLED".to_owned());
        }

        if run_state.phase == Phase::Verify {
            if drive_verification(
                app,
                &request_id,
                &workspace,
                &mut run_state,
                &recorder,
                &cancellation,
            )
            .await?
            {
                return finish_run(app, request_id, model_name, usage, &mut run_state, &recorder);
            }
            continue;
        }

        if run_state.model_rounds >= MAX_MODEL_ROUNDS {
            let detail = format!(
                "HAWK v7 reached its {MAX_MODEL_ROUNDS}-round model budget. Phase={}, checks={}",
                run_state.phase.as_str(),
                run_state.verification.pending_summary()
            );
            recorder.text(run_state.phase, "budget_exhausted", &detail);
            return Err(detail);
        }

        let messages = build_messages(
            workspace.root(),
            &payload.permission_profile,
            &original_request,
            &run_state,
        );
        let (raw_action, round_usage, raw_content) = next_action(
            provider,
            &endpoint,
            &api_key,
            &model_name,
            messages,
            &cancellation,
        )
        .await?;
        merge_usage(&mut usage, round_usage);
        run_state.model_rounds = run_state.model_rounds.saturating_add(1);
        recorder.text(
            run_state.phase,
            "model_output",
            &truncate(&raw_content, 3_000),
        );

        let action = match workspace.normalize(raw_action) {
            Ok(action) => action,
            Err(error) => {
                run_state.last_observation = format!(
                    "HAWK rejected that action before execution because its path escaped or did not resolve inside the workspace: {error}. Correct the path and choose the next action."
                );
                recorder.text(
                    run_state.phase,
                    "policy_reject",
                    &run_state.last_observation,
                );
                continue;
            }
        };
        recorder.json(run_state.phase, "action", &action_to_json(&action));

        if let AgentAction::Finish { .. } = &action {
            if let Some(reason) = run_state.completion_blocker() {
                run_state.last_observation = format!(
                    "HAWK rejected finish: {reason}. Continue real execution instead of narrating."
                );
                if !run_state.verification.all_passed() {
                    run_state.phase = Phase::Verify;
                } else {
                    run_state.phase = Phase::Act;
                }
                continue;
            }
            return finish_run(app, request_id, model_name, usage, &mut run_state, &recorder);
        }

        if action.is_inspection() {
            if let AgentAction::ReadFile { path } = &action {
                if let Some(cached) = run_state.guard.cached_read(path) {
                    duplicate_inspections = duplicate_inspections.saturating_add(1);
                    run_state.last_observation = format!(
                        "HAWK blocked a duplicate read of unchanged `{path}`. The previous complete observation is still valid:\n{}\n\nUse it now; edit, search a dependency, or run verification instead of reading this file again.",
                        truncate(cached, 16_000)
                    );
                    if duplicate_inspections >= MAX_DUPLICATE_INSPECTIONS {
                        run_state.phase = Phase::Act;
                    }
                    continue;
                }
            } else if run_state
                .guard
                .seen_inspections
                .contains(&action.fingerprint())
            {
                duplicate_inspections = duplicate_inspections.saturating_add(1);
                run_state.last_observation = format!(
                    "HAWK blocked duplicate inspection `{}` because the workspace has not changed. Make progress with an edit or verification command.",
                    action.label()
                );
                if duplicate_inspections >= MAX_DUPLICATE_INSPECTIONS {
                    run_state.phase = Phase::Act;
                }
                continue;
            }

            if run_state.guard.inspections_since_progress >= MAX_INSPECTIONS_WITHOUT_PROGRESS {
                run_state.phase = Phase::Act;
                run_state.last_observation = format!(
                    "Inspection budget exhausted after {MAX_INSPECTIONS_WITHOUT_PROGRESS} non-progress steps. Do not read/list again until you edit code or run a bounded verification command. Focus file: {}.",
                    run_state.guard.focus_file.as_deref().unwrap_or("none")
                );
                continue;
            }
        }

        if matches!(action, AgentAction::RunCommand { .. })
            && run_state.guard.repeated_failed_command(&action)
        {
            run_state.phase = Phase::Repair;
            run_state.last_observation = format!(
                "HAWK blocked the identical failed command `{}` because no file changed since it failed. Repair the cause first.",
                action.label()
            );
            continue;
        }

        let activity_id = format!("v7-{}-{}", run_state.model_rounds, action.name());
        emit_activity(
            app,
            &request_id,
            &activity_id,
            action.name(),
            "running",
            action_detail(&action),
            action.path().map(str::to_owned),
        )?;

        let result = workspace.execute(&action, &cancellation).await;
        run_state.evidence.tool_actions = run_state.evidence.tool_actions.saturating_add(1);

        match result {
            Ok(output) => {
                duplicate_inspections = 0;
                record_success(&mut run_state, &action);
                if action.is_inspection() {
                    run_state.guard.remember_inspection(&action, &output);
                    run_state.phase = Phase::Inspect;
                } else if action.is_progress() {
                    let is_edit = matches!(
                        action,
                        AgentAction::WriteFile { .. } | AgentAction::ReplaceInFile { .. }
                    );
                    let had_failed_check = run_state.verification.last_failed.is_some();
                    if is_edit {
                        invalidate_passed_verification(&mut run_state);
                    }
                    run_state.guard.mark_progress(&action);
                    if is_edit && had_failed_check {
                        run_state.phase = Phase::Verify;
                    } else {
                        run_state.phase = Phase::Act;
                    }
                }

                let observation = format!(
                    "Action succeeded: {}\n{}",
                    action.label(),
                    truncate(&output, 20_000)
                );
                run_state.last_observation = observation.clone();
                run_state.push_journal(format!("OK — {}", action.label()));
                recorder.text(run_state.phase, "observation", &observation);
                emit_activity(
                    app,
                    &request_id,
                    &activity_id,
                    action.name(),
                    "completed",
                    truncate(output.lines().next().unwrap_or(&output), 300),
                    action.path().map(str::to_owned),
                )?;
            }
            Err(error) => {
                if matches!(action, AgentAction::RunCommand { .. }) {
                    run_state.guard.mark_failed_command(&action);
                }
                run_state.phase = Phase::Repair;
                run_state.last_observation = format!(
                    "Action FAILED: {}\n{}\nFix the exact cause. Do not retry the identical failing command until you change the project or choose a materially different diagnostic action.",
                    action.label(),
                    truncate(&error, 20_000)
                );
                run_state.push_journal(format!(
                    "FAILED — {} — {}",
                    action.label(),
                    truncate(&error, 800)
                ));
                recorder.text(run_state.phase, "error", &run_state.last_observation);
                emit_activity(
                    app,
                    &request_id,
                    &activity_id,
                    action.name(),
                    "failed",
                    truncate(&error, 300),
                    action.path().map(str::to_owned),
                )?;
            }
        }
    }
}

async fn drive_verification(
    app: &AppHandle,
    request_id: &str,
    workspace: &WorkspaceRuntime,
    state: &mut RunState,
    recorder: &EventRecorder,
    cancellation: &CancellationToken,
) -> Result<bool, String> {
    let Some(check) = state.verification.next_check() else {
        if state.completion_blocker().is_none() {
            state.phase = Phase::Complete;
            return Ok(true);
        }
        state.phase = Phase::Act;
        state.last_observation = "All requested verification currently passes. Continue implementing the requested project changes before completion.".to_owned();
        return Ok(false);
    };

    if !workspace.root().join("package.json").is_file() {
        state.phase = Phase::Act;
        state.last_observation = "Verification is requested, but package.json does not exist yet. Build the project structure first; HAWK will run verification automatically afterward.".to_owned();
        return Ok(false);
    }

    let action = node_verification_action(check)
        .ok_or_else(|| format!("Unsupported verification check: {check}"))?;
    let activity_id = format!("v7-verify-{check}");
    emit_activity(
        app,
        request_id,
        &activity_id,
        "run_command",
        "running",
        format!("Verification: {check}"),
        None,
    )?;
    recorder.json(
        state.phase,
        "verification_action",
        &action_to_json(&action),
    );
    state.evidence.tool_actions = state.evidence.tool_actions.saturating_add(1);

    match workspace.execute(&action, cancellation).await {
        Ok(output) => {
            state.verification.mark(check, true);
            state.evidence.commands = state.evidence.commands.saturating_add(1);
            state.evidence.successful_commands.push(action.label());
            state.guard.mark_progress(&action);
            state.last_observation = format!(
                "Verification `{check}` PASSED.\n{}",
                truncate(&output, 12_000)
            );
            state.push_journal(format!("VERIFY PASS — {check}"));
            recorder.text(
                state.phase,
                "verification_pass",
                &state.last_observation,
            );
            emit_activity(
                app,
                request_id,
                &activity_id,
                "run_command",
                "completed",
                format!("{check}: PASS"),
                None,
            )?;

            if state.verification.all_passed() {
                if state.completion_blocker().is_none() {
                    state.phase = Phase::Complete;
                    return Ok(true);
                }
                state.phase = Phase::Act;
                state.last_observation = "Baseline verification passes, but the requested task still requires real project changes. Implement them now; HAWK will re-run checks after edits.".to_owned();
            }
            Ok(false)
        }
        Err(error) => {
            state.verification.mark(check, false);
            state.phase = Phase::Repair;
            state.last_observation = format!(
                "Deterministic verification `{check}` FAILED. This is the exact runtime observation:\n{}\n\nRepair the project. After a real edit, HAWK will automatically retry `{check}`.",
                truncate(&error, 20_000)
            );
            state.push_journal(format!(
                "VERIFY FAILED — {check} — {}",
                truncate(&error, 900)
            ));
            recorder.text(
                state.phase,
                "verification_failed",
                &state.last_observation,
            );
            emit_activity(
                app,
                request_id,
                &activity_id,
                "run_command",
                "failed",
                truncate(&error, 320),
                None,
            )?;
            Ok(false)
        }
    }
}

fn invalidate_passed_verification(state: &mut RunState) {
    if state.verification.test == CheckState::Passed {
        state.verification.test = CheckState::Pending;
    }
    if state.verification.lint == CheckState::Passed {
        state.verification.lint = CheckState::Pending;
    }
    if state.verification.build == CheckState::Passed {
        state.verification.build = CheckState::Pending;
    }
}

fn build_messages(
    root: &std::path::Path,
    permission: &str,
    original_request: &str,
    state: &RunState,
) -> Vec<Value> {
    let journal = if state.journal.is_empty() {
        "No successful or failed runtime actions recorded yet.".to_owned()
    } else {
        state.journal.join("\n")
    };
    let system = format!(
        "You are the coding decision model inside HAWK Code v7. HAWK—not you—owns execution, permissions, loop detection, verification order, and completion. Your only job is to select ONE next action that advances the user task. The output is constrained to HAWK's JSON action schema. Never narrate. Never claim an action happened until the runtime observation says it happened.\n\nWorkspace: {}\nPermission profile: {}\n\nUse search_text before repeatedly opening files. A read_file observation represents the file view available for that step; do not reread an unchanged file. Keep focus on the current file until you have a reason to edit it, verify it, or inspect a direct dependency. If verification failed, repair the cause rather than retrying the same command. Do not use bare interactive node/python. Do not start dev/start/serve servers in the autonomous loop.",
        root.display(),
        permission
    );
    let user = format!(
        "USER TASK — source of truth:\n{}\n\nCONTROLLER STATE:\nphase={}\nmodel_round={}/{}\nwrites={}\ncommands={}\nfocus_file={}\nverification={}\ninspection_streak={}\n\nCOMPACT EVENT JOURNAL:\n{}\n\nLATEST RUNTIME OBSERVATION:\n{}\n\nChoose exactly one next action. In repair phase, inspect only what the error requires and then edit. In act phase, make a real project change or a bounded diagnostic command. Do not finish unless the controller state shows all requested verification passed and the task has real execution evidence.",
        original_request,
        state.phase.as_str(),
        state.model_rounds + 1,
        MAX_MODEL_ROUNDS,
        state.evidence.writes,
        state.evidence.commands,
        state.guard.focus_file.as_deref().unwrap_or("none"),
        state.verification.pending_summary(),
        state.guard.inspections_since_progress,
        journal,
        truncate(&state.last_observation, 20_000),
    );
    vec![
        json!({"role": "system", "content": system}),
        json!({"role": "user", "content": user}),
    ]
}

fn record_success(state: &mut RunState, action: &AgentAction) {
    match action {
        AgentAction::WriteFile { .. } | AgentAction::ReplaceInFile { .. } => {
            state.evidence.writes = state.evidence.writes.saturating_add(1);
        }
        AgentAction::RunCommand { .. } => {
            state.evidence.commands = state.evidence.commands.saturating_add(1);
            state.evidence.successful_commands.push(action.label());
        }
        _ => {}
    }
}

fn finish_run(
    app: &AppHandle,
    request_id: String,
    model: String,
    usage: UsageSummary,
    state: &mut RunState,
    recorder: &EventRecorder,
) -> Result<ChatResult, String> {
    state.phase = Phase::Complete;
    let summary = format!(
        "تم إكمال المهمة عبر HAWK Agent v7 بعد {} جولة نموذج و{} عملية تنفيذ فعلية. التعديلات: {}، الأوامر الناجحة: {}، وحالة التحقق: {}.",
        state.model_rounds,
        state.evidence.tool_actions,
        state.evidence.writes,
        state.evidence.commands,
        state.verification.pending_summary()
    );
    recorder.text(state.phase, "completed", &summary);
    emit_activity(
        app,
        &request_id,
        "agent-v7",
        "agent",
        "completed",
        "Verified completion".to_owned(),
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

fn action_detail(action: &AgentAction) -> String {
    match action {
        AgentAction::ListFiles { .. } => "Inspecting workspace files".to_owned(),
        AgentAction::SearchText { query, .. } => format!("Searching workspace for {query}"),
        AgentAction::ReadFile { path } => format!("Reading {path}"),
        AgentAction::WriteFile { path, .. } => format!("Writing {path}"),
        AgentAction::ReplaceInFile { path, .. } => format!("Editing {path}"),
        AgentAction::RunCommand { program, args, .. } => {
            format!("Running {program} {}", args.join(" "))
        }
        AgentAction::GitStatus => "Inspecting Git status".to_owned(),
        AgentAction::BrowserControl { .. } => "Controlling browser".to_owned(),
        AgentAction::Finish { .. } => "Finishing".to_owned(),
    }
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
            "{}\n... truncated by HAWK controller ...",
            value.chars().take(max_chars).collect::<String>()
        )
    }
}
