use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use keyring::Entry;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex, time::Instant};

const ACCOUNT_SERVICE: &str = "com.hawkstudio.code.local-account";
const MAX_FAILURES: u8 = 5;
const LOCK_SECONDS: u64 = 30;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPayload {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginPayload {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthProfile {
    pub provider: &'static str,
    pub name: String,
    pub email: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAccountRecord {
    version: u8,
    name: String,
    email: String,
    password_hash: String,
}

#[derive(Debug, Default)]
struct LoginAttempt {
    failures: u8,
    locked_until: Option<Instant>,
}

#[derive(Default)]
pub struct AuthRuntime {
    attempts: Mutex<HashMap<String, LoginAttempt>>,
}

fn normalize_email(raw: &str) -> Result<String, String> {
    let email = raw.trim().to_ascii_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return Err("Enter a valid email address.".to_owned());
    };
    if email.len() > 254
        || local.is_empty()
        || domain.len() < 3
        || !domain.contains('.')
        || email.chars().any(char::is_whitespace)
    {
        return Err("Enter a valid email address.".to_owned());
    }
    Ok(email)
}

fn validate_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if !(2..=80).contains(&name.chars().count()) || name.chars().any(char::is_control) {
        return Err("Your name must contain between 2 and 80 characters.".to_owned());
    }
    Ok(name.to_owned())
}

fn validate_password(password: &str, email: &str) -> Result<(), String> {
    let lower = password.to_lowercase();
    let email_name = email.split('@').next().unwrap_or_default();
    let common = [
        "password",
        "password123",
        "123456789",
        "qwerty123",
        "admin123",
    ];
    if password.chars().count() < 12 || password.chars().count() > 128 {
        return Err("Password must contain between 12 and 128 characters.".to_owned());
    }
    if password.chars().any(char::is_whitespace)
        || !password.chars().any(char::is_lowercase)
        || !password.chars().any(char::is_uppercase)
        || !password.chars().any(|character| character.is_ascii_digit())
        || !password
            .chars()
            .any(|character| !character.is_alphanumeric())
    {
        return Err("Use uppercase, lowercase, a number, and a symbol with no spaces.".to_owned());
    }
    if common.contains(&lower.as_str()) || (email_name.len() >= 4 && lower.contains(email_name)) {
        return Err(
            "Choose a password that is not based on your email or a common phrase.".to_owned(),
        );
    }
    Ok(())
}

fn account_entry(email: &str) -> Result<Entry, String> {
    Entry::new(ACCOUNT_SERVICE, email)
        .map_err(|_| "Windows Credential Manager is unavailable.".to_owned())
}

fn profile(record: LocalAccountRecord) -> AuthProfile {
    AuthProfile {
        provider: "local",
        name: record.name,
        email: record.email,
        avatar_url: None,
    }
}

pub fn register(payload: RegisterPayload) -> Result<AuthProfile, String> {
    let email = normalize_email(&payload.email)?;
    let name = validate_name(&payload.name)?;
    validate_password(&payload.password, &email)?;
    let entry = account_entry(&email)?;
    match entry.get_password() {
        Ok(_) => return Err("A local account already exists for this email.".to_owned()),
        Err(keyring::Error::NoEntry) => {}
        Err(_) => return Err("The local account store could not be read.".to_owned()),
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.password.as_bytes(), &salt)
        .map_err(|_| "The password could not be protected.".to_owned())?
        .to_string();
    let record = LocalAccountRecord {
        version: 1,
        name,
        email,
        password_hash,
    };
    let encoded = serde_json::to_string(&record)
        .map_err(|_| "The local account could not be encoded.".to_owned())?;
    entry
        .set_password(&encoded)
        .map_err(|_| "The local account could not be saved securely.".to_owned())?;
    Ok(profile(record))
}

impl AuthRuntime {
    pub fn login(&self, payload: LoginPayload) -> Result<AuthProfile, String> {
        let email = normalize_email(&payload.email)?;
        {
            let mut attempts = self
                .attempts
                .lock()
                .map_err(|_| "The login guard is unavailable.".to_owned())?;
            if let Some(attempt) = attempts.get_mut(&email) {
                if let Some(until) = attempt.locked_until {
                    if until > Instant::now() {
                        return Err("Too many failed attempts. Try again in 30 seconds.".to_owned());
                    }
                    *attempt = LoginAttempt::default();
                }
            }
        }

        let record = account_entry(&email)?
            .get_password()
            .ok()
            .and_then(|encoded| serde_json::from_str::<LocalAccountRecord>(&encoded).ok());
        let verified = record.as_ref().is_some_and(|account| {
            PasswordHash::new(&account.password_hash)
                .ok()
                .is_some_and(|hash| {
                    Argon2::default()
                        .verify_password(payload.password.as_bytes(), &hash)
                        .is_ok()
                })
        });
        if !verified {
            let mut attempts = self
                .attempts
                .lock()
                .map_err(|_| "The login guard is unavailable.".to_owned())?;
            let attempt = attempts.entry(email).or_default();
            attempt.failures = attempt.failures.saturating_add(1);
            if attempt.failures >= MAX_FAILURES {
                attempt.locked_until =
                    Some(Instant::now() + std::time::Duration::from_secs(LOCK_SECONDS));
            }
            return Err("The email or password is incorrect.".to_owned());
        }
        self.attempts
            .lock()
            .map_err(|_| "The login guard is unavailable.".to_owned())?
            .remove(&email);
        Ok(profile(record.expect("verified records are present")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_weak_passwords() {
        assert!(validate_password("password123", "person@example.com").is_err());
        assert!(validate_password("Strong-Password-2026!", "person@example.com").is_ok());
    }

    #[test]
    fn normalizes_valid_email_addresses() {
        assert_eq!(
            normalize_email("  Person@Example.COM ").expect("email should be valid"),
            "person@example.com"
        );
    }
}
