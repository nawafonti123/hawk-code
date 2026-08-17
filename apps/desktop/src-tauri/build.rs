use std::{fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=.env.local");
    load_public_build_config(Path::new(".env.local"));
    tauri_build::build()
}

fn load_public_build_config(path: &Path) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if matches!(
            key,
            "HAWK_GOOGLE_OAUTH_CLIENT_ID" | "HAWK_GOOGLE_OAUTH_CLIENT_SECRET"
        ) {
            let value = value.trim().trim_matches(['\"', '\'']);
            if !value.is_empty() && !value.contains(['\r', '\n']) {
                println!("cargo:rustc-env={key}={value}");
            }
        }
    }
}
