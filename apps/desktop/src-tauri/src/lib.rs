mod agent;
mod attachments;
mod auth;
mod autonomous_agent;
mod autonomous_agent_v2;
mod autonomous_agent_v3;
mod autonomous_agent_v5;
mod browser_automation;
mod browser_fast_path;
mod mcp;
mod oauth;
mod project;
mod project_graph;
mod provider;

use project::{GitDiffPayload, GitFileDiff, GitStatus, ProjectPathPayload, ProjectSummary};
use provider::{
    ChatPayload, ChatResult, ConnectionResult, ProviderConfig, ProviderRuntime, ProviderStatus,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;
use tauri_plugin_sql::{Migration, MigrationKind};
use thiserror::Error;

const IPC_PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IpcRequest<T> {
    protocol_version: u16,
    request_id: String,
    payload: T,
}

#[derive(Debug, Deserialize)]
struct EmptyPayload {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspacePayload {
    workspace_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyPayload {
    api_key: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeStatus {
    protocol_version: u16,
    app_version: &'static str,
    platform: &'static str,
    database_ready: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceValidation {
    valid: bool,
    canonical_path: String,
    display_name: String,
}

#[derive(Debug, Error)]
enum IpcError {
    #[error("Unsupported IPC protocol version: {0}")]
    UnsupportedProtocol(u16),
    #[error("Invalid request identifier")]
    InvalidRequestId,
    #[error("The selected workspace path is empty")]
    EmptyPath,
    #[error("The selected workspace does not exist or is not a directory")]
    InvalidWorkspace,
    #[error("Unable to resolve the selected workspace")]
    CanonicalizationFailed,
}

fn validate_envelope<T>(request: &IpcRequest<T>) -> Result<(), IpcError> {
    if request.protocol_version != IPC_PROTOCOL_VERSION {
        return Err(IpcError::UnsupportedProtocol(request.protocol_version));
    }
    if request.request_id.len() < 16 || request.request_id.len() > 64 {
        return Err(IpcError::InvalidRequestId);
    }
    Ok(())
}

#[tauri::command]
fn runtime_status(request: IpcRequest<EmptyPayload>) -> Result<RuntimeStatus, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    Ok(RuntimeStatus {
        protocol_version: IPC_PROTOCOL_VERSION,
        app_version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        database_ready: true,
    })
}

#[tauri::command]
fn validate_workspace(
    request: IpcRequest<WorkspacePayload>,
) -> Result<WorkspaceValidation, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    let raw_path = request.payload.workspace_path.trim();
    if raw_path.is_empty() {
        return Err(IpcError::EmptyPath.to_string());
    }
    let path = PathBuf::from(raw_path);
    if !path.is_dir() {
        return Err(IpcError::InvalidWorkspace.to_string());
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| IpcError::CanonicalizationFailed.to_string())?;
    let display_name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Workspace")
        .to_owned();
    Ok(WorkspaceValidation {
        valid: true,
        canonical_path: canonical.to_string_lossy().into_owned(),
        display_name,
    })
}

#[tauri::command]
fn qwen_provider_status(request: IpcRequest<EmptyPayload>) -> Result<ProviderStatus, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    Ok(provider::provider_status())
}

#[tauri::command]
fn qwen_save_api_key(request: IpcRequest<ApiKeyPayload>) -> Result<ProviderStatus, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    provider::save_api_key(&request.payload.api_key)
}

#[tauri::command]
fn qwen_delete_api_key(request: IpcRequest<EmptyPayload>) -> Result<bool, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    provider::delete_api_key()
}

#[tauri::command]
async fn qwen_test_connection(
    request: IpcRequest<ProviderConfig>,
    runtime: State<'_, ProviderRuntime>,
) -> Result<ConnectionResult, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    provider::test_connection(&runtime, request.payload).await
}

#[tauri::command]
async fn qwen_chat(
    app: tauri::AppHandle,
    request: IpcRequest<ChatPayload>,
    runtime: State<'_, ProviderRuntime>,
) -> Result<ChatResult, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    let cancellation = runtime.replace_cancellation();
    provider::stream_chat(&app, &runtime, request.payload, cancellation).await
}

#[tauri::command]
async fn qwen_agent(
    app: tauri::AppHandle,
    request: IpcRequest<agent::AgentPayload>,
    runtime: State<'_, ProviderRuntime>,
) -> Result<ChatResult, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    let cancellation = runtime.replace_cancellation();
    match browser_fast_path::try_run(
        &app,
        &runtime,
        request.payload,
        cancellation.clone(),
    )
    .await?
    {
        browser_fast_path::FastPathOutcome::Handled(result) => Ok(result),
        browser_fast_path::FastPathOutcome::Continue(payload) => {
            autonomous_agent::run(&app, &runtime, payload, cancellation).await
        }
    }
}

#[tauri::command]
async fn mcp_probe(
    request: IpcRequest<mcp::McpProbePayload>,
) -> Result<mcp::McpProbeResult, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    mcp::probe(request.payload).await
}

#[tauri::command]
async fn mcp_builtin_probe(
    request: IpcRequest<WorkspacePayload>,
) -> Result<mcp::McpProbeResult, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    let workspace = (!request.payload.workspace_path.trim().is_empty())
        .then_some(request.payload.workspace_path);
    mcp::probe_builtin(workspace).await
}

#[tauri::command]
fn mcp_builtin_call(
    request: IpcRequest<mcp::BuiltinCallPayload>,
) -> Result<serde_json::Value, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    mcp::call_builtin(request.payload)
}

#[tauri::command]
fn stop_all(
    request: IpcRequest<EmptyPayload>,
    runtime: State<'_, ProviderRuntime>,
) -> Result<bool, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    Ok(runtime.stop_all())
}

#[tauri::command]
fn workspace_summary(request: IpcRequest<ProjectPathPayload>) -> Result<ProjectSummary, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    project::summarize_workspace(&request.payload.workspace_path)
}

#[tauri::command]
fn workspace_git_status(request: IpcRequest<ProjectPathPayload>) -> Result<GitStatus, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    project::git_status(&request.payload.workspace_path)
}

#[tauri::command]
fn workspace_git_diff(request: IpcRequest<GitDiffPayload>) -> Result<GitFileDiff, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    project::git_diff(request.payload)
}

#[tauri::command]
fn prepare_attachments(
    request: IpcRequest<attachments::AttachmentPayload>,
) -> Result<Vec<attachments::PreparedAttachment>, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    attachments::prepare(request.payload)
}

#[tauri::command]
fn auth_register(request: IpcRequest<auth::RegisterPayload>) -> Result<auth::AuthProfile, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    auth::register(request.payload)
}

#[tauri::command]
fn auth_login(
    request: IpcRequest<auth::LoginPayload>,
    runtime: State<'_, auth::AuthRuntime>,
) -> Result<auth::AuthProfile, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    runtime.login(request.payload)
}

#[tauri::command]
fn oauth_status(
    request: IpcRequest<EmptyPayload>,
) -> Result<Vec<oauth::OAuthProviderStatus>, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    Ok(oauth::provider_statuses())
}

#[tauri::command]
async fn oauth_login_google(
    request: IpcRequest<EmptyPayload>,
) -> Result<auth::AuthProfile, String> {
    validate_envelope(&request).map_err(|error| error.to_string())?;
    oauth::login_google().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![Migration {
        version: 1,
        description: "create_phase_one_foundation",
        sql: include_str!("../migrations/0001_foundation.sql"),
        kind: MigrationKind::Up,
    }];

    tauri::Builder::default()
        .manage(ProviderRuntime::new())
        .manage(auth::AuthRuntime::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:hawk-code.db", migrations)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            runtime_status,
            validate_workspace,
            qwen_provider_status,
            qwen_save_api_key,
            qwen_delete_api_key,
            qwen_test_connection,
            qwen_chat,
            qwen_agent,
            mcp_probe,
            mcp_builtin_probe,
            mcp_builtin_call,
            stop_all,
            workspace_summary,
            workspace_git_status,
            workspace_git_diff,
            prepare_attachments,
            auth_register,
            auth_login,
            oauth_status,
            oauth_login_google
        ])
        .run(tauri::generate_context!())
        .expect("failed to run HAWK Code desktop application");
}

pub fn run_builtin_mcp_stdio() -> Result<(), String> {
    mcp::run_builtin_stdio()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_protocol_version() {
        let request = IpcRequest {
            protocol_version: 999,
            request_id: "2ec9cb4d-5f75-4ef8-a6a6-1bde243ada76".to_owned(),
            payload: EmptyPayload {},
        };
        assert!(matches!(
            validate_envelope(&request),
            Err(IpcError::UnsupportedProtocol(999))
        ));
    }

    #[test]
    fn accepts_current_protocol_version() {
        let request = IpcRequest {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: "2ec9cb4d-5f75-4ef8-a6a6-1bde243ada76".to_owned(),
            payload: EmptyPayload {},
        };
        assert!(validate_envelope(&request).is_ok());
    }
}
