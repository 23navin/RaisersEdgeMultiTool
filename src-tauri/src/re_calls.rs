// re_calls.rs
//
// The Rust half of the hybrid RE NXT (Blackbaud SKY) API-call library. Mirrors
// the TS catalog in src/lib/re-calls.ts and is the single place where an RE call
// is actually executed — shared by the report pipeline (report.rs) and, later,
// the Data Requests tab.
//
// MOCK MODE: `execute_call` currently reads a per-report fixture file instead of
// doing real HTTP, so the whole pipeline runs with no live RE connection. The
// one function below is the seam — see the marked block for where real SKY HTTP
// (auth via sky_auth::re_nxt_access_token, then execute → poll job → page) drops
// in, keeping the rest of the pipeline unchanged.

use std::fs;
use std::path::Path;

use crate::errors::AppError;

// A call's transport kind. query_execute hides the SKY async job flow; the rest
// are thin wrappers over arbitrary endpoints. Kept for parity with the TS
// catalog and to validate profile `ref`s — the real executor will branch on it.
#[allow(dead_code)] // RestGet unused until more calls land / real HTTP branches on kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReCallKind {
    QueryExecute,
    RestGet,
    RestPost,
}

pub struct ReCallDefinition {
    pub id: &'static str,
    #[allow(dead_code)] // read by the real executor (mock ignores transport)
    pub kind: ReCallKind,
}

// Central registry — mirrors RE_CALLS in src/lib/re-calls.ts. Bundle-local
// `template` definitions (the per-bundle half of the hybrid model) bypass this
// lookup; see execute_call.
const REGISTRY: &[ReCallDefinition] = &[
    ReCallDefinition { id: "re.query.execute", kind: ReCallKind::QueryExecute },
    ReCallDefinition { id: "re.query.create", kind: ReCallKind::RestPost },
];

pub fn lookup(call_ref: &str) -> Option<&'static ReCallDefinition> {
    REGISTRY.iter().find(|c| c.id == call_ref)
}

// Execute an RE call and return its JSON result.
//
// `call_ref` names a central-registry entry; when it is None the profile is
// expected to have supplied an inline `template` (`has_template`). `resolved_bind`
// is the request after `{{param:...}}` substitution — exercised here (and echoed
// by the caller for debugging) so the param→request merge is validated, even
// though mock mode ignores it for data.
//
// Returns the parsed JSON (expected: an array of row objects) read from
// `fixture_path`.
pub fn execute_call(
    call_ref: Option<&str>,
    has_template: bool,
    _resolved_bind: &serde_json::Value,
    fixture_path: &Path,
) -> Result<serde_json::Value, AppError> {
    // Validate the call resolves to something runnable.
    match call_ref {
        Some(r) => {
            if lookup(r).is_none() {
                return Err(AppError::ParseError(format!(
                    "Unknown RE call '{}'. Add it to the registry or supply an inline template.",
                    r
                )));
            }
        }
        None => {
            if !has_template {
                return Err(AppError::ParseError(
                    "Query has neither a `ref` nor an inline `template`.".to_string(),
                ));
            }
        }
    }

    // ── MOCK SEAM ────────────────────────────────────────────────────────────
    // Real implementation replaces this block with a SKY API request:
    //   1. token = sky_auth::re_nxt_access_token(app)
    //   2. for query_execute: POST execute → poll job → page results
    //      (Authorization: Bearer <token>, Bb-Api-Subscription-Key header)
    //   3. parse the response rows into the same serde_json::Value array.
    // Everything downstream (write JSON → temp, DuckDB read_json_auto) is
    // identical, so only this block changes.
    let bytes = fs::read(fixture_path).map_err(|e| {
        AppError::IoError(format!(
            "Mock fixture not found at {}: {}",
            fixture_path.display(),
            e
        ))
    })?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        AppError::ParseError(format!(
            "Mock fixture {} is not valid JSON: {}",
            fixture_path.display(),
            e
        ))
    })?;
    Ok(json)
    // ── END MOCK SEAM ────────────────────────────────────────────────────────
}
