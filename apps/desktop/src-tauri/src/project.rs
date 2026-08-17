use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPathPayload {
    pub workspace_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub file_count: usize,
    pub directory_count: usize,
    pub frameworks: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub branch: String,
    pub clean: bool,
    pub entries: Vec<String>,
    pub file_count: usize,
    pub additions: usize,
    pub deletions: usize,
    pub files: Vec<GitFileChange>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileChange {
    pub path: String,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffPayload {
    pub workspace_path: String,
    pub file_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileDiff {
    pub path: String,
    pub patch: String,
    pub truncated: bool,
}

fn canonical_workspace(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path.trim());
    if !path.is_dir() {
        return Err("The workspace does not exist or is not a directory.".to_owned());
    }
    path.canonicalize()
        .map_err(|_| "The workspace path could not be resolved.".to_owned())
}

pub fn summarize_workspace(path: &str) -> Result<ProjectSummary, String> {
    let root = canonical_workspace(path)?;
    let mut files = 0usize;
    let mut directories = 0usize;
    let mut frameworks = Vec::new();
    let mut stack = vec![root];
    let mut truncated = false;

    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|_| "A project directory could not be read.".to_owned())?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if matches!(
                    name.as_str(),
                    ".git" | "node_modules" | "target" | "dist" | "build" | ".next"
                ) {
                    continue;
                }
                directories += 1;
                if directories + files >= 20_000 {
                    truncated = true;
                    break;
                }
                stack.push(path);
            } else {
                files += 1;
                match name.as_str() {
                    "package.json" => frameworks.push("Node.js / JavaScript".to_owned()),
                    "Cargo.toml" => frameworks.push("Rust".to_owned()),
                    "pubspec.yaml" => frameworks.push("Flutter / Dart".to_owned()),
                    "requirements.txt" | "pyproject.toml" => frameworks.push("Python".to_owned()),
                    _ if name.ends_with(".sln") || name.ends_with(".csproj") => {
                        frameworks.push(".NET".to_owned())
                    }
                    _ => {}
                }
                if directories + files >= 20_000 {
                    truncated = true;
                    break;
                }
            }
        }
        if truncated {
            break;
        }
    }
    frameworks.sort();
    frameworks.dedup();
    Ok(ProjectSummary {
        file_count: files,
        directory_count: directories,
        frameworks,
        truncated,
    })
}

pub fn git_status(path: &str) -> Result<GitStatus, String> {
    let root = canonical_workspace(path)?;
    let raw_status = run_git(
        &root,
        &[
            "-c",
            "core.quotepath=false",
            "status",
            "--short",
            "--branch",
            "--untracked-files=all",
            "--",
            ".",
        ],
    )?;
    let lines: Vec<String> = raw_status.lines().map(str::to_owned).collect();
    let branch = lines
        .first()
        .map(|line| line.trim_start_matches("## ").to_owned())
        .unwrap_or_else(|| "HEAD".to_owned());
    let entries = lines.into_iter().skip(1).take(500).collect::<Vec<_>>();

    let mut stats = HashMap::<String, (usize, usize)>::new();
    merge_numstat(
        &mut stats,
        &run_git(
            &root,
            &[
                "-c",
                "core.quotepath=false",
                "diff",
                "--numstat",
                "--no-renames",
                "--",
                ".",
            ],
        )?,
    );
    merge_numstat(
        &mut stats,
        &run_git(
            &root,
            &[
                "-c",
                "core.quotepath=false",
                "diff",
                "--cached",
                "--numstat",
                "--no-renames",
                "--",
                ".",
            ],
        )?,
    );

    let files = entries
        .iter()
        .filter_map(|entry| parse_status_entry(entry))
        .map(|(status, file_path)| {
            let mut counts = stats.get(&file_path).copied().unwrap_or((0, 0));
            if status == "??" {
                counts.0 = count_untracked_lines(&root, &file_path);
            }
            GitFileChange {
                path: file_path,
                status,
                additions: counts.0,
                deletions: counts.1,
            }
        })
        .collect::<Vec<_>>();
    let additions = files.iter().map(|file| file.additions).sum();
    let deletions = files.iter().map(|file| file.deletions).sum();

    Ok(GitStatus {
        branch,
        clean: entries.is_empty(),
        file_count: files.len(),
        additions,
        deletions,
        files,
        entries,
    })
}

pub fn git_diff(payload: GitDiffPayload) -> Result<GitFileDiff, String> {
    let root = canonical_workspace(&payload.workspace_path)?;
    let file_path = validate_relative_git_path(&payload.file_path)?;
    let status = run_git(
        &root,
        &[
            "-c",
            "core.quotepath=false",
            "status",
            "--short",
            "--",
            file_path,
        ],
    )?;
    let mut patch = run_git(
        &root,
        &[
            "-c",
            "core.quotepath=false",
            "diff",
            "--no-ext-diff",
            "--unified=3",
            "--",
            file_path,
        ],
    )?;
    patch.push_str(&run_git(
        &root,
        &[
            "-c",
            "core.quotepath=false",
            "diff",
            "--cached",
            "--no-ext-diff",
            "--unified=3",
            "--",
            file_path,
        ],
    )?);

    if patch.trim().is_empty() && status.trim_start().starts_with("??") {
        if let Ok(content) = fs::read_to_string(root.join(file_path)) {
            patch.push_str("@@ New untracked file @@\n");
            for line in content.lines().take(1_000) {
                patch.push('+');
                patch.push_str(line);
                patch.push('\n');
            }
        }
    }

    const MAX_PATCH_CHARS: usize = 200_000;
    let truncated = patch.chars().count() > MAX_PATCH_CHARS;
    if truncated {
        patch = patch.chars().take(MAX_PATCH_CHARS).collect();
        patch.push_str("\n... diff truncated by HAWK Code ...");
    }
    if patch.trim().is_empty() {
        patch = "No textual diff is available for this file.".to_owned();
    }
    Ok(GitFileDiff {
        path: file_path.to_owned(),
        patch,
        truncated,
    })
}

fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|_| {
            "Unable to run Git. Make sure it is installed and available on PATH.".to_owned()
        })?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            "Git could not inspect this workspace.".to_owned()
        } else {
            message
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_status_entry(entry: &str) -> Option<(String, String)> {
    let status = entry.get(..2)?.trim().to_owned();
    let raw_path = entry.get(3..)?.trim().trim_matches('"');
    let path = raw_path.rsplit_once(" -> ").map_or(raw_path, |(_, to)| to);
    (!path.is_empty()).then(|| (status, path.to_owned()))
}

fn merge_numstat(stats: &mut HashMap<String, (usize, usize)>, output: &str) {
    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let additions = parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let deletions = parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let Some(path) = parts.next() else { continue };
        let counts = stats.entry(path.to_owned()).or_insert((0, 0));
        counts.0 += additions;
        counts.1 += deletions;
    }
}

fn count_untracked_lines(root: &Path, file_path: &str) -> usize {
    let path = root.join(file_path);
    let Ok(metadata) = path.metadata() else {
        return 0;
    };
    if metadata.len() > 1_048_576 {
        return 0;
    }
    fs::read_to_string(path)
        .map(|content| content.lines().count())
        .unwrap_or(0)
}

fn validate_relative_git_path(file_path: &str) -> Result<&str, String> {
    let candidate = Path::new(file_path);
    if file_path.trim().is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err("The requested Git path is invalid.".to_owned());
    }
    Ok(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_status_entries() {
        assert_eq!(
            parse_status_entry(" M src/main.rs"),
            Some(("M".to_owned(), "src/main.rs".to_owned()))
        );
        assert_eq!(
            parse_status_entry("?? notes.txt"),
            Some(("??".to_owned(), "notes.txt".to_owned()))
        );
    }

    #[test]
    fn rejects_paths_that_escape_the_workspace() {
        assert!(validate_relative_git_path("../secret.txt").is_err());
        assert!(validate_relative_git_path("src/main.rs").is_ok());
    }
}
