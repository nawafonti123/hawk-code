use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const GRAPH_VERSION: u16 = 1;
const MAX_CACHED_TEXT_BYTES: u64 = 1_000_000;
const MAX_QUERY_RESULTS: usize = 30;
const MAX_QUERY_FILE_CHARS: usize = 6_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedFile {
    size: u64,
    modified_ms: u64,
    sha256: String,
    content: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct GraphIndex {
    version: u16,
    workspace_path: String,
    updated_at_ms: u64,
    directories: Vec<String>,
    files: BTreeMap<String, CachedFile>,
}

#[derive(Debug, Default, Clone)]
pub struct GraphChanges {
    pub initial: bool,
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
    pub reused: usize,
}

pub struct ProjectGraph {
    index: GraphIndex,
    cache_path: PathBuf,
    changes: GraphChanges,
}

impl ProjectGraph {
    pub fn context(&self) -> String {
        let changes = &self.changes;
        let mode = if changes.initial {
            "Initial persistent index created"
        } else {
            "Incremental index synchronized"
        };
        let mut changed = changes
            .added
            .iter()
            .chain(changes.modified.iter())
            .take(80)
            .cloned()
            .collect::<Vec<_>>();
        changed.sort();
        let changed_paths = if changed.is_empty() {
            "none".to_owned()
        } else {
            changed.join(", ")
        };
        format!(
            "HAWK Graph persistent project memory is active. {mode}: {} indexed files in {} directories. This sync reused {} unchanged files and detected {} added, {} modified, and {} deleted files. Changed paths: {changed_paths}. Do not scan or reread the whole project. Use project_graph_structure for the cached hierarchy and project_graph_query to recall relevant cached source. Project queries rank filenames, paths, exact phrases, symbol occurrences, and recently changed files, and return excerpts around the matching code rather than blindly returning file beginnings. Use read_file only when exact full content is required; unchanged reads are served from the local graph cache. All graph data stays on this device.",
            self.index.files.len(),
            self.index.directories.len(),
            changes.reused,
            changes.added.len(),
            changes.modified.len(),
            changes.deleted.len(),
        )
    }

    pub fn structure(&self, query: Option<&str>) -> String {
        let needle = query.unwrap_or_default().trim().to_lowercase();
        let files = self
            .index
            .files
            .keys()
            .filter(|path| needle.is_empty() || path.to_lowercase().contains(&needle))
            .map(String::as_str)
            .collect::<Vec<_>>();
        format!(
            "HAWK Graph cached structure: {} files, {} directories.\n{}",
            self.index.files.len(),
            self.index.directories.len(),
            files.join("\n")
        )
    }

    pub fn query(&self, query: &str, requested_limit: Option<u64>) -> Result<String, String> {
        let query_lower = query.trim().to_lowercase();
        let terms = query_lower
            .split_whitespace()
            .filter(|term| term.len() > 1)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if terms.is_empty() {
            return Err("A project graph query must not be empty.".to_owned());
        }
        let limit = requested_limit
            .unwrap_or(10)
            .clamp(1, MAX_QUERY_RESULTS as u64) as usize;
        let changed = self
            .changes
            .added
            .iter()
            .chain(self.changes.modified.iter())
            .map(|path| normalize(path))
            .collect::<BTreeSet<_>>();

        let mut matches = self
            .index
            .files
            .iter()
            .filter_map(|(path, file)| {
                let path_lower = path.to_lowercase();
                let file_name = path_lower.rsplit('/').next().unwrap_or(&path_lower);
                let content = file.content.as_deref().unwrap_or_default();
                let content_lower = content.to_lowercase();
                let mut score = 0usize;

                if path_lower == query_lower || file_name == query_lower {
                    score += 220;
                } else if path_lower.contains(&query_lower) {
                    score += 120;
                }
                if content_lower.contains(&query_lower) {
                    score += 45;
                }

                for term in &terms {
                    if file_name.contains(term) {
                        score += 55;
                    } else if path_lower.contains(term) {
                        score += 28;
                    }
                    let occurrences = content_lower.matches(term).count().min(12);
                    score += occurrences * 4;
                }
                if changed.contains(path) {
                    score += 18;
                }

                (score > 0).then_some((score, path, file))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.cmp(right.1))
        });

        let sections = matches
            .into_iter()
            .take(limit)
            .map(|(score, path, file)| {
                let content = file
                    .content
                    .as_deref()
                    .map(|value| relevant_excerpt(value, &query_lower, &terms, MAX_QUERY_FILE_CHARS))
                    .unwrap_or_else(|| {
                        "[Metadata indexed; content is binary or too large.]".to_owned()
                    });
                format!(
                    "===== CACHED FILE: {path} | relevance {score} | {} bytes | sha256 {} =====\n{content}",
                    file.size, file.sha256
                )
            })
            .collect::<Vec<_>>();
        if sections.is_empty() {
            Ok(format!("No cached project files matched: {query}"))
        } else {
            Ok(format!(
                "HAWK Graph returned {} ranked cached matches for: {query}\n\n{}",
                sections.len(),
                sections.join("\n\n")
            ))
        }
    }

    pub fn read_text(&mut self, path: &Path, relative: &str) -> Result<String, String> {
        let key = normalize(relative);
        let metadata = path
            .metadata()
            .map_err(|_| "The requested file metadata could not be read.".to_owned())?;
        if let Some(cached) = self.index.files.get(&key) {
            if cached.size == metadata.len()
                && cached.modified_ms == modified_ms(&metadata)
                && cached.content.is_some()
            {
                return Ok(cached.content.clone().unwrap_or_default());
            }
        }
        let content = fs::read_to_string(path)
            .map_err(|_| "The requested file is not valid UTF-8 text.".to_owned())?;
        self.refresh_file(path, relative, Some(content.clone()))?;
        Ok(content)
    }

    pub fn refresh_written_file(&mut self, root: &Path, relative: &str) -> Result<(), String> {
        let path = root.join(relative);
        let content = fs::read_to_string(&path).ok();
        self.refresh_file(&path, relative, content)
    }

    fn refresh_file(
        &mut self,
        path: &Path,
        relative: &str,
        content: Option<String>,
    ) -> Result<(), String> {
        let metadata = path
            .metadata()
            .map_err(|_| "The updated project file could not be indexed.".to_owned())?;
        let bytes = fs::read(path)
            .map_err(|_| "The updated project file could not be cached.".to_owned())?;
        let key = normalize(relative);
        let cached_content =
            if metadata.len() <= MAX_CACHED_TEXT_BYTES && should_cache_content(&key) {
                content.or_else(|| String::from_utf8(bytes.clone()).ok())
            } else {
                None
            };
        self.index.files.insert(
            key,
            CachedFile {
                size: metadata.len(),
                modified_ms: modified_ms(&metadata),
                sha256: digest(&bytes),
                content: cached_content,
            },
        );
        self.index.updated_at_ms = now_ms();
        self.save()
    }

    fn save(&self) -> Result<(), String> {
        save_index(&self.cache_path, &self.index)
    }
}

pub fn sync(app: &AppHandle, root: &Path) -> Result<ProjectGraph, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "HAWK Graph could not resolve its local data directory.".to_owned())?;
    let cache_dir = data_dir.join("project-graphs");
    let workspace_key = digest(root.to_string_lossy().as_bytes());
    sync_into(root, cache_dir.join(format!("{workspace_key}.json")))
}

pub(crate) fn sync_into(root: &Path, cache_path: PathBuf) -> Result<ProjectGraph, String> {
    let previous = load_index(&cache_path).unwrap_or_default();
    let initial = previous.version != GRAPH_VERSION || previous.workspace_path.is_empty();
    let mut current_files = BTreeMap::new();
    let mut directories = BTreeSet::new();
    let mut changes = GraphChanges {
        initial,
        ..GraphChanges::default()
    };
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|_| "HAWK Graph could not scan a project directory.".to_owned())?;
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if file_type.is_dir() {
                if ignored_directory(&name) {
                    continue;
                }
                if let Ok(relative) = path.strip_prefix(root) {
                    directories.insert(normalize(&relative.to_string_lossy()));
                }
                stack.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let relative = match path.strip_prefix(root) {
                Ok(value) => normalize(&value.to_string_lossy()),
                Err(_) => continue,
            };
            let metadata = match entry.metadata() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if let Some(old) = previous.files.get(&relative) {
                if old.size == metadata.len() && old.modified_ms == modified_ms(&metadata) {
                    current_files.insert(relative, old.clone());
                    changes.reused += 1;
                    continue;
                }
            }
            let bytes = match fs::read(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let hash = digest(&bytes);
            if let Some(old) = previous.files.get(&relative) {
                if old.sha256 == hash {
                    current_files.insert(
                        relative,
                        CachedFile {
                            size: metadata.len(),
                            modified_ms: modified_ms(&metadata),
                            ..old.clone()
                        },
                    );
                    changes.reused += 1;
                    continue;
                }
                changes.modified.push(relative.clone());
            } else {
                changes.added.push(relative.clone());
            }
            let content = (metadata.len() <= MAX_CACHED_TEXT_BYTES
                && should_cache_content(&relative))
            .then(|| String::from_utf8(bytes.clone()).ok())
            .flatten();
            current_files.insert(
                relative,
                CachedFile {
                    size: metadata.len(),
                    modified_ms: modified_ms(&metadata),
                    sha256: hash,
                    content,
                },
            );
        }
    }
    changes.deleted = previous
        .files
        .keys()
        .filter(|path| !current_files.contains_key(*path))
        .cloned()
        .collect();
    let index = GraphIndex {
        version: GRAPH_VERSION,
        workspace_path: root.to_string_lossy().into_owned(),
        updated_at_ms: now_ms(),
        directories: directories.into_iter().collect(),
        files: current_files,
    };
    save_index(&cache_path, &index)?;
    Ok(ProjectGraph {
        index,
        cache_path,
        changes,
    })
}

fn load_index(path: &Path) -> Option<GraphIndex> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_index(path: &Path, index: &GraphIndex) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "HAWK Graph could not create its local cache directory.".to_owned())?;
    }
    let encoded = serde_json::to_vec(index)
        .map_err(|_| "HAWK Graph could not encode the project index.".to_owned())?;
    fs::write(path, encoded).map_err(|_| "HAWK Graph could not save the project index.".to_owned())
}

fn ignored_directory(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".hawk-graph"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | "coverage"
            | ".turbo"
            | ".cache"
            | ".playwright-cli"
    )
}

fn should_cache_content(relative: &str) -> bool {
    let name = relative
        .rsplit('/')
        .next()
        .unwrap_or(relative)
        .to_ascii_lowercase();
    !(name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name == "id_rsa"
        || name == "id_ed25519"
        || name.contains("credentials")
        || name.contains("secrets"))
}

fn normalize(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn modified_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn relevant_excerpt(value: &str, query: &str, terms: &[&str], max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    let lines = value.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return truncate(value, max_chars);
    }
    let best = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let lower = line.to_lowercase();
            let phrase_score = if lower.contains(query) { 30 } else { 0 };
            let term_score = terms
                .iter()
                .map(|term| lower.matches(term).count().min(6) * 4)
                .sum::<usize>();
            (phrase_score + term_score, index)
        })
        .max_by_key(|(score, _)| *score)
        .unwrap_or((0, 0));
    if best.0 == 0 {
        return truncate(value, max_chars);
    }

    let start = best.1.saturating_sub(28);
    let end = (best.1 + 45).min(lines.len());
    let mut excerpt = lines[start..end].join("\n");
    if start > 0 {
        excerpt = format!("... excerpt starts near line {} ...\n{excerpt}", start + 1);
    }
    if end < lines.len() {
        excerpt.push_str(&format!("\n... excerpt ends near line {end} ..."));
    }
    truncate(&excerpt, max_chars)
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_owned()
    } else {
        format!(
            "{}\n... cached content truncated ...",
            value.chars().take(max_chars).collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_secret_file_contents_out_of_the_plaintext_graph_cache() {
        assert!(!should_cache_content(".env"));
        assert!(!should_cache_content("config/production.secrets.json"));
        assert!(should_cache_content("src/config.ts"));
    }

    #[test]
    fn relevant_excerpt_centers_large_files_on_the_match() {
        let mut source = (0..180)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>();
        source[140] = "function importantCheckoutFlow() {}".to_owned();
        let source = source.join("\n");
        let excerpt = relevant_excerpt(
            &source,
            "importantcheckoutflow",
            &["importantcheckoutflow"],
            1_500,
        );
        assert!(excerpt.contains("importantCheckoutFlow"));
        assert!(!excerpt.contains("line 0\n"));
    }

    #[test]
    fn persists_structure_and_only_refreshes_changed_files() {
        let base = std::env::temp_dir().join(format!("hawk-graph-cache-{}", now_ms()));
        let root = base.join("project");
        let cache = base.join("cache/index.json");
        fs::create_dir_all(root.join("src")).expect("fixture directory should exist");
        fs::write(root.join("src/one.rs"), "fn one() {}\n").expect("first fixture should exist");
        fs::write(root.join("src/two.rs"), "fn two() {}\n").expect("second fixture should exist");

        let first = sync_into(&root, cache.clone()).expect("initial graph should build");
        assert!(first.changes.initial);
        assert_eq!(first.changes.added.len(), 2);
        assert!(first.structure(None).contains("src/one.rs"));

        let second = sync_into(&root, cache.clone()).expect("unchanged graph should load");
        assert!(!second.changes.initial);
        assert_eq!(second.changes.reused, 2);
        assert!(second.changes.added.is_empty());
        assert!(second.changes.modified.is_empty());

        fs::write(root.join("src/two.rs"), "fn two_changed() {}\n")
            .expect("fixture should update");
        let third = sync_into(&root, cache.clone()).expect("changed graph should refresh");
        assert_eq!(third.changes.modified, vec!["src/two.rs".to_owned()]);
        assert_eq!(third.changes.reused, 1);
        let recalled = third
            .query("two_changed", Some(4))
            .expect("cached query should work");
        assert!(recalled.contains("fn two_changed()"));

        let _ = fs::remove_dir_all(base);
    }
}
