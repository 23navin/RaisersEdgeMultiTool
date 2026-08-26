// re_calls.rs
//
// The Rust half of the hybrid RE NXT (Blackbaud SKY) API-call library. Mirrors
// the TS catalog in src/lib/re-calls.ts and is the single place where an RE call
// is actually executed — shared by the report pipeline (report.rs) and, later,
// the Data Requests tab.
//
// Two transports:
//   - Transport::Mock  — reads a per-report fixture file (offline / dev / tests).
//   - Transport::Live  — calls the real SKY Query API using a bearer token +
//                        subscription key obtained from sky_auth.
//
// The command layer picks the transport (live when connected to RE unless
// RE_NXT_MOCK is set); report.rs just calls execute_query / create_query.
//
// SKY Query API flow implemented here (see REPORT_PROFILES.md → Execution):
//   1. POST /query/queries/execute?product=RE&module=None   → { id: <jobId> }
//   2. poll GET /query/jobs/{jobId} until status == Completed → { sas_uri, ... }
//   3. GET sas_uri (Azure blob, NO auth headers, ~15 min TTL) → result rows JSON
// Paths/params are constants below so a slightly-off value is a one-line fix.

use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use crate::errors::AppError;

// ── SKY API endpoint config (adjust here if the env differs) ───────────────────
const API_BASE: &str = "https://api.sky.blackbaud.com";
const EXECUTE_PATH: &str = "/query/queries/execute";
const EXECUTE_QUERY_PARAMS: &str = "product=RE&module=None";
const JOB_PATH: &str = "/query/jobs"; // GET {JOB_PATH}/{jobId}
const CREATE_PATH: &str = "/query/queries"; // POST to create/save a query
const SUBSCRIPTION_HEADER: &str = "Bb-Api-Subscription-Key";

// Interactive polling: SKY guidance is no faster than every 5s, but a few
// seconds keeps an operational report responsive. ~3 min ceiling.
const POLL_INTERVAL: Duration = Duration::from_secs(3);
const POLL_MAX_ATTEMPTS: u32 = 60;

// ── Transport ──────────────────────────────────────────────────────────────────

pub enum Transport {
    Mock,
    Live {
        access_token: String,
        subscription_key: String,
    },
}

impl Transport {
    // Tag surfaced to the frontend so the user can see which source a refresh hit.
    pub fn label(&self) -> &'static str {
        match self {
            Transport::Mock => "mock",
            Transport::Live { .. } => "live",
        }
    }
}

// ── Call registry (mirrors src/lib/re-calls.ts) ────────────────────────────────

#[allow(dead_code)] // RestGet unused until more calls land / real HTTP branches on kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReCallKind {
    QueryExecute,
    RestGet,
    RestPost,
}

pub struct ReCallDefinition {
    pub id: &'static str,
    #[allow(dead_code)] // selected on by the live executor; mock ignores transport
    pub kind: ReCallKind,
}

const REGISTRY: &[ReCallDefinition] = &[
    ReCallDefinition { id: "re.query.execute", kind: ReCallKind::QueryExecute },
    ReCallDefinition { id: "re.query.create", kind: ReCallKind::RestPost },
];

pub fn lookup(call_ref: &str) -> Option<&'static ReCallDefinition> {
    REGISTRY.iter().find(|c| c.id == call_ref)
}

fn validate_ref(call_ref: Option<&str>, has_template: bool) -> Result<(), AppError> {
    match call_ref {
        Some(r) => {
            if lookup(r).is_none() {
                return Err(AppError::ParseError(format!(
                    "Unknown RE call '{}'. Add it to the registry or supply an inline template.",
                    r
                )));
            }
            Ok(())
        }
        None => {
            if has_template {
                Ok(())
            } else {
                Err(AppError::ParseError(
                    "Query has neither a `ref` nor an inline `template`.".to_string(),
                ))
            }
        }
    }
}

// ── execute_query ──────────────────────────────────────────────────────────────
// Runs a query and returns its rows as a JSON array of row objects. `request` is
// the resolved ad-hoc query definition (or saved-query reference) to send.

pub fn execute_query(
    transport: &Transport,
    call_ref: Option<&str>,
    has_template: bool,
    request: &Value,
    fixture_path: &Path,
) -> Result<Value, AppError> {
    validate_ref(call_ref, has_template)?;
    match transport {
        Transport::Mock => read_fixture(fixture_path),
        Transport::Live { access_token, subscription_key } => {
            live_execute_query(access_token, subscription_key, request)
        }
    }
}

// ── create_query ───────────────────────────────────────────────────────────────
// Creates/saves a query in RE from `request` (e.g. { name, ids }). Returns the
// created query JSON (live) or a synthetic stub (mock).

pub fn create_query(
    transport: &Transport,
    call_ref: Option<&str>,
    has_template: bool,
    request: &Value,
) -> Result<Value, AppError> {
    validate_ref(call_ref, has_template)?;
    match transport {
        Transport::Mock => Ok(json!({ "id": "mock-query-id", "mock": true })),
        Transport::Live { access_token, subscription_key } => {
            let url = format!("{}{}", API_BASE, CREATE_PATH);
            // NOTE: the exact create/static-query body is the least-documented
            // part of the Query API — adjust this request shape (and CREATE_PATH)
            // to your environment. `request` carries { name, ids } from report.rs.
            post_json(access_token, subscription_key, &url, request)
        }
    }
}

// ── Mock ───────────────────────────────────────────────────────────────────────

fn read_fixture(fixture_path: &Path) -> Result<Value, AppError> {
    let bytes = std::fs::read(fixture_path).map_err(|e| {
        AppError::IoError(format!(
            "Mock fixture not found at {}: {}",
            fixture_path.display(),
            e
        ))
    })?;
    let json: Value = serde_json::from_slice(&bytes).map_err(|e| {
        AppError::ParseError(format!(
            "Mock fixture {} is not valid JSON: {}",
            fixture_path.display(),
            e
        ))
    })?;
    Ok(normalize_rows(json))
}

// ── Live SKY Query flow ─────────────────────────────────────────────────────────

fn live_execute_query(
    token: &str,
    key: &str,
    request: &Value,
) -> Result<Value, AppError> {
    let client = reqwest::blocking::Client::new();

    // 1. Start the execution job.
    let execute_url = format!("{}{}?{}", API_BASE, EXECUTE_PATH, EXECUTE_QUERY_PARAMS);
    let start = post_json(token, key, &execute_url, request)?;
    let job_id = json_id(&start).ok_or_else(|| {
        AppError::ParseError(format!(
            "Execute response carried no job id: {}",
            truncate(&start.to_string(), 300)
        ))
    })?;

    // 2. Poll the job until it completes.
    let job_url = format!("{}{}/{}", API_BASE, JOB_PATH, job_id);
    let mut sas_uri: Option<String> = None;
    for _ in 0..POLL_MAX_ATTEMPTS {
        std::thread::sleep(POLL_INTERVAL);
        let job = get_json(token, key, &job_url)?;
        let status = job
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        if status.contains("complet") {
            sas_uri = job
                .get("sas_uri")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            break;
        }
        if status.contains("fail") || status.contains("cancel") || status.contains("error") {
            return Err(AppError::NetworkError(format!(
                "Query job {} ended with status '{}': {}",
                job_id,
                status,
                truncate(&job.to_string(), 300)
            )));
        }
    }
    let sas_uri = sas_uri.ok_or_else(|| {
        AppError::NetworkError(format!(
            "Query job {} did not complete within {}s",
            job_id,
            POLL_INTERVAL.as_secs() * POLL_MAX_ATTEMPTS as u64
        ))
    })?;

    // 3. Download the results from the SAS URI — no auth headers, short-lived.
    let resp = client
        .get(&sas_uri)
        .send()
        .map_err(|e| AppError::NetworkError(format!("Result download failed: {}", e)))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(AppError::NetworkError(format!(
            "Result download returned {}: {}",
            status,
            truncate(&body, 300)
        )));
    }
    let result: Value = resp
        .json()
        .map_err(|e| AppError::ParseError(format!("Result JSON parse failed: {}", e)))?;
    Ok(normalize_rows(result))
}

// ── HTTP helpers ────────────────────────────────────────────────────────────────

fn post_json(token: &str, key: &str, url: &str, body: &Value) -> Result<Value, AppError> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(url)
        .bearer_auth(token)
        .header(SUBSCRIPTION_HEADER, key)
        .json(body)
        .send()
        .map_err(|e| AppError::NetworkError(e.to_string()))?;
    json_or_err(resp)
}

fn get_json(token: &str, key: &str, url: &str) -> Result<Value, AppError> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(url)
        .bearer_auth(token)
        .header(SUBSCRIPTION_HEADER, key)
        .send()
        .map_err(|e| AppError::NetworkError(e.to_string()))?;
    json_or_err(resp)
}

fn json_or_err(resp: reqwest::blocking::Response) -> Result<Value, AppError> {
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        // 401/403 most often mean an expired token or wrong subscription key.
        return Err(AppError::NetworkError(format!(
            "SKY API returned {}: {}",
            status,
            truncate(&text, 400)
        )));
    }
    serde_json::from_str(&text)
        .map_err(|e| AppError::ParseError(format!("SKY API response was not JSON: {}", e)))
}

// ── result normalization ─────────────────────────────────────────────────────────
// Coerce whatever the API / fixture returns into a JSON array of row objects, so
// DuckDB's read_json_auto sees a consistent shape. Handles:
//   - already an array                              → as-is
//   - { rows: [...] } / results / value / data / records
//   - { fields: [...], rows: [[...]] }              → zipped into objects
//   - a single object                              → wrapped in a 1-element array

fn normalize_rows(v: Value) -> Value {
    if v.is_array() {
        return v;
    }
    if let Value::Object(ref map) = v {
        for key in ["rows", "results", "value", "data", "records"] {
            if let Some(Value::Array(arr)) = map.get(key) {
                // rows-of-arrays + a sibling `fields` list → build objects.
                if arr.first().map(|r| r.is_array()).unwrap_or(false) {
                    if let Some(fields) = map.get("fields").and_then(field_names) {
                        return zip_rows(&fields, arr);
                    }
                }
                return Value::Array(arr.clone());
            }
        }
        return Value::Array(vec![v]);
    }
    json!([])
}

// Pull column names from a `fields` value that may be ["a","b"] or
// [{ "name": "a" }, { "id": "b" }].
fn field_names(fields: &Value) -> Option<Vec<String>> {
    let arr = fields.as_array()?;
    let names = arr
        .iter()
        .map(|f| match f {
            Value::String(s) => s.clone(),
            Value::Object(m) => m
                .get("name")
                .or_else(|| m.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        })
        .collect();
    Some(names)
}

fn zip_rows(fields: &[String], rows: &[Value]) -> Value {
    let objs = rows.iter().map(|row| {
        let cells = row.as_array().cloned().unwrap_or_default();
        let mut obj = serde_json::Map::new();
        for (i, name) in fields.iter().enumerate() {
            obj.insert(name.clone(), cells.get(i).cloned().unwrap_or(Value::Null));
        }
        Value::Object(obj)
    });
    Value::Array(objs.collect())
}

// Job/execute responses put the id under "id" (sometimes "job_id"); accept a
// string or a number.
fn json_id(v: &Value) -> Option<String> {
    for key in ["id", "job_id"] {
        match v.get(key) {
            Some(Value::String(s)) => return Some(s.clone()),
            Some(Value::Number(n)) => return Some(n.to_string()),
            _ => {}
        }
    }
    None
}

// Char-safe truncation — error bodies may contain multibyte UTF-8, so never
// slice on a byte boundary.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut)
    }
}
