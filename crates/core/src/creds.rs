// creds.rs
//
// The SKY OAuth token-endpoint call, shared by both shells. The desktop shell
// (sky_auth.rs) owns the interactive loopback flow and its per-machine
// connection file; the web shell owns a service-account credential store fed
// by env vars. Both refresh through this one function.
//
// Blocking (reqwest::blocking) — call from a blocking thread, never on an
// async runtime.

use serde::Deserialize;

use crate::errors::AppError;

pub const TOKEN_URL: &str = "https://oauth2.sky.blackbaud.com/token";

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

// Exchange a refresh token for a fresh access token (rotates the refresh
// token too — persist the returned one).
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
