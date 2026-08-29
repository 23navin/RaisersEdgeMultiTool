// creds.rs (server)
//
// The web shell's RE NXT credential store: one shared service account for the
// whole deployment. client_id / client_secret / subscription_key come from
// env; the rotating refresh token lives on the data volume (it changes on
// every refresh, so it can't be a static env var). The current access token
// is cached in memory behind a lock so concurrent requests don't stampede the
// token endpoint.
//
// Bootstrap: SKY has no client-credentials grant, so the first refresh token
// has to come from one interactive OAuth run — do it in the desktop app and
// copy the token into DATA_DIR/re_nxt_token.json ({"refresh_token": "..."}),
// or set RE_REFRESH_TOKEN once and the store seeds the file from it.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use multitool_core::creds;
use multitool_core::errors::AppError;
use multitool_core::re_calls::Transport;

const EXPIRY_SKEW_SECS: i64 = 60;

#[derive(Serialize, Deserialize)]
struct StoredToken {
    refresh_token: String,
}

struct Cached {
    access_token: String,
    expires_at: i64, // unix seconds
}

pub struct ServerCreds {
    client_id: String,
    client_secret: String,
    subscription_key: String,
    token_path: PathBuf,
    cached: Mutex<Option<Cached>>,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl ServerCreds {
    // None when the env doesn't configure a live connection — the server then
    // runs in mock mode (fixtures), same as an unconnected desktop.
    pub fn from_env(data_dir: &std::path::Path) -> Option<Self> {
        let client_id = std::env::var("RE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("RE_CLIENT_SECRET").ok()?;
        let subscription_key = std::env::var("RE_SUBSCRIPTION_KEY").ok()?;
        let token_path = data_dir.join("re_nxt_token.json");

        // Seed the token file from RE_REFRESH_TOKEN on first boot.
        if !token_path.exists() {
            if let Ok(seed) = std::env::var("RE_REFRESH_TOKEN") {
                let _ = fs::write(
                    &token_path,
                    serde_json::to_vec_pretty(&StoredToken { refresh_token: seed })
                        .unwrap_or_default(),
                );
            }
        }

        Some(Self {
            client_id,
            client_secret,
            subscription_key,
            token_path,
            cached: Mutex::new(None),
        })
    }

    // A usable transport: cached access token when fresh, otherwise refresh
    // (rotating the stored refresh token). Blocking — call via spawn_blocking.
    pub fn transport(&self) -> Result<Transport, AppError> {
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| AppError::AuthError("Credential lock poisoned".into()))?;

        if let Some(c) = cached.as_ref() {
            if c.expires_at - EXPIRY_SKEW_SECS > now_secs() {
                return Ok(Transport::Live {
                    access_token: c.access_token.clone(),
                    subscription_key: self.subscription_key.clone(),
                });
            }
        }

        // Refresh under the lock — one request refreshes, the rest wait for
        // the fresh token instead of racing the endpoint.
        let stored: StoredToken = serde_json::from_slice(
            &fs::read(&self.token_path).map_err(|e| {
                AppError::AuthError(format!(
                    "No refresh token at {} — bootstrap the connection first: {}",
                    self.token_path.display(),
                    e
                ))
            })?,
        )
        .map_err(|e| AppError::ParseError(format!("Bad token file: {}", e)))?;

        let token = creds::refresh(&self.client_id, &self.client_secret, &stored.refresh_token)?;

        // Persist the rotated refresh token before using the access token —
        // losing it means re-bootstrapping.
        fs::write(
            &self.token_path,
            serde_json::to_vec_pretty(&StoredToken {
                refresh_token: token.refresh_token,
            })
            .unwrap_or_default(),
        )
        .map_err(|e| AppError::IoError(format!("Cannot persist refresh token: {}", e)))?;

        *cached = Some(Cached {
            access_token: token.access_token.clone(),
            expires_at: now_secs() + token.expires_in,
        });

        Ok(Transport::Live {
            access_token: token.access_token,
            subscription_key: self.subscription_key.clone(),
        })
    }
}
