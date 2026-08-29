// creds.rs (server)
//
// The web shell's RE NXT connection. Two ways one gets established, checked
// in this order:
//
//   1. Interactive — someone fills in the Settings → General form and signs
//      in through Blackbaud. The result is persisted to
//      DATA_DIR/re_nxt_connection.json, exactly the format the desktop app
//      writes, so a connection is portable between the two.
//   2. Environment — RE_CLIENT_ID / RE_CLIENT_SECRET / RE_SUBSCRIPTION_KEY
//      (+ a seeded refresh token) for headless deploys that shouldn't depend
//      on a human visiting the UI.
//
// Neither present (or RE_NXT_MOCK set) → mock mode against bundle fixtures.
//
// The connection is server-wide: one person connects, everyone using the
// server shares it, and every RE write is attributed to that Blackbaud user.
// When per-user identity arrives, this store becomes keyed by user and the
// rest of the shell is unchanged.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use multitool_core::creds::{self, Connection, ConnectionStatus};
use multitool_core::errors::AppError;
use multitool_core::re_calls::Transport;

// An in-flight authorization: the credentials the user typed, held between
// the /connect_re_nxt call and Blackbaud's redirect back to /oauth/callback.
// Keyed by the CSRF state nonce.
pub struct Pending {
    pub client_id: String,
    pub client_secret: String,
    pub subscription_key: String,
    pub redirect_uri: String,
    pub created_at: i64,
}

// Abandoned authorizations (user closed the Blackbaud tab) are swept after this.
const PENDING_TTL_SECS: i64 = 15 * 60;

pub struct CredStore {
    connection_path: PathBuf,
    // Serializes refreshes so concurrent requests don't race the token
    // endpoint and spend each other's refresh token.
    refresh_lock: Mutex<()>,
    pending: Mutex<Vec<(String, Pending)>>,
    // Fallback service account from env, used when no connection file exists.
    env_fallback: Option<EnvCreds>,
    force_mock: bool,
}

// Only what the client-credential rotation check needs; the subscription key
// and refresh token reach the connection file via the seed above.
struct EnvCreds {
    client_id: String,
    client_secret: String,
}

impl CredStore {
    pub fn new(data_dir: &Path, force_mock: bool) -> Self {
        let connection_path = data_dir.join("re_nxt_connection.json");

        // Seed a connection file from env on first boot, so a headless deploy
        // never needs the UI. RE_REFRESH_TOKEN is spent on the first refresh
        // and replaced by the rotated one in the file from then on.
        let env_fallback = match (
            std::env::var("RE_CLIENT_ID"),
            std::env::var("RE_CLIENT_SECRET"),
            std::env::var("RE_SUBSCRIPTION_KEY"),
        ) {
            (Ok(client_id), Ok(client_secret), Ok(subscription_key)) => {
                if !connection_path.exists() {
                    if let Ok(seed) = std::env::var("RE_REFRESH_TOKEN") {
                        let conn = Connection {
                            client_id: client_id.clone(),
                            client_secret: client_secret.clone(),
                            subscription_key: subscription_key.clone(),
                            access_token: String::new(),
                            refresh_token: seed,
                            expires_at: 0, // forces a refresh on first use
                            environment_id: None,
                            environment_name: None,
                        };
                        let _ = creds::save_connection(&connection_path, &conn);
                    }
                }
                Some(EnvCreds { client_id, client_secret })
            }
            _ => None,
        };

        Self {
            connection_path,
            refresh_lock: Mutex::new(()),
            pending: Mutex::new(Vec::new()),
            env_fallback,
            force_mock,
        }
    }

    // ── interactive connect ───────────────────────────────────────────────────

    // Stash the typed credentials and hand back the Blackbaud URL the browser
    // should visit. Nothing is persisted until the callback succeeds, so a
    // half-finished attempt leaves no connection behind.
    pub fn begin_connect(
        &self,
        client_id: String,
        client_secret: String,
        subscription_key: String,
        redirect_uri: String,
    ) -> Result<String, AppError> {
        let state = creds::random_state();
        let url = creds::authorize_url(&client_id, &redirect_uri, &state);

        let mut pending = self.lock_pending()?;
        let now = creds::now_secs();
        pending.retain(|(_, p)| now - p.created_at < PENDING_TTL_SECS);
        pending.push((
            state,
            Pending { client_id, client_secret, subscription_key, redirect_uri, created_at: now },
        ));
        Ok(url)
    }

    // Complete the handshake: match the state nonce, exchange the code, persist.
    pub fn finish_connect(&self, state: &str, code: &str) -> Result<ConnectionStatus, AppError> {
        let p = {
            let mut pending = self.lock_pending()?;
            let idx = pending.iter().position(|(s, _)| s == state).ok_or_else(|| {
                AppError::AuthError(
                    "This sign-in link is no longer valid — start the connection again.".into(),
                )
            })?;
            pending.remove(idx).1
        };

        let token = creds::exchange_code(&p.client_id, &p.client_secret, code, &p.redirect_uri)?;
        let conn = Connection {
            client_id: p.client_id,
            client_secret: p.client_secret,
            subscription_key: p.subscription_key,
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at: creds::now_secs() + token.expires_in,
            environment_id: token.environment_id,
            environment_name: token.environment_name,
        };
        creds::save_connection(&self.connection_path, &conn)?;
        Ok(conn.status())
    }

    pub fn disconnect(&self) -> Result<(), AppError> {
        creds::delete_connection(&self.connection_path)
    }

    // ── status + transport ────────────────────────────────────────────────────

    // Reports the stored connection AND whether mock mode is pinned on. These
    // are independent: a connection can exist while RE_NXT_MOCK forces every
    // call to fixtures, and the UI must be able to say so rather than claiming
    // the sign-in failed.
    pub fn status(&self) -> ConnectionStatus {
        let mut status = match creds::load_connection(&self.connection_path) {
            Ok(Some(conn)) => conn.status(),
            Ok(None) => ConnectionStatus::default(),
            Err(e) => {
                // Don't silently look disconnected when the file is unreadable.
                tracing::error!(
                    path = %self.connection_path.display(),
                    error = %e,
                    "cannot read RE NXT connection file"
                );
                ConnectionStatus::default()
            }
        };
        status.mock_forced = self.force_mock;
        status
    }

    // True when a usable live connection exists — cheap, no network.
    pub fn is_connected(&self) -> bool {
        !self.force_mock
            && matches!(creds::load_connection(&self.connection_path), Ok(Some(_)))
    }

    // The transport every RE-touching operation runs through. Refreshes (and
    // re-persists) the access token when it's near expiry. Blocking — call
    // via spawn_blocking.
    pub fn transport(&self) -> Result<Transport, AppError> {
        if self.force_mock {
            return Ok(Transport::Mock);
        }

        // One refresh at a time: Blackbaud spends the refresh token on use, so
        // two concurrent refreshes would invalidate each other.
        let _guard = self
            .refresh_lock
            .lock()
            .map_err(|_| AppError::AuthError("Credential lock poisoned".into()))?;

        let Some(mut conn) = creds::load_connection(&self.connection_path)? else {
            // No connection: fall back to mock rather than failing the run, so
            // an unconfigured server still demos against fixtures.
            return Ok(Transport::Mock);
        };

        // An env-configured deploy can rotate its client credentials without
        // re-authorizing; keep the stored ones in step.
        if let Some(env) = &self.env_fallback {
            if conn.client_id == env.client_id && conn.client_secret != env.client_secret {
                conn.client_secret = env.client_secret.clone();
            }
        }

        if creds::ensure_fresh(&mut conn)? {
            creds::save_connection(&self.connection_path, &conn)?;
        }

        Ok(Transport::Live {
            access_token: conn.access_token,
            subscription_key: conn.subscription_key,
        })
    }

    fn lock_pending(&self) -> Result<std::sync::MutexGuard<'_, Vec<(String, Pending)>>, AppError> {
        self.pending
            .lock()
            .map_err(|_| AppError::AuthError("Pending-auth lock poisoned".into()))
    }
}
