use crate::auth::AuthProfile;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use keyring::Entry;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::Url;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const GOOGLE_TOKEN_SERVICE: &str = "com.hawkstudio.code.oauth.google";
const CALLBACK_PATH: &str = "/oauth/google/callback";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthProviderStatus {
    provider: &'static str,
    configured: bool,
}

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct GoogleTokenError {
    error: String,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    name: Option<String>,
    email: String,
    email_verified: Option<bool>,
    picture: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredGoogleTokens {
    version: u8,
    access_token: String,
    refresh_token: Option<String>,
    expires_at_unix: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct OAuthCallback {
    code: String,
}

pub fn provider_statuses() -> Vec<OAuthProviderStatus> {
    vec![
        OAuthProviderStatus {
            provider: "google",
            configured: google_client_id().is_some() && google_client_secret().is_some(),
        },
        OAuthProviderStatus {
            provider: "github",
            configured: false,
        },
        OAuthProviderStatus {
            provider: "facebook",
            configured: false,
        },
    ]
}

pub async fn login_google() -> Result<AuthProfile, String> {
    let client_id = google_client_id()
        .ok_or_else(|| "Google sign-in is not configured for this HAWK Code build.".to_owned())?;
    let client_secret = google_client_secret()
        .ok_or_else(|| "Google sign-in is not configured for this HAWK Code build.".to_owned())?;
    let state = random_url_token(32);
    let verifier = random_url_token(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| "HAWK could not start the secure Google sign-in callback.".to_owned())?;
    let port = listener
        .local_addr()
        .map_err(|_| "HAWK could not prepare Google sign-in.".to_owned())?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
    let authorization_url =
        build_google_authorization_url(client_id, &redirect_uri, &state, &challenge)?;

    open_system_browser(authorization_url.as_str())?;
    let expected_state = state.clone();
    let callback = tokio::task::spawn_blocking(move || {
        wait_for_callback(listener, &expected_state, CALLBACK_TIMEOUT)
    })
    .await
    .map_err(|_| "Google sign-in was interrupted.".to_owned())??;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "HAWK could not prepare the secure Google connection.".to_owned())?;
    let token_form = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", callback.code.as_str()),
        ("code_verifier", verifier.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri.as_str()),
    ];
    let token_response = client
        .post(GOOGLE_TOKEN_URL)
        .form(&token_form)
        .send()
        .await
        .map_err(|_| {
            "Google did not complete sign-in. Check your connection and try again.".to_owned()
        })?;
    if !token_response.status().is_success() {
        let error_code = token_response
            .json::<GoogleTokenError>()
            .await
            .ok()
            .map(|error| error.error)
            .filter(|error| {
                matches!(
                    error.as_str(),
                    "invalid_client"
                        | "invalid_grant"
                        | "redirect_uri_mismatch"
                        | "unauthorized_client"
                )
            });
        return Err(match error_code {
            Some(code) => {
                format!("Google rejected the sign-in response ({code}). Please try again.")
            }
            None => "Google rejected the sign-in response. Please try again.".to_owned(),
        });
    }
    let tokens = token_response
        .json::<GoogleTokenResponse>()
        .await
        .map_err(|_| "Google returned an invalid sign-in response.".to_owned())?;
    if tokens.access_token.is_empty() {
        return Err("Google returned an incomplete sign-in response.".to_owned());
    }

    let user_response = client
        .get(GOOGLE_USERINFO_URL)
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|_| "HAWK could not retrieve your Google profile.".to_owned())?;
    if !user_response.status().is_success() {
        return Err("Google did not provide a valid profile.".to_owned());
    }
    let user = user_response
        .json::<GoogleUserInfo>()
        .await
        .map_err(|_| "Google returned an invalid profile.".to_owned())?;
    if user.email_verified != Some(true) || !looks_like_email(&user.email) {
        return Err("Google did not provide a verified email address.".to_owned());
    }

    store_google_tokens(&user.email, tokens)?;
    Ok(AuthProfile {
        provider: "google",
        name: clean_profile_name(user.name, &user.email),
        email: user.email,
        avatar_url: safe_avatar_url(user.picture),
    })
}

fn google_client_id() -> Option<&'static str> {
    option_env!("HAWK_GOOGLE_OAUTH_CLIENT_ID")
        .map(str::trim)
        .filter(|value| value.ends_with(".apps.googleusercontent.com") && value.len() <= 256)
}

fn google_client_secret() -> Option<&'static str> {
    option_env!("HAWK_GOOGLE_OAUTH_CLIENT_SECRET")
        .map(str::trim)
        .filter(|value| value.starts_with("GOCSPX-") && value.len() <= 256)
}

fn random_url_token(byte_count: usize) -> String {
    let mut bytes = vec![0_u8; byte_count];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn build_google_authorization_url(
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> Result<Url, String> {
    let mut url = Url::parse(GOOGLE_AUTH_URL)
        .map_err(|_| "HAWK could not prepare the Google authorization URL.".to_owned())?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "openid email profile")
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("access_type", "offline")
        .append_pair("prompt", "select_account consent");
    Ok(url)
}

fn open_system_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(url)
            .spawn()
            .map_err(|_| "HAWK could not open the browser for Google sign-in.".to_owned())?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|_| "HAWK could not open the browser for Google sign-in.".to_owned())?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|_| "HAWK could not open the browser for Google sign-in.".to_owned())?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err("Google sign-in is not supported on this platform.".to_owned())
}

fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> Result<OAuthCallback, String> {
    listener
        .set_nonblocking(true)
        .map_err(|_| "HAWK could not monitor the Google sign-in callback.".to_owned())?;
    let started = Instant::now();
    while started.elapsed() < timeout {
        match listener.accept() {
            Ok((mut stream, _)) => match read_callback(&mut stream, expected_state) {
                Ok(callback) => {
                    write_browser_response(&mut stream, true);
                    return Ok(callback);
                }
                Err(error) => {
                    write_browser_response(&mut stream, false);
                    if error == "The Google sign-in request was cancelled." {
                        return Err(error);
                    }
                }
            },
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return Err("The Google sign-in callback failed.".to_owned()),
        }
    }
    Err("Google sign-in timed out. Please try again.".to_owned())
}

fn read_callback(stream: &mut TcpStream, expected_state: &str) -> Result<OAuthCallback, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| "The Google sign-in callback failed.".to_owned())?;
    let mut buffer = [0_u8; 8192];
    let length = stream
        .read(&mut buffer)
        .map_err(|_| "The Google sign-in callback failed.".to_owned())?;
    let request = std::str::from_utf8(&buffer[..length])
        .map_err(|_| "The Google sign-in callback was invalid.".to_owned())?;
    let mut request_line = request
        .lines()
        .next()
        .ok_or_else(|| "The Google sign-in callback was invalid.".to_owned())?
        .split_whitespace();
    if request_line.next() != Some("GET") {
        return Err("The Google sign-in callback was invalid.".to_owned());
    }
    let target = request_line
        .next()
        .ok_or_else(|| "The Google sign-in callback was invalid.".to_owned())?;
    parse_callback_target(target, expected_state)
}

fn parse_callback_target(target: &str, expected_state: &str) -> Result<OAuthCallback, String> {
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "The Google sign-in callback was invalid.".to_owned())?;
    if url.path() != CALLBACK_PATH {
        return Err("The Google sign-in callback was invalid.".to_owned());
    }
    let mut codes = Vec::new();
    let mut states = Vec::new();
    let mut cancelled = false;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => codes.push(value.into_owned()),
            "state" => states.push(value.into_owned()),
            "error" => cancelled = true,
            _ => {}
        }
    }
    if cancelled {
        return Err("The Google sign-in request was cancelled.".to_owned());
    }
    if states.len() != 1 || states.first().map(String::as_str) != Some(expected_state) {
        return Err("The Google sign-in security check failed.".to_owned());
    }
    let code = (codes.len() == 1)
        .then(|| codes.remove(0))
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .ok_or_else(|| "Google did not return an authorization code.".to_owned())?;
    Ok(OAuthCallback { code })
}

fn write_browser_response(stream: &mut TcpStream, success: bool) {
    let (title, body) = if success {
        (
            "Sign-in complete",
            "You can close this window and return to HAWK Code.",
        )
    } else {
        (
            "Sign-in not completed",
            "Return to HAWK Code and try again.",
        )
    };
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{title}</title><style>body{{font-family:system-ui;background:#0d0f0d;color:#f5f3eb;display:grid;place-items:center;min-height:100vh;margin:0}}main{{max-width:34rem;padding:2rem;text-align:center}}h1{{font-size:1.5rem}}p{{color:#a8ada7}}</style></head><body><main><h1>{title}</h1><p>{body}</p></main></body></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn store_google_tokens(email: &str, tokens: GoogleTokenResponse) -> Result<(), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "HAWK could not timestamp the Google session.".to_owned())?
        .as_secs();
    let record = StoredGoogleTokens {
        version: 1,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at_unix: now.saturating_add(tokens.expires_in),
    };
    let encoded = serde_json::to_string(&record)
        .map_err(|_| "HAWK could not protect the Google session.".to_owned())?;
    Entry::new(GOOGLE_TOKEN_SERVICE, email)
        .map_err(|_| "Windows Credential Manager is unavailable.".to_owned())?
        .set_password(&encoded)
        .map_err(|_| "HAWK could not save the Google session securely.".to_owned())
}

fn looks_like_email(email: &str) -> bool {
    email.len() <= 254
        && !email.chars().any(char::is_whitespace)
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.len() >= 3 && domain.contains('.')
        })
}

fn clean_profile_name(name: Option<String>, email: &str) -> String {
    name.map(|value| value.trim().chars().take(80).collect::<String>())
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .unwrap_or_else(|| email.split('@').next().unwrap_or("HAWK user").to_owned())
}

fn safe_avatar_url(value: Option<String>) -> Option<String> {
    value.filter(|candidate| {
        candidate.len() <= 2048 && Url::parse(candidate).is_ok_and(|url| url.scheme() == "https")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_callback() {
        assert_eq!(
            parse_callback_target(
                "/oauth/google/callback?code=secure-code&state=expected",
                "expected"
            ),
            Ok(OAuthCallback {
                code: "secure-code".to_owned()
            })
        );
    }

    #[test]
    fn rejects_state_mismatch() {
        assert!(parse_callback_target(
            "/oauth/google/callback?code=secure-code&state=attacker",
            "expected"
        )
        .is_err());
    }

    #[test]
    fn rejects_wrong_callback_path() {
        assert!(parse_callback_target("/favicon.ico?state=expected", "expected").is_err());
    }
}
