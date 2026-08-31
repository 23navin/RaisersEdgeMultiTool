// creds.rs
//
// The RE NXT connection: what it is, how it's persisted, and how its access
// token gets refreshed. Shared by both shells so the on-disk format is
// identical — a connection established on desktop can be dropped onto a
// server and vice versa.
//
// What differs per shell is only how the authorization *code* is obtained:
//   desktop — loopback listener on 127.0.0.1:13631 + system browser
//             (sky_auth.rs; the pattern for apps with no addressable URL)
//   web     — the server's own /api/oauth/callback route
//             (crates/server; a server already has a URL, so no listener)
// Both then call exchange_code() here, and both refresh through the same path.
//
// Blocking (reqwest::blocking) — call from a blocking thread, never on an
// async runtime.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::errors::AppError;

pub const AUTH_URL: &str = "https://oauth2.sky.blackbaud.com/authorization";
pub const TOKEN_URL: &str = "https://oauth2.sky.blackbaud.com/token";

// Refresh a little before the token actually expires so an in-flight request
// never races the expiry.
pub const EXPIRY_SKEW_SECS: i64 = 60;

// ── Persisted connection ──────────────────────────────────────────────────────
// Everything needed to make authenticated calls and to refresh silently.
//
// NOTE (security): written as plaintext JSON. On desktop that's a per-user
// file; on a server it lives on the data volume. Either way it holds the
// client secret and refresh token — protect the file, and prefer an OS
// keychain / secret manager for production.

#[derive(Serialize, Deserialize, Clone)]
pub struct Connection {
    pub client_id: String,
    pub client_secret: String,
    pub subscription_key: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64, // unix seconds — when access_token stops being valid
    pub environment_id: Option<String>,
    pub environment_name: Option<String>,
}

// What the settings UI renders. Deliberately omits tokens/secret.
//
// `mock_forced` is reported separately from `connected` on purpose: a
// deployment can hold a perfectly good connection while RE_NXT_MOCK pins every
// call to fixtures. Folding that into `connected: false` made a successful
// sign-in look like a silent failure.
#[derive(Serialize, Clone, Default)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub environment_id: Option<String>,
    pub environment_name: Option<String>,
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub mock_forced: bool,
}

impl Connection {
    pub fn status(&self) -> ConnectionStatus {
        ConnectionStatus {
            connected: true,
            environment_id: self.environment_id.clone(),
            environment_name: self.environment_name.clone(),
            expires_at: Some(self.expires_at),
            mock_forced: false, // the shell fills this in from its config
        }
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Persistence ───────────────────────────────────────────────────────────────

pub fn load_connection(path: &Path) -> Result<Option<Connection>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|e| AppError::IoError(e.to_string()))?;
    let conn = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::ParseError(format!("Bad connection file: {}", e)))?;
    Ok(Some(conn))
}

pub fn save_connection(path: &Path, conn: &Connection) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::IoError(e.to_string()))?;
    }
    let json =
        serde_json::to_vec_pretty(conn).map_err(|e| AppError::ParseError(e.to_string()))?;
    fs::write(path, json).map_err(|e| AppError::IoError(e.to_string()))
}

pub fn delete_connection(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| AppError::IoError(e.to_string()))?;
    }
    Ok(())
}

// ── Token endpoint ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    #[serde(default)]
    pub environment_id: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
}

// POST to the token endpoint. Credentials go via HTTP Basic auth
// (client_id:client_secret), which Blackbaud accepts.
pub fn post_token(
    client_id: &str,
    client_secret: &str,
    form: &[(&str, &str)],
) -> Result<TokenResponse, AppError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::NetworkError(e.to_string()))?;
    let resp = client
        .post(TOKEN_URL)
        .basic_auth(client_id, Some(client_secret))
        .form(form)
        .send()
        .map_err(|e| AppError::NetworkError(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(AppError::AuthError(format!(
            "Token endpoint returned {status}: {body}"
        )));
    }
    resp.json::<TokenResponse>()
        .map_err(|e| AppError::ParseError(e.to_string()))
}

// Exchange an authorization code for the initial token pair. `redirect_uri`
// must byte-match the one sent to /authorization and registered on the app.
pub fn exchange_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, AppError> {
    post_token(
        client_id,
        client_secret,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ],
    )
}

// Exchange a refresh token for a fresh access token. Blackbaud rotates the
// refresh token on every use, so the caller must persist the returned one.
pub fn refresh(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<TokenResponse, AppError> {
    post_token(
        client_id,
        client_secret,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
}

// Bring `conn` up to date in place, refreshing when the access token is at or
// near expiry. Returns true when a refresh happened — the caller must then
// persist, because the old refresh token is now spent.
pub fn ensure_fresh(conn: &mut Connection) -> Result<bool, AppError> {
    if conn.expires_at - EXPIRY_SKEW_SECS > now_secs() {
        return Ok(false);
    }
    let token = refresh(&conn.client_id, &conn.client_secret, &conn.refresh_token)?;
    conn.access_token = token.access_token;
    conn.refresh_token = token.refresh_token;
    conn.expires_at = now_secs() + token.expires_in;
    if token.environment_id.is_some() {
        conn.environment_id = token.environment_id;
        conn.environment_name = token.environment_name;
    }
    Ok(true)
}

// Build the URL the user's browser is sent to in order to sign in and consent.
pub fn authorize_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    format!(
        "{AUTH_URL}?client_id={}&response_type=code&redirect_uri={}&state={}",
        urlencode(client_id),
        urlencode(redirect_uri),
        urlencode(state),
    )
}

// ── URL helpers + CSRF state ──────────────────────────────────────────────────

// Unguessable per-attempt CSRF nonce. Unlike a clock/pid-derived value this
// is actually random, which matters on the web where the callback is a public
// route anyone can hit.
pub fn random_state() -> String {
    let mut buf = [0u8; 32];
    if getrandom::getrandom(&mut buf).is_err() {
        // Fall back to a time/counter mix rather than failing the connect.
        let n = now_secs() as u128 * 1_000_000 + std::process::id() as u128;
        buf[..16].copy_from_slice(&n.to_le_bytes());
    }
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

// Minimal percent-encoding for query values (RFC 3986 unreserved set kept).
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// Minimal percent-decoding for a redirect query (handles %XX and '+').
pub fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_is_random_and_long() {
        let a = random_state();
        let b = random_state();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b, "two nonces must differ");
    }

    #[test]
    fn url_roundtrip() {
        let s = "http://localhost:13631/callback?x=1 2&y=é";
        assert_eq!(urldecode(&urlencode(s)), s);
    }

    #[test]
    fn authorize_url_encodes_redirect() {
        let u = authorize_url("cid", "https://app.example.org/api/oauth/callback", "st");
        assert!(u.contains("client_id=cid"));
        assert!(u.contains("redirect_uri=https%3A%2F%2Fapp.example.org%2Fapi%2Foauth%2Fcallback"));
        assert!(u.contains("state=st"));
    }
}
