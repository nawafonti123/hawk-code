use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentAction {
    ListFiles {
        #[serde(default)]
        query: Option<String>,
    },
    SearchText {
        query: String,
        #[serde(default)]
        path: Option<String>,
    },
    ReadFile {
        path: String,
    },
    WriteFile {
        path: String,
        content: String,
    },
    ReplaceInFile {
        path: String,
        #[serde(rename = "oldText")]
        old_text: String,
        #[serde(rename = "newText")]
        new_text: String,
    },
    RunCommand {
        program: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(rename = "timeoutSeconds", default)]
        timeout_seconds: Option<u64>,
    },
    GitStatus,
    BrowserControl {
        browser: Value,
    },
    Finish {
        #[serde(default)]
        summary: String,
    },
}

impl AgentAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ListFiles { .. } => "list_files",
            Self::SearchText { .. } => "search_text",
            Self::ReadFile { .. } => "read_file",
            Self::WriteFile { .. } => "write_file",
            Self::ReplaceInFile { .. } => "replace_in_file",
            Self::RunCommand { .. } => "run_command",
            Self::GitStatus => "git_status",
            Self::BrowserControl { .. } => "browser_control",
            Self::Finish { .. } => "finish",
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            Self::ReadFile { path }
            | Self::WriteFile { path, .. }
            | Self::ReplaceInFile { path, .. } => Some(path),
            Self::SearchText { path, .. } => path.as_deref(),
            _ => None,
        }
    }

    pub fn is_inspection(&self) -> bool {
        matches!(
            self,
            Self::ListFiles { .. } | Self::SearchText { .. } | Self::ReadFile { .. } | Self::GitStatus
        )
    }

    pub fn is_progress(&self) -> bool {
        matches!(
            self,
            Self::WriteFile { .. }
                | Self::ReplaceInFile { .. }
                | Self::RunCommand { .. }
                | Self::BrowserControl { .. }
        )
    }

    pub fn fingerprint(&self) -> String {
        match self {
            Self::ListFiles { query } => format!("list:{}", query.as_deref().unwrap_or("*")),
            Self::SearchText { query, path } => {
                format!("search:{}:{}", path.as_deref().unwrap_or("."), query)
            }
            Self::ReadFile { path } => format!("read:{path}"),
            Self::WriteFile { path, .. } => format!("write:{path}"),
            Self::ReplaceInFile { path, .. } => format!("replace:{path}"),
            Self::RunCommand {
                program,
                args,
                cwd,
                ..
            } => format!(
                "cmd:{}:{}:{}",
                cwd.as_deref().unwrap_or("."),
                program,
                args.join("\u{1f}")
            ),
            Self::GitStatus => "git_status".to_owned(),
            Self::BrowserControl { browser } => format!("browser:{browser}"),
            Self::Finish { .. } => "finish".to_owned(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::ListFiles { query } => format!("list_files {}", query.as_deref().unwrap_or("*")),
            Self::SearchText { query, path } => format!(
                "search_text {} in {}",
                query,
                path.as_deref().unwrap_or("workspace")
            ),
            Self::ReadFile { path } => format!("read_file {path}"),
            Self::WriteFile { path, .. } => format!("write_file {path}"),
            Self::ReplaceInFile { path, .. } => format!("replace_in_file {path}"),
            Self::RunCommand { program, args, .. } => {
                format!("run_command {program} {}", args.join(" "))
            }
            Self::GitStatus => "git_status".to_owned(),
            Self::BrowserControl { .. } => "browser_control".to_owned(),
            Self::Finish { .. } => "finish".to_owned(),
        }
    }
}

pub fn action_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": [
                    "list_files",
                    "search_text",
                    "read_file",
                    "write_file",
                    "replace_in_file",
                    "run_command",
                    "git_status",
                    "browser_control",
                    "finish"
                ]
            },
            "query": {"type": "string"},
            "path": {"type": "string"},
            "content": {"type": "string"},
            "oldText": {"type": "string"},
            "newText": {"type": "string"},
            "program": {"type": "string"},
            "args": {"type": "array", "items": {"type": "string"}},
            "cwd": {"type": "string"},
            "timeoutSeconds": {"type": "integer", "minimum": 1, "maximum": 600},
            "browser": {"type": "object"},
            "summary": {"type": "string"}
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

pub fn parse_action(content: &str) -> Result<AgentAction, String> {
    let trimmed = content.trim();
    if let Ok(action) = serde_json::from_str::<AgentAction>(trimmed) {
        return validate(action);
    }

    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim);
    if let Some(candidate) = unfenced {
        if let Ok(action) = serde_json::from_str::<AgentAction>(candidate) {
            return validate(action);
        }
    }

    if let Some(value) = legacy_tool_call_to_json(trimmed) {
        if let Ok(action) = serde_json::from_value::<AgentAction>(value) {
            return validate(action);
        }
    }

    Err("Model output did not match the HAWK action schema.".to_owned())
}

fn validate(action: AgentAction) -> Result<AgentAction, String> {
    match &action {
        AgentAction::SearchText { query, .. } if query.trim().is_empty() => {
            Err("search_text requires a non-empty query.".to_owned())
        }
        AgentAction::ReadFile { path }
        | AgentAction::WriteFile { path, .. }
        | AgentAction::ReplaceInFile { path, .. }
            if path.trim().is_empty() =>
        {
            Err("File action requires a non-empty path.".to_owned())
        }
        AgentAction::WriteFile { content, .. } if content.is_empty() => {
            Err("write_file requires content.".to_owned())
        }
        AgentAction::ReplaceInFile {
            old_text, new_text, ..
        } if old_text.is_empty() || old_text == new_text => {
            Err("replace_in_file requires distinct non-empty oldText/newText.".to_owned())
        }
        AgentAction::RunCommand { program, .. } if program.trim().is_empty() => {
            Err("run_command requires a program.".to_owned())
        }
        AgentAction::BrowserControl { browser } if !browser.is_object() => {
            Err("browser_control requires a browser object.".to_owned())
        }
        _ => Ok(action),
    }
}

fn legacy_tool_call_to_json(input: &str) -> Option<Value> {
    let function_name = extract_function_name(input)?;
    let mut map = parse_parameters(input);
    map.insert("action".to_owned(), Value::String(function_name));
    Some(Value::Object(map))
}

fn extract_function_name(input: &str) -> Option<String> {
    for marker in ["<function=", "function="] {
        let position = input.find(marker)?;
        let tail = &input[position + marker.len()..];
        let end = tail
            .find(|character: char| {
                matches!(character, '>' | '\n' | '\r' | '<' | ' ' | '\t')
            })
            .unwrap_or(tail.len());
        let name = tail[..end]
            .trim()
            .trim_matches(|character: char| matches!(character, '\'' | '"' | '`'));
        if matches!(
            name,
            "list_files"
                | "search_text"
                | "read_file"
                | "write_file"
                | "replace_in_file"
                | "run_command"
                | "git_status"
                | "browser_control"
                | "finish"
        ) {
            return Some(name.to_owned());
        }
    }
    None
}

fn parse_parameters(input: &str) -> Map<String, Value> {
    let mut result = Map::new();
    let marker = "<parameter=";
    let mut cursor = 0usize;
    while let Some(relative) = input[cursor..].find(marker) {
        let name_start = cursor + relative + marker.len();
        let tail = &input[name_start..];
        let Some(name_end) = tail.find('>') else {
            break;
        };
        let name = tail[..name_end]
            .trim()
            .trim_matches(|character: char| matches!(character, '\'' | '"' | '`'));
        let value_start = name_start + name_end + 1;
        let remaining = &input[value_start..];
        let Some(value_end) = remaining.find("</parameter>") else {
            break;
        };
        let raw = remaining[..value_end].trim();
        let value = if matches!(name, "args" | "browser") {
            serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned()))
        } else if name == "timeoutSeconds" {
            raw.parse::<u64>()
                .map(|number| Value::Number(number.into()))
                .unwrap_or_else(|_| Value::String(raw.to_owned()))
        } else {
            Value::String(raw.to_owned())
        };
        result.insert(name.to_owned(), value);
        cursor = value_start + value_end + "</parameter>".len();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_schema_json() {
        let action = parse_action(r#"{"action":"run_command","program":"npm","args":["test"]}"#)
            .expect("valid action");
        assert_eq!(action.name(), "run_command");
    }

    #[test]
    fn parses_legacy_loose_function() {
        let action = parse_action("<tool_call>\nfunction=list_files\n</function>")
            .expect("legacy syntax");
        assert_eq!(action.name(), "list_files");
    }
}
