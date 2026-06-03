// sky_auth.rs
//
// Blackbaud SKY API OAuth 2.0 — authorization-code flow for a desktop app.
//
// Because Tauri has no browser of its own, we run the loopback redirect
// pattern recommended for installed apps:
//   1. Bind a TcpListener on a fixed localhost port.
//   2. Open the system browser at Blackbaud's /authorization endpoint.
//   3. The user logs in to their Raiser's Edge NXT (test/cohort) environment
//      and consents; Blackbaud redirects back to http://localhost:<port>/callback
//      with a one-time `code`.
//   4. We read that code off the loopback socket and exchange it at the
//      /token endpoint for an access_token + refresh_token, then persist
//      the whole connection to app_data_dir.
//
// The persisted Connection holds the credentials needed to silently refresh
// the access token (valid ~60 min) for subsequent API calls. Both the
// Authorization: Bearer header AND the Bb-Api-Subscription-Key header are
// required on every SKY API request — see valid_access_token / the
// subscription_key field.
//
// NOTE (security): the connection file is plaintext JSON on disk, which is
// fine for a test/cohort environment. For production, store the secret and
// refresh token in the OS keychain instead.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::errors::AppError;

// Blackbaud SKY API OAuth endpoints (same for all environments).
const AUTH_URL: &str = "https://oauth2.sky.blackbaud.com/authorization";
const TOKEN_URL: &str = "https://oauth2.sky.blackbaud.com/token";

// Fixed loopback redirect. This exact string must be registered as a
// Redirect URI on the application in the Blackbaud developer portal, or the
// token exchange returns redirect_uri_mismatch.
const REDIRECT_URI: &str = "http://localhost:13631/callback";
const LOOPBACK_ADDR: &str = "127.0.0.1:13631";

// Refresh a little before the token actually expires so an in-flight request
// never races the expiry.
const EXPIRY_SKEW_SECS: i64 = 60;

// ── Persisted connection ───────────────────────────────────────────────────────
// Everything needed to make authenticated calls and to refresh silently.

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

// ── Status surfaced to the frontend ────────────────────────────────────────────
// Deliberately omits tokens/secret — only what the UI needs to render state.

#[derive(Serialize)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub environment_id: Option<String>,
    pub environment_name: Option<String>,
    pub expires_at: Option<i64>,
}

// Blackbaud's token response. environment_id / environment_name identify which
// RE NXT environment the user authorized against (returned on both grant types).
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    environment_id: Option<String>,
    #[serde(default)]
    environment_name: Option<String>,
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

fn connection_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::IoError(e.to_string()))?;
    std::fs::create_dir_all(&dir).map_err(|e| AppError::IoError(e.to_string()))?;
    Ok(dir.join("re_nxt_connection.json"))
}

fn load_connection(app: &AppHandle) -> Result<Option<Connection>, AppError> {
    let path = connection_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| AppError::IoError(e.to_string()))?;
    let conn = serde_json::from_slice(&bytes).map_err(|e| AppError::ParseError(e.to_string()))?;
    Ok(Some(conn))
}

fn save_connection(app: &AppHandle, conn: &Connection) -> Result<(), AppError> {
    let path = connection_path(app)?;
    let json = serde_json::to_vec_pretty(conn).map_err(|e| AppError::ParseError(e.to_string()))?;
    std::fs::write(&path, json).map_err(|e| AppError::IoError(e.to_string()))
}

fn status_from(conn: &Connection) -> ConnectionStatus {
    ConnectionStatus {
        connected: true,
        environment_id: conn.environment_id.clone(),
        environment_name: conn.environment_name.clone(),
        expires_at: Some(conn.expires_at),
    }
}

// ── OAuth flow (blocking) ───────────────────────────────────────────────────────

// Runs the full interactive connect: spins the loopback server, opens the
// browser, waits for the redirect, exchanges the code, persists the result.
// Blocking by design — the caller runs it on a blocking thread.
fn connect_blocking(
    app: &AppHandle,
    client_id: String,
    client_secret: String,
    subscription_key: String,
) -> Result<ConnectionStatus, AppError> {
    // Bind first — if the port is taken we want to fail before opening a browser.
    let listener = TcpListener::bind(LOOPBACK_ADDR).map_err(|e| {
        AppError::IoError(format!(
            "Could not bind {LOOPBACK_ADDR} for the OAuth redirect: {e}"
        ))
    })?;

    let state = random_state();
    let auth_url = format!(
        "{AUTH_URL}?client_id={}&response_type=code&redirect_uri={}&state={}",
        urlencode(&client_id),
        urlencode(REDIRECT_URI),
        urlencode(&state),
    );
    open_url(&auth_url);

    let code = wait_for_code(&listener, &state)?;
    let token = exchange_code(&client_id, &client_secret, &code)?;

    let conn = Connection {
        client_id,
        client_secret,
        subscription_key,
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: now_secs() + token.expires_in,
        environment_id: token.environment_id,
        environment_name: token.environment_name,
    };
    save_connection(app, &conn)?;
    Ok(status_from(&conn))
}

// Accepts loopback connections until one hits /callback, then returns the
// authorization code. Validates the state nonce to guard against CSRF.
fn wait_for_code(listener: &TcpListener, expected_state: &str) -> Result<String, AppError> {
    for stream in listener.incoming() {
        let mut stream = stream.map_err(|e| AppError::IoError(e.to_string()))?;

        let request_line = {
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| AppError::IoError(e.to_string()))?;
            line
        };

        // "GET /callback?code=...&state=... HTTP/1.1"
        let path = request_line.split_whitespace().nth(1).unwrap_or("");
        if !path.starts_with("/callback") {
            // Browsers probe /favicon.ico etc. — keep waiting for the real hit.
            respond(&mut stream, "Waiting for authorization…");
            continue;
        }

        let query = path.splitn(2, '?').nth(1).unwrap_or("");
        let mut code: Option<String> = None;
        let mut state: Option<String> = None;
        let mut error: Option<String> = None;
        for pair in query.split('&') {
            let mut it = pair.splitn(2, '=');
            match (it.next(), it.next()) {
                (Some("code"), Some(v)) => code = Some(urldecode(v)),
                (Some("state"), Some(v)) => state = Some(urldecode(v)),
                (Some("error"), Some(v)) => error = Some(urldecode(v)),
                _ => {}
            }
        }

        respond(
            &mut stream,
            "Connected to Raiser's Edge NXT. You can close this tab and return to the app.",
        );

        if let Some(err) = error {
            return Err(AppError::AuthError(format!(
                "Authorization was denied or failed: {err}"
            )));
        }
        if state.as_deref() != Some(expected_state) {
            return Err(AppError::AuthError(
                "OAuth state mismatch — aborting for safety.".into(),
            ));
        }
        return code.ok_or_else(|| {
            AppError::AuthError("Redirect carried no authorization code.".into())
        });
    }
    Err(AppError::IoError(
        "Loopback listener closed before the redirect arrived.".into(),
    ))
}

fn respond(stream: &mut TcpStream, body: &str) {
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Import Tool</title>\
         </head><body style=\"font-family:system-ui;padding:3rem;text-align:center\">\
         <p>{body}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

// POST the authorization code to the token endpoint. Credentials go via HTTP
// Basic auth (client_id:client_secret), which Blackbaud accepts.
fn exchange_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
) -> Result<TokenResponse, AppError> {
    post_token(
        client_id,
        client_secret,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
        ],
    )
}

fn refresh_token(conn: &Connection) -> Result<TokenResponse, AppError> {
    post_token(
        &conn.client_id,
        &conn.client_secret,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &conn.refresh_token),
        ],
    )
}

fn post_token(
    client_id: &str,
    client_secret: &str,
    form: &[(&str, &str)],
) -> Result<TokenResponse, AppError> {
    let client = reqwest::blocking::Client::new();
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

// Returns a usable access token, refreshing (and re-persisting) if the stored
// one is within EXPIRY_SKEW_SECS of expiry. The single accessor every API call
// should go through.
fn valid_access_token(app: &AppHandle) -> Result<String, AppError> {
    let mut conn = load_connection(app)?
        .ok_or_else(|| AppError::AuthError("Not connected to Raiser's Edge NXT.".into()))?;

    if conn.expires_at - EXPIRY_SKEW_SECS > now_secs() {
        return Ok(conn.access_token);
    }

    let token = refresh_token(&conn)?;
    conn.access_token = token.access_token;
    conn.refresh_token = token.refresh_token;
    conn.expires_at = now_secs() + token.expires_in;
    if token.environment_id.is_some() {
        conn.environment_id = token.environment_id;
        conn.environment_name = token.environment_name;
    }
    save_connection(app, &conn)?;
    Ok(conn.access_token)
}

// ── small helpers (no extra deps) ───────────────────────────────────────────────

// Opaque CSRF nonce — not a secret, just needs to be unguessable per-attempt.
fn random_state() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}{:x}", nanos, std::process::id())
}

fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

// Minimal percent-encoding for query values (RFC 3986 unreserved set kept).
fn urlencode(s: &str) -> String {
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

// Minimal percent-decoding for the redirect query (handles %XX and '+').
fn urldecode(s: &str) -> String {
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

// ── Tauri commands ──────────────────────────────────────────────────────────────

// Interactive connect. Opens the browser; resolves once the user finishes the
// OAuth handshake against their RE NXT environment. Run on a blocking thread
// because the loopback wait and the HTTP exchange both block.
#[tauri::command]
pub async fn connect_re_nxt(
    app: AppHandle,
    client_id: String,
    client_secret: String,
    subscription_key: String,
) -> Result<ConnectionStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        connect_blocking(&app, client_id, client_secret, subscription_key)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

// Cheap status check for App.tsx on mount — does no network I/O.
#[tauri::command]
pub fn re_nxt_status(app: AppHandle) -> Result<ConnectionStatus, String> {
    match load_connection(&app).map_err(|e| e.to_string())? {
        Some(conn) => Ok(status_from(&conn)),
        None => Ok(ConnectionStatus {
            connected: false,
            environment_id: None,
            environment_name: None,
            expires_at: None,
        }),
    }
}

// Forget the stored connection (tokens + credentials).
#[tauri::command]
pub fn disconnect_re_nxt(app: AppHandle) -> Result<(), String> {
    let path = connection_path(&app).map_err(|e| e.to_string())?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// Returns a fresh access token (refreshing if needed). The Data Requests /
// Reports code will call this before each SKY API request and pair it with
// the stored subscription key.
#[tauri::command]
pub async fn re_nxt_access_token(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || valid_access_token(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
