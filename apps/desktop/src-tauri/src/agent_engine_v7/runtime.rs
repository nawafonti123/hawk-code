use super::protocol::AgentAction;
use crate::{browser_automation, project};
use serde_json::Value;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Output;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

const MAX_FILE_BYTES: u64 = 1_500_000;
const MAX_WRITE_BYTES: usize = 2_000_000;
const MAX_TOOL_OUTPUT: usize = 20_000;
const MAX_SEARCH_RESULTS: usize = 120;

pub struct WorkspaceRuntime {
    root: PathBuf,
    permission: String,
}

impl WorkspaceRuntime {
    pub fn new(path: Option<&str>, permission: &str) -> Result<Self, String> {
        let root = workspace_root(path)?;
        Ok(Self {
            root,
            permission: permission.to_owned(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn normalize(&self, action: AgentAction) -> Result<AgentAction, String> {
        match action {
            AgentAction::ReadFile { path } => Ok(AgentAction::ReadFile {
                path: normalize_workspace_relative(&self.root, &path)?,
            }),
            AgentAction::WriteFile { path, content } => Ok(AgentAction::WriteFile {
                path: normalize_workspace_relative(&self.root, &path)?,
                content,
            }),
            AgentAction::ReplaceInFile {
                path,
                old_text,
                new_text,
            } => Ok(AgentAction::ReplaceInFile {
                path: normalize_workspace_relative(&self.root, &path)?,
                old_text,
                new_text,
            }),
            AgentAction::SearchText { query, path } => Ok(AgentAction::SearchText {
                query,
                path: path
                    .map(|value| normalize_workspace_relative(&self.root, &value))
                    .transpose()?,
            }),
            AgentAction::RunCommand {
                program,
                args,
                cwd,
                timeout_seconds,
            } => Ok(AgentAction::RunCommand {
                program,
                args,
                cwd: cwd
                    .map(|value| normalize_workspace_relative(&self.root, &value))
                    .transpose()?,
                timeout_seconds,
            }),
            other => Ok(other),
        }
    }

    pub async fn execute(
        &self,
        action: &AgentAction,
        cancellation: &CancellationToken,
    ) -> Result<String, String> {
        match action {
            AgentAction::ListFiles { query } => list_files(&self.root, query.as_deref()),
            AgentAction::SearchText { query, path } => {
                search_text(&self.root, query, path.as_deref())
            }
            AgentAction::ReadFile { path } => read_file(&self.root, path),
            AgentAction::WriteFile { path, content } => {
                require_edit(&self.permission)?;
                write_file(&self.root, path, content)
            }
            AgentAction::ReplaceInFile {
                path,
                old_text,
                new_text,
            } => {
                require_edit(&self.permission)?;
                replace_in_file(&self.root, path, old_text, new_text)
            }
            AgentAction::RunCommand {
                program,
                args,
                cwd,
                timeout_seconds,
            } => {
                require_command_permission(&self.permission, program, args)?;
                run_command(
                    &self.root,
                    program,
                    args,
                    cwd.as_deref(),
                    timeout_seconds.unwrap_or(150),
                    cancellation,
                )
                .await
            }
            AgentAction::GitStatus => serde_json::to_string_pretty(&project::git_status(
                self.root.to_string_lossy().as_ref(),
            )?)
            .map_err(|_| "Unable to serialize Git status.".to_owned()),
            AgentAction::BrowserControl { browser } => {
                require_edit(&self.permission)?;
                browser_automation::run(&self.root, browser, cancellation).await
            }
            AgentAction::Finish { .. } => Err("finish is handled by the controller.".to_owned()),
        }
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

fn normalize_workspace_relative(root: &Path, raw: &str) -> Result<String, String> {
    let cleaned = raw
        .trim()
        .trim_matches(|character: char| matches!(character, '\'' | '"' | '`'))
        .trim();
    if cleaned.is_empty() || cleaned == "." {
        return Ok(".".to_owned());
    }
    let candidate = PathBuf::from(cleaned);
    if !candidate.is_absolute() {
        return Ok(cleaned.replace('\\', "/"));
    }

    if candidate.exists() {
        if let Ok(canonical) = candidate.canonicalize() {
            if let Ok(relative) = canonical.strip_prefix(root) {
                let relative = relative.to_string_lossy().replace('\\', "/");
                return Ok(if relative.is_empty() { ".".to_owned() } else { relative });
            }
        }
    }

    let root_text = normalize_path_text(&root.to_string_lossy());
    let candidate_text = normalize_path_text(cleaned);
    if candidate_text == root_text {
        return Ok(".".to_owned());
    }
    let prefix = format!("{root_text}/");
    candidate_text
        .strip_prefix(&prefix)
        .filter(|relative| !relative.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "The action path is outside the active workspace.".to_owned())
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

fn safe_path(root: &Path, relative: &str, allow_missing: bool) -> Result<PathBuf, String> {
    if relative == "." {
        return Ok(root.to_path_buf());
    }
    let candidate = Path::new(relative);
    if relative.trim().is_empty()
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
        return Err(format!("Path does not exist: {relative}"));
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
                if ignored_directory(&name) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if let Ok(relative) = path.strip_prefix(root) {
                let text = relative.to_string_lossy().replace('\\', "/");
                if query.is_empty() || text.to_lowercase().contains(&query) {
                    found.push(text);
                    if found.len() >= 600 {
                        break;
                    }
                }
            }
        }
        if found.len() >= 600 {
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

fn search_text(root: &Path, query: &str, relative: Option<&str>) -> Result<String, String> {
    let start = match relative.filter(|value| *value != ".") {
        Some(value) => safe_path(root, value, false)?,
        None => root.to_path_buf(),
    };
    let needle = query.to_lowercase();
    let mut matches = Vec::new();
    let mut stack = vec![start];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            let entries = match fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let child = entry.path();
                if child.is_dir()
                    && child
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(ignored_directory)
                {
                    continue;
                }
                stack.push(child);
            }
            continue;
        }
        let Ok(metadata) = path.metadata() else {
            continue;
        };
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                matches.push(format!("{relative}:{}: {}", index + 1, truncate(line, 240)));
                if matches.len() >= MAX_SEARCH_RESULTS {
                    break;
                }
            }
        }
        if matches.len() >= MAX_SEARCH_RESULTS {
            break;
        }
    }
    Ok(if matches.is_empty() {
        "No text matches found.".to_owned()
    } else {
        matches.join("\n")
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
    let content = fs::read_to_string(&path)
        .map_err(|_| "File is not valid UTF-8 text.".to_owned())?;
    Ok(truncate(&content, MAX_TOOL_OUTPUT))
}

fn write_file(root: &Path, relative: &str, content: &str) -> Result<String, String> {
    if content.len() > MAX_WRITE_BYTES {
        return Err("File content exceeds the 2 MB write limit.".to_owned());
    }
    validate_json_if_needed(relative, content)?;
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
    validate_json_if_needed(relative, &updated)?;
    fs::write(&path, updated)
        .map_err(|error| format!("Unable to save {relative}: {error}"))?;
    Ok(format!("Updated {relative}."))
}

fn validate_json_if_needed(relative: &str, content: &str) -> Result<(), String> {
    if relative.to_ascii_lowercase().ends_with(".json") {
        serde_json::from_str::<Value>(content)
            .map(|_| ())
            .map_err(|error| format!("Refusing invalid JSON for {relative}: {error}"))
    } else {
        Ok(())
    }
}

async fn run_command(
    root: &Path,
    raw_program: &str,
    args: &[String],
    cwd: Option<&str>,
    timeout_seconds: u64,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let raw_program = raw_program.trim();
    if raw_program.is_empty() || raw_program.contains('/') || raw_program.contains('\\') {
        return Err("program must be an executable name available on PATH.".to_owned());
    }
    let lower = raw_program.to_ascii_lowercase();
    if matches!(lower.as_str(), "node" | "python" | "python3" | "py") && args.is_empty() {
        return Err(format!("Refusing interactive `{raw_program}` without a script."));
    }
    if matches!(lower.as_str(), "npm" | "pnpm" | "yarn") && is_long_running_script(args) {
        return Err("Long-running dev/start/serve commands are not valid autonomous verification steps.".to_owned());
    }
    let cwd = match cwd.filter(|value| *value != ".") {
        Some(relative) => {
            let path = safe_path(root, relative, false)?;
            if !path.is_dir() {
                return Err("run_command cwd must be a directory.".to_owned());
            }
            path
        }
        None => root.to_path_buf(),
    };
    let timeout_seconds = timeout_seconds.clamp(1, 600);
    let mut command = Command::new(platform_program(raw_program));
    command
        .args(args)
        .current_dir(&cwd)
        .kill_on_drop(true)
        .env("CI", "1")
        .env("NO_COLOR", "1");
    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
        result = timeout(Duration::from_secs(timeout_seconds), command.output()) => {
            result
                .map_err(|_| format!("Command timed out after {timeout_seconds}s: {raw_program} {}", args.join(" ")))?
                .map_err(|error| format!("Unable to start {raw_program}: {error}"))?
        }
    };
    command_result(raw_program, args, output)
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

fn require_edit(permission: &str) -> Result<(), String> {
    if matches!(permission, "auto" | "full") {
        Ok(())
    } else {
        Err("This action requires edit permission.".to_owned())
    }
}

fn require_command_permission(permission: &str, program: &str, args: &[String]) -> Result<(), String> {
    if permission == "full" {
        return Ok(());
    }
    if permission == "auto" && is_safe_development_command(program, args) {
        return Ok(());
    }
    Err("This command requires Full access unless it is a bounded project-local development command.".to_owned())
}

fn is_safe_development_command(program: &str, args: &[String]) -> bool {
    let program = program.trim().to_ascii_lowercase();
    let args = args
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    match program.as_str() {
        "npm" | "pnpm" | "yarn" => {
            (args.len() == 1 && matches!(args[0].as_str(), "test" | "lint" | "build" | "check"))
                || (args.len() >= 2
                    && args[0] == "run"
                    && matches!(
                        args[1].as_str(),
                        "test" | "lint" | "build" | "typecheck" | "check" | "verify"
                    ))
        }
        "node" => {
            !args.is_empty()
                && !args.iter().any(|value| {
                    matches!(value.as_str(), "-e" | "--eval" | "-p" | "--print" | "-i" | "--interactive")
                })
                && args.iter().all(|value| !looks_absolute_cli_argument(value))
        }
        "python" | "python3" | "py" => {
            !args.is_empty()
                && !args.iter().any(|value| matches!(value.as_str(), "-c" | "-m"))
                && args.iter().all(|value| !looks_absolute_cli_argument(value))
        }
        "cargo" => args.first().is_some_and(|value| {
            matches!(
                value.as_str(),
                "test" | "check" | "build" | "clippy" | "fmt" | "run"
            )
        }),
        "git" => args
            .first()
            .is_some_and(|value| matches!(value.as_str(), "status" | "diff" | "log" | "show")),
        _ => false,
    }
}

fn looks_absolute_cli_argument(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    normalized.starts_with('/')
        || normalized.starts_with("//")
        || normalized
            .as_bytes()
            .get(1)
            .is_some_and(|byte| *byte == b':')
}

fn is_long_running_script(args: &[String]) -> bool {
    args.first()
        .is_some_and(|value| matches!(value.as_str(), "start" | "dev" | "serve"))
        || (args.first().is_some_and(|value| value == "run")
            && args
                .get(1)
                .is_some_and(|value| matches!(value.as_str(), "start" | "dev" | "serve")))
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

fn ignored_directory(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "target" | ".next" | "dist" | "build" | ".turbo" | ".cache"
    )
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_owned()
    } else {
        format!(
            "{}\n... truncated by HAWK runtime ...",
            value.chars().take(max_chars).collect::<String>()
        )
    }
}
