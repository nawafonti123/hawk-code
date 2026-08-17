use serde_json::Value;
use std::path::Path;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

const MAX_BROWSER_OUTPUT: usize = 36_000;
const BROWSER_TIMEOUT: Duration = Duration::from_secs(120);

pub async fn run(
    root: &Path,
    args: &Value,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let action = required(args, "action")?;
    let cli_args = command_args(action, args)?;
    let first = execute(root, &cli_args, cancellation).await;

    if action == "open" {
        if let Err(error) = &first {
            let lower = error.to_ascii_lowercase();
            if lower.contains("browser")
                && (lower.contains("install") || lower.contains("executable"))
            {
                install_browser(root, cancellation).await?;
                return execute(root, &cli_args, cancellation).await;
            }
        }
    }
    first
}

fn command_args(action: &str, args: &Value) -> Result<Vec<String>, String> {
    let mut command = Vec::new();
    match action {
        "open" => {
            let url = safe_url(required(args, "url")?)?;
            command.extend([
                "open".to_owned(),
                url,
                "--headed".to_owned(),
                "--persistent".to_owned(),
            ]);
        }
        "goto" => {
            let url = safe_url(required(args, "url")?)?;
            command.extend(["goto".to_owned(), url]);
        }
        "snapshot" => {
            command.extend([
                "snapshot".to_owned(),
                "--depth=6".to_owned(),
                "--raw".to_owned(),
            ]);
        }
        "click" => {
            command.extend(["click".to_owned(), required(args, "target")?.to_owned()]);
        }
        "fill" => {
            command.extend([
                "fill".to_owned(),
                required(args, "target")?.to_owned(),
                required(args, "value")?.to_owned(),
            ]);
        }
        "type" => {
            command.extend(["type".to_owned(), required(args, "value")?.to_owned()]);
        }
        "press" => {
            command.extend(["press".to_owned(), required(args, "value")?.to_owned()]);
        }
        "screenshot" => {
            command.push("screenshot".to_owned());
            if args["fullPage"].as_bool().unwrap_or(false) {
                command.push("--full-page".to_owned());
            }
        }
        "back" => command.push("go-back".to_owned()),
        "forward" => command.push("go-forward".to_owned()),
        "reload" => command.push("reload".to_owned()),
        "close" => command.push("close".to_owned()),
        _ => {
            return Err(
                "Unsupported browser action. Use open, goto, snapshot, click, fill, type, press, screenshot, back, forward, reload, or close."
                    .to_owned(),
            )
        }
    }
    Ok(command)
}

async fn execute(
    root: &Path,
    cli_args: &[String],
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let npx = if cfg!(windows) { "npx.cmd" } else { "npx" };
    let mut command = Command::new(npx);
    command
        .args(["--yes", "--package", "@playwright/cli@latest", "playwright-cli"])
        .args(cli_args)
        .current_dir(root)
        .kill_on_drop(true);

    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
        result = timeout(BROWSER_TIMEOUT, command.output()) => result
            .map_err(|_| "Playwright browser automation timed out after 120 seconds.".to_owned())?
            .map_err(|error| format!("Unable to start Playwright CLI through npx: {error}"))?,
    };
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(format!("Playwright action failed:\n{}", truncate(&combined)));
    }
    Ok(truncate(&combined))
}

async fn install_browser(
    root: &Path,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let npx = if cfg!(windows) { "npx.cmd" } else { "npx" };
    let mut command = Command::new(npx);
    command
        .args([
            "--yes",
            "--package",
            "@playwright/cli@latest",
            "playwright-cli",
            "install-browser",
            "chromium",
        ])
        .current_dir(root)
        .kill_on_drop(true);
    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err("TASK_CANCELLED".to_owned()),
        result = timeout(Duration::from_secs(300), command.output()) => result
            .map_err(|_| "Installing the Playwright Chromium browser timed out.".to_owned())?
            .map_err(|error| format!("Unable to install the Playwright browser: {error}"))?,
    };
    if !output.status.success() {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(format!(
            "Playwright Chromium installation failed:\n{}",
            truncate(&combined)
        ));
    }
    Ok(())
}

fn safe_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("Browser automation only accepts http:// or https:// URLs.".to_owned());
    }
    Ok(trimmed.to_owned())
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Missing required browser argument: {key}"))
}

fn truncate(value: &str) -> String {
    if value.chars().count() <= MAX_BROWSER_OUTPUT {
        value.to_owned()
    } else {
        format!(
            "{}\n... browser output truncated by HAWK Code ...",
            value.chars().take(MAX_BROWSER_OUTPUT).collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_web_urls() {
        assert!(safe_url("file:///C:/Windows/win.ini").is_err());
        assert!(safe_url("javascript:alert(1)").is_err());
        assert!(safe_url("https://example.com").is_ok());
    }

    #[test]
    fn maps_browser_actions_to_playwright_commands() {
        let args = serde_json::json!({"action": "fill", "target": "e5", "value": "hello"});
        assert_eq!(
            command_args("fill", &args).expect("fill should be valid"),
            vec!["fill", "e5", "hello"]
        );
    }
}
