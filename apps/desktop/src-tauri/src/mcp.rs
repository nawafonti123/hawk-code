use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::time::{timeout, Duration};

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpProbePayload {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpProbeResult {
    pub server_name: String,
    pub protocol_version: String,
    pub tools: Vec<McpTool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinCallPayload {
    pub tool: String,
    pub workspace_path: String,
}

fn builtin_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "hawk.workspace.summary".to_owned(),
            description:
                "Inspect the bounded structure and detected stack of the active workspace."
                    .to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": { "workspacePath": { "type": "string" } },
                "required": ["workspacePath"],
                "additionalProperties": false
            }),
        },
        McpTool {
            name: "hawk.git.status".to_owned(),
            description: "Read the active branch and bounded Git working-tree status.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": { "workspacePath": { "type": "string" } },
                "required": ["workspacePath"],
                "additionalProperties": false
            }),
        },
    ]
}

pub fn call_builtin(payload: BuiltinCallPayload) -> Result<Value, String> {
    match payload.tool.as_str() {
        "hawk.workspace.summary" => crate::project::summarize_workspace(&payload.workspace_path)
            .and_then(|result| {
                serde_json::to_value(result)
                    .map_err(|_| "The workspace result could not be encoded.".to_owned())
            }),
        "hawk.git.status" => {
            crate::project::git_status(&payload.workspace_path).and_then(|result| {
                serde_json::to_value(result)
                    .map_err(|_| "The Git result could not be encoded.".to_owned())
            })
        }
        _ => Err("The requested built-in MCP tool is not registered.".to_owned()),
    }
}

pub async fn probe_builtin(workspace_path: Option<String>) -> Result<McpProbeResult, String> {
    let executable = std::env::current_exe()
        .map_err(|_| "The HAWK executable path could not be resolved.".to_owned())?;
    probe(McpProbePayload {
        name: "HAWK Workspace MCP".to_owned(),
        command: executable.to_string_lossy().into_owned(),
        args: vec!["--hawk-mcp-stdio".to_owned()],
        workspace_path,
    })
    .await
}

pub fn run_builtin_stdio() -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|_| "Failed to read MCP stdin.".to_owned())?;
        if line.len() > MAX_MESSAGE_BYTES {
            return Err("The MCP request exceeded the message limit.".to_owned());
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let result = match method {
            "initialize" => json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "HAWK Workspace MCP", "version": env!("CARGO_PKG_VERSION") }
            }),
            "tools/list" => json!({ "tools": builtin_tools() }),
            "tools/call" => {
                let tool = request["params"]["name"].as_str().unwrap_or_default();
                let workspace_path = request["params"]["arguments"]["workspacePath"]
                    .as_str()
                    .unwrap_or_default();
                match call_builtin(BuiltinCallPayload {
                    tool: tool.to_owned(),
                    workspace_path: workspace_path.to_owned(),
                }) {
                    Ok(value) => json!({
                        "content": [{ "type": "text", "text": value.to_string() }],
                        "structuredContent": value,
                        "isError": false
                    }),
                    Err(error) => json!({
                        "content": [{ "type": "text", "text": error }],
                        "isError": true
                    }),
                }
            }
            _ => json!({}),
        };
        let response = json!({ "jsonrpc": "2.0", "id": id, "result": result });
        serde_json::to_writer(&mut stdout, &response)
            .map_err(|_| "Failed to encode the MCP response.".to_owned())?;
        stdout
            .write_all(b"\n")
            .map_err(|_| "Failed to write the MCP response.".to_owned())?;
        stdout
            .flush()
            .map_err(|_| "Failed to flush the MCP response.".to_owned())?;
    }
    Ok(())
}

fn validated_command(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if !path.is_absolute() || !path.is_file() {
        return Err(
            "MCP server command must be an absolute path to an existing executable.".to_owned(),
        );
    }
    path.canonicalize()
        .map_err(|_| "The MCP server executable could not be resolved.".to_owned())
}

fn validated_workspace(raw: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(raw.trim());
    if !path.is_dir() {
        return Err("The MCP working directory must be an existing directory.".to_owned());
    }
    path.canonicalize()
        .map(Some)
        .map_err(|_| "The MCP working directory could not be resolved.".to_owned())
}

async fn write_message(child: &mut Child, value: Value) -> Result<(), String> {
    let mut serialized =
        serde_json::to_vec(&value).map_err(|_| "Failed to encode an MCP request.".to_owned())?;
    serialized.push(b'\n');
    let stdin = child
        .stdin
        .as_mut()
        .ok_or_else(|| "MCP stdin is unavailable.".to_owned())?;
    stdin
        .write_all(&serialized)
        .await
        .map_err(|_| "Failed to write to the MCP server.".to_owned())?;
    stdin
        .flush()
        .await
        .map_err(|_| "Failed to flush the MCP request.".to_owned())
}

async fn read_response(
    reader: &mut BufReader<ChildStdout>,
    expected_id: i64,
) -> Result<Value, String> {
    loop {
        let mut line = String::new();
        let bytes = timeout(
            Duration::from_secs(15),
            reader
                .take((MAX_MESSAGE_BYTES + 1) as u64)
                .read_line(&mut line),
        )
        .await
        .map_err(|_| "The MCP server timed out.".to_owned())?
        .map_err(|_| "Failed to read from the MCP server.".to_owned())?;
        if bytes == 0 {
            return Err("The MCP server exited before responding.".to_owned());
        }
        if bytes > MAX_MESSAGE_BYTES {
            return Err("The MCP server returned an oversized message.".to_owned());
        }
        let value: Value = serde_json::from_str(line.trim())
            .map_err(|_| "The MCP server returned invalid JSON-RPC.".to_owned())?;
        if value.get("id").and_then(Value::as_i64) != Some(expected_id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!("MCP server error: {error}"));
        }
        return value
            .get("result")
            .cloned()
            .ok_or_else(|| "The MCP response has no result.".to_owned());
    }
}

pub async fn probe(payload: McpProbePayload) -> Result<McpProbeResult, String> {
    if payload.name.trim().is_empty() || payload.name.len() > 80 || payload.args.len() > 32 {
        return Err("The MCP server name or argument count is invalid.".to_owned());
    }
    if payload
        .args
        .iter()
        .any(|arg| arg.len() > 2_048 || arg.contains('\0'))
    {
        return Err("An MCP server argument is invalid.".to_owned());
    }
    let executable = validated_command(&payload.command)?;
    let workspace = validated_workspace(payload.workspace_path.as_deref())?;
    let mut command = Command::new(executable);
    command
        .args(payload.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(workspace) = workspace {
        command.current_dir(workspace);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "The MCP server could not be started.".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "MCP stdout is unavailable.".to_owned())?;
    let mut reader = BufReader::new(stdout);

    write_message(
        &mut child,
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "HAWK Code", "version": env!("CARGO_PKG_VERSION") }
            }
        }),
    )
    .await?;
    let initialized = read_response(&mut reader, 1).await?;
    let protocol_version = initialized
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(MCP_PROTOCOL_VERSION)
        .to_owned();
    write_message(
        &mut child,
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }),
    )
    .await?;
    write_message(
        &mut child,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }),
    )
    .await?;
    let listed = read_response(&mut reader, 2).await?;
    let tools = listed
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(250)
        .filter_map(|tool| {
            Some(McpTool {
                name: tool.get("name")?.as_str()?.to_owned(),
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .chars()
                    .take(1_000)
                    .collect(),
                input_schema: tool
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object" })),
            })
        })
        .collect();
    let _ = child.kill().await;
    Ok(McpProbeResult {
        server_name: payload.name.trim().to_owned(),
        protocol_version,
        tools,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_path_lookup_for_mcp_commands() {
        assert!(validated_command("npx").is_err());
    }

    #[test]
    fn built_in_workspace_tool_runs_without_an_external_download() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let result = call_builtin(BuiltinCallPayload {
            tool: "hawk.workspace.summary".to_owned(),
            workspace_path: workspace.to_string_lossy().into_owned(),
        })
        .expect("the built-in workspace tool should run");
        assert!(result["fileCount"].as_u64().unwrap_or_default() > 0);
    }

    #[tokio::test]
    #[ignore = "requires HAWK_TEST_NODE to point to node.exe"]
    async fn discovers_tools_from_a_real_stdio_server() {
        let node = std::env::var("HAWK_TEST_NODE").expect("HAWK_TEST_NODE is required");
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("mcp_server.mjs");
        let result = probe(McpProbePayload {
            name: "Fixture".to_owned(),
            command: node,
            args: vec![fixture.to_string_lossy().into_owned()],
            workspace_path: None,
        })
        .await
        .expect("the fixture MCP server should initialize");
        assert_eq!(result.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.tools[0].name, "fixture.echo");
    }
}
