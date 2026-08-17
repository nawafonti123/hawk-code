use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

const MAX_ATTACHMENTS: usize = 10;
const MAX_FILE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 28 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentPayload {
    pub paths: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAttachment {
    pub id: String,
    pub name: String,
    pub path: String,
    pub mime_type: String,
    pub size: u64,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
}

fn file_kind(extension: &str) -> Option<(&'static str, &'static str)> {
    match extension {
        "png" => Some(("image", "image/png")),
        "jpg" | "jpeg" => Some(("image", "image/jpeg")),
        "webp" => Some(("image", "image/webp")),
        "gif" => Some(("image", "image/gif")),
        "bmp" => Some(("image", "image/bmp")),
        "svg" => Some(("text", "image/svg+xml")),
        "txt" | "md" | "mdx" | "log" => Some(("text", "text/plain")),
        "json" => Some(("text", "application/json")),
        "yaml" | "yml" => Some(("text", "application/yaml")),
        "toml" => Some(("text", "application/toml")),
        "xml" => Some(("text", "application/xml")),
        "csv" => Some(("text", "text/csv")),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "css" | "scss" | "html" | "rs" | "py"
        | "go" | "java" | "kt" | "kts" | "swift" | "dart" | "sh" | "ps1" | "bat" | "cmd"
        | "sql" | "graphql" | "vue" | "svelte" => Some(("text", "text/plain")),
        _ => None,
    }
}

pub fn prepare(payload: AttachmentPayload) -> Result<Vec<PreparedAttachment>, String> {
    if payload.paths.is_empty() || payload.paths.len() > MAX_ATTACHMENTS {
        return Err("You can attach from 1 to 10 files.".to_owned());
    }

    let mut total = 0_u64;
    let mut prepared = Vec::with_capacity(payload.paths.len());
    for raw_path in payload.paths {
        let path = PathBuf::from(raw_path.trim())
            .canonicalize()
            .map_err(|_| "An attachment path could not be resolved.".to_owned())?;
        if !path.is_file() {
            return Err("Only regular files can be attached.".to_owned());
        }
        let metadata =
            fs::metadata(&path).map_err(|_| "An attachment could not be inspected.".to_owned())?;
        if metadata.len() > MAX_FILE_BYTES {
            return Err(format!("{} is larger than 12 MB.", path.display()));
        }
        total = total.saturating_add(metadata.len());
        if total > MAX_TOTAL_BYTES {
            return Err("The selected attachments exceed the 28 MB total limit.".to_owned());
        }

        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let (kind, mime_type) =
            file_kind(&extension).ok_or_else(|| {
                format!(
                "{} is not supported yet. Choose an image, source file, or plain-text document.",
                path.file_name().and_then(|value| value.to_str()).unwrap_or("The file")
            )
            })?;
        let bytes = fs::read(&path).map_err(|_| "An attachment could not be read.".to_owned())?;
        let (text_content, data_url) = if kind == "image" {
            (
                None,
                Some(format!(
                    "data:{mime_type};base64,{}",
                    STANDARD.encode(bytes)
                )),
            )
        } else {
            let text = String::from_utf8(bytes)
                .map_err(|_| "A text attachment is not valid UTF-8.".to_owned())?;
            (Some(text), None)
        };
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("attachment")
            .to_owned();
        prepared.push(PreparedAttachment {
            id: format!("{}-{}", prepared.len(), metadata.len()),
            name,
            path: path.to_string_lossy().into_owned(),
            mime_type: mime_type.to_owned(),
            size: metadata.len(),
            kind,
            text_content,
            data_url,
        });
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_binary_extensions() {
        assert!(file_kind("exe").is_none());
    }

    #[test]
    fn recognizes_visual_and_text_inputs() {
        assert_eq!(file_kind("png"), Some(("image", "image/png")));
        assert_eq!(file_kind("rs"), Some(("text", "text/plain")));
    }

    #[test]
    fn omits_empty_optional_fields_from_ipc_json() {
        let attachment = PreparedAttachment {
            id: "test-1".to_owned(),
            name: "notes.txt".to_owned(),
            path: "notes.txt".to_owned(),
            mime_type: "text/plain".to_owned(),
            size: 4,
            kind: "text",
            text_content: Some("test".to_owned()),
            data_url: None,
        };
        let value = serde_json::to_value(attachment).expect("attachment must serialize");
        assert_eq!(value["textContent"], "test");
        assert!(value.get("dataUrl").is_none());
    }
}
