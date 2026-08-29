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
// The Connection model, its on-disk format, and the token exchange/refresh
// calls live in multitool_core::creds — shared with the web shell, which runs
// the same handshake through its own /api/oauth/callback route instead of a
// loopback listener. Only the code-acquisition half is desktop-specific.
//
// NOTE (security): the connection file is plaintext JSON on disk, which is
// fine for a test/cohort environment. For production, store the secret and
// refresh token in the OS keychain instead.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use multitool_core::creds::{self, TokenResponse};
use multitool_core::errors::AppError;

// Fixed loopback redirect. This exact string must be registered as a
// Redirect URI on the application in the Blackbaud developer portal, or the
// token exchange returns redirect_uri_mismatch.
const REDIRECT_URI: &str = "http://localhost:13631/callback";
const LOOPBACK_ADDR: &str = "127.0.0.1:13631";

// ── Persisted connection ───────────────────────────────────────────────────────
// Model + persistence + refresh all come from core; this shell only decides
// *where* the file lives (per-user app data dir).

pub use multitool_core::creds::{Connection, ConnectionStatus};

fn connection_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::IoError(e.to_string()))?;
    std::fs::create_dir_all(&dir).map_err(|e| AppError::IoError(e.to_string()))?;
    Ok(dir.join("re_nxt_connection.json"))
}

fn load_connection(app: &AppHandle) -> Result<Option<Connection>, AppError> {
    creds::load_connection(&connection_path(app)?)
}

fn save_connection(app: &AppHandle, conn: &Connection) -> Result<(), AppError> {
    creds::save_connection(&connection_path(app)?, conn)
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

    let state = creds::random_state();
    open_url(&creds::authorize_url(&client_id, REDIRECT_URI, &state));

    let code = wait_for_code(&listener, &state)?;
    let token = exchange_code(&client_id, &client_secret, &code)?;

    let conn = Connection {
        client_id,
        client_secret,
        subscription_key,
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: creds::now_secs() + token.expires_in,
        environment_id: token.environment_id,
        environment_name: token.environment_name,
    };
    save_connection(app, &conn)?;
    Ok(conn.status())
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
                (Some("code"), Some(v)) => code = Some(creds::urldecode(v)),
                (Some("state"), Some(v)) => state = Some(creds::urldecode(v)),
                (Some("error"), Some(v)) => error = Some(creds::urldecode(v)),
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

// POST the authorization code to the token endpoint.
fn exchange_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
) -> Result<TokenResponse, AppError> {
    creds::exchange_code(client_id, client_secret, code, REDIRECT_URI)
}

// Returns a usable access token, refreshing (and re-persisting) if the stored
// one is within EXPIRY_SKEW_SECS of expiry. The single accessor every API call
// should go through.
fn valid_access_token(app: &AppHandle) -> Result<String, AppError> {
    let mut conn = load_connection(app)?
        .ok_or_else(|| AppError::AuthError("Not connected to Raiser's Edge NXT.".into()))?;
    // Blackbaud spends the refresh token on use, so persist immediately.
    if creds::ensure_fresh(&mut conn)? {
        save_connection(app, &conn)?;
    }
    Ok(conn.access_token)
}

// ── Internal accessors for SKY API callers (report.rs / re_calls.rs) ────────────

// True when a connection file exists — cheap, no network. Used to decide whether
// the report pipeline runs live or falls back to mock fixtures.
pub fn has_connection(app: &AppHandle) -> bool {
    matches!(load_connection(app), Ok(Some(_)))
}

// A fresh access token (refreshing if needed) paired with the stored
// subscription key — the two headers every SKY API request needs. Blocking;
// call from a blocking thread (e.g. via spawn_blocking), never on the async
// runtime — reqwest::blocking panics inside a Tokio context.
pub fn live_credentials(app: &AppHandle) -> Result<(String, String), AppError> {
    let conn = load_connection(app)?
        .ok_or_else(|| AppError::AuthError("Not connected to Raiser's Edge NXT.".into()))?;
    let token = valid_access_token(app)?;
    Ok((token, conn.subscription_key))
}

// ── small helpers (no extra deps) ───────────────────────────────────────────────

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
    tokio::task::spawn_blocking(move || {
        connect_blocking(&app, client_id, client_secret, subscription_key)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

// Cheap status check for App.tsx on mount — does no network I/O.
#[tauri::command]
pub fn re_nxt_status(app: AppHandle) -> Result<ConnectionStatus, String> {
    let mock_forced = std::env::var("RE_NXT_MOCK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let mut status = match load_connection(&app).map_err(|e| e.to_string())? {
        Some(conn) => conn.status(),
        None => ConnectionStatus::default(),
    };
    status.mock_forced = mock_forced;
    Ok(status)
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
    tokio::task::spawn_blocking(move || valid_access_token(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
