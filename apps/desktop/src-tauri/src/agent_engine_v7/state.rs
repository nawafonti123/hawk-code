use super::protocol::AgentAction;
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Inspect,
    Act,
    Verify,
    Repair,
    Complete,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Act => "act",
            Self::Verify => "verify",
            Self::Repair => "repair",
            Self::Complete => "complete",
        }
    }
}

#[derive(Default, Debug)]
pub struct Requirements {
    pub execution: bool,
    pub writes: bool,
    pub test: bool,
    pub lint: bool,
    pub build: bool,
}

impl Requirements {
    pub fn infer(text: &str) -> Self {
        let lower = text.to_lowercase();
        Self {
            execution: contains_any(
                &lower,
                &[
                    "أنش", "انش", "ابن", "بناء", "نفذ", "نفّذ", "طبق", "طبّق", "اصلح",
                    "أصلح", "عدّل", "عدل", "اكتب", "شغل", "شغّل", "اختبر", "أكمل", "اكمل",
                    "create", "build", "implement", "fix", "modify", "write", "run", "test",
                    "continue",
                ],
            ),
            writes: contains_any(
                &lower,
                &[
                    "أنش", "انش", "ابن", "بناء", "طبق", "طبّق", "اصلح", "أصلح", "عدّل",
                    "عدل", "اكتب", "create", "build", "implement", "fix", "modify", "write",
                ],
            ),
            test: contains_any(
                &lower,
                &["اختبار", "اختبارات", "اختبر", "test", "tests", "npm test"],
            ),
            lint: contains_any(&lower, &["lint", "فحص lint"]),
            build: contains_any(
                &lower,
                &[
                    "npm run build",
                    "pnpm build",
                    "cargo build",
                    " build",
                    "بناء المشروع",
                    "البناء",
                ],
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    NotRequired,
    Pending,
    Failed,
    Passed,
}

#[derive(Debug)]
pub struct VerificationState {
    pub test: CheckState,
    pub lint: CheckState,
    pub build: CheckState,
    pub last_failed: Option<&'static str>,
}

impl VerificationState {
    pub fn new(requirements: &Requirements) -> Self {
        Self {
            test: if requirements.test {
                CheckState::Pending
            } else {
                CheckState::NotRequired
            },
            lint: if requirements.lint {
                CheckState::Pending
            } else {
                CheckState::NotRequired
            },
            build: if requirements.build {
                CheckState::Pending
            } else {
                CheckState::NotRequired
            },
            last_failed: None,
        }
    }

    pub fn all_passed(&self) -> bool {
        [self.test, self.lint, self.build]
            .into_iter()
            .all(|state| matches!(state, CheckState::NotRequired | CheckState::Passed))
    }

    pub fn next_check(&self) -> Option<&'static str> {
        if matches!(self.test, CheckState::Pending | CheckState::Failed) {
            return Some("test");
        }
        if matches!(self.lint, CheckState::Pending | CheckState::Failed) {
            return Some("lint");
        }
        if matches!(self.build, CheckState::Pending | CheckState::Failed) {
            return Some("build");
        }
        None
    }

    pub fn mark(&mut self, name: &'static str, passed: bool) {
        let state = if passed {
            CheckState::Passed
        } else {
            CheckState::Failed
        };
        match name {
            "test" => self.test = state,
            "lint" => self.lint = state,
            "build" => self.build = state,
            _ => return,
        }
        if passed {
            if self.last_failed == Some(name) {
                self.last_failed = None;
            }
        } else {
            self.last_failed = Some(name);
        }
    }

    pub fn pending_summary(&self) -> String {
        let mut parts = Vec::new();
        for (name, state) in [
            ("test", self.test),
            ("lint", self.lint),
            ("build", self.build),
        ] {
            match state {
                CheckState::Pending => parts.push(format!("{name}:pending")),
                CheckState::Failed => parts.push(format!("{name}:failed")),
                CheckState::Passed => parts.push(format!("{name}:pass")),
                CheckState::NotRequired => {}
            }
        }
        if parts.is_empty() {
            "none".to_owned()
        } else {
            parts.join(", ")
        }
    }
}

#[derive(Default)]
pub struct Evidence {
    pub tool_actions: usize,
    pub writes: usize,
    pub commands: usize,
    pub successful_commands: Vec<String>,
}

#[derive(Default)]
pub struct LoopGuard {
    pub inspections_since_progress: usize,
    pub read_cache: HashMap<String, String>,
    pub seen_inspections: HashSet<String>,
    pub failed_command_epoch: HashMap<String, usize>,
    pub progress_epoch: usize,
    pub focus_file: Option<String>,
}

impl LoopGuard {
    pub fn cached_read(&self, path: &str) -> Option<&str> {
        self.read_cache.get(path).map(String::as_str)
    }

    pub fn remember_inspection(&mut self, action: &AgentAction, output: &str) {
        self.inspections_since_progress = self.inspections_since_progress.saturating_add(1);
        self.seen_inspections.insert(action.fingerprint());
        if let AgentAction::ReadFile { path } = action {
            self.focus_file = Some(path.clone());
            self.read_cache.insert(path.clone(), output.to_owned());
        }
    }

    pub fn mark_progress(&mut self, action: &AgentAction) {
        self.progress_epoch = self.progress_epoch.saturating_add(1);
        self.inspections_since_progress = 0;
        self.seen_inspections.clear();
        if let Some(path) = action.path() {
            self.read_cache.remove(path);
            self.focus_file = Some(path.to_owned());
        }
    }

    pub fn mark_failed_command(&mut self, action: &AgentAction) {
        self.failed_command_epoch
            .insert(action.fingerprint(), self.progress_epoch);
    }

    pub fn repeated_failed_command(&self, action: &AgentAction) -> bool {
        self.failed_command_epoch
            .get(&action.fingerprint())
            .is_some_and(|epoch| *epoch == self.progress_epoch)
    }
}

pub struct RunState {
    pub phase: Phase,
    pub requirements: Requirements,
    pub verification: VerificationState,
    pub evidence: Evidence,
    pub guard: LoopGuard,
    pub journal: Vec<String>,
    pub model_rounds: usize,
    pub last_observation: String,
}

impl RunState {
    pub fn new(requirements: Requirements) -> Self {
        let verification = VerificationState::new(&requirements);
        Self {
            phase: Phase::Inspect,
            requirements,
            verification,
            evidence: Evidence::default(),
            guard: LoopGuard::default(),
            journal: Vec::new(),
            model_rounds: 0,
            last_observation: "No action has executed yet.".to_owned(),
        }
    }

    pub fn push_journal(&mut self, entry: String) {
        self.journal.push(truncate(&entry, 900));
        if self.journal.len() > 32 {
            let overflow = self.journal.len() - 32;
            self.journal.drain(0..overflow);
        }
    }

    pub fn completion_blocker(&self) -> Option<String> {
        if self.requirements.execution && self.evidence.tool_actions == 0 {
            return Some("no real action executed".to_owned());
        }
        if self.requirements.writes && self.evidence.writes == 0 {
            return Some("the task requires project changes but no file was edited".to_owned());
        }
        if !self.verification.all_passed() {
            return Some(format!(
                "required verification is incomplete: {}",
                self.verification.pending_summary()
            ));
        }
        None
    }
}

#[derive(Serialize)]
struct PersistedEvent<'a> {
    ts_ms: u128,
    request_id: &'a str,
    phase: &'a str,
    kind: &'a str,
    detail: ValueWrapper<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ValueWrapper<'a> {
    Text(&'a str),
    Json(&'a serde_json::Value),
}

pub struct EventRecorder {
    path: PathBuf,
    request_id: String,
}

impl EventRecorder {
    pub fn new(request_id: &str) -> Self {
        let base = data_dir().join("agent-runs");
        let _ = fs::create_dir_all(&base);
        let safe_id = request_id
            .chars()
            .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
            .collect::<String>();
        Self {
            path: base.join(format!("{}.jsonl", if safe_id.is_empty() { "run" } else { &safe_id })),
            request_id: request_id.to_owned(),
        }
    }

    pub fn text(&self, phase: Phase, kind: &str, detail: &str) {
        self.append(PersistedEvent {
            ts_ms: now_ms(),
            request_id: &self.request_id,
            phase: phase.as_str(),
            kind,
            detail: ValueWrapper::Text(detail),
        });
    }

    pub fn json(&self, phase: Phase, kind: &str, detail: &serde_json::Value) {
        self.append(PersistedEvent {
            ts_ms: now_ms(),
            request_id: &self.request_id,
            phase: phase.as_str(),
            kind,
            detail: ValueWrapper::Json(detail),
        });
    }

    fn append(&self, event: PersistedEvent<'_>) {
        let Ok(line) = serde_json::to_string(&event) else {
            return;
        };
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(file, "{line}");
        }
    }
}

fn data_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("HAWK Code");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".hawk-code");
    }
    std::env::temp_dir().join("hawk-code")
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn contains_any(text: &str, values: &[&str]) -> bool {
    values.iter().any(|value| text.contains(value))
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_owned()
    } else {
        format!("{}…", value.chars().take(max_chars).collect::<String>())
    }
}

pub fn node_verification_action(check: &str) -> Option<AgentAction> {
    match check {
        "test" => Some(AgentAction::RunCommand {
            program: "npm".to_owned(),
            args: vec!["test".to_owned()],
            cwd: None,
            timeout_seconds: Some(180),
        }),
        "lint" => Some(AgentAction::RunCommand {
            program: "npm".to_owned(),
            args: vec!["run".to_owned(), "lint".to_owned()],
            cwd: None,
            timeout_seconds: Some(180),
        }),
        "build" => Some(AgentAction::RunCommand {
            program: "npm".to_owned(),
            args: vec!["run".to_owned(), "build".to_owned()],
            cwd: None,
            timeout_seconds: Some(240),
        }),
        _ => None,
    }
}

pub fn action_to_json(action: &AgentAction) -> serde_json::Value {
    serde_json::to_value(action).unwrap_or_else(|_| json!({"action": action.name()}))
}
