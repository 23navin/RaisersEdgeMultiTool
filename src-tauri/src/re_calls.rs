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

// ── SKY Query API config ───────────────────────────────────────────────────────
// Shapes below follow "API reference/query.yaml" (the Query API OpenAPI spec).
// API_BASE is that spec's `servers.url`, so paths here match it one-for-one.
const API_BASE: &str = "https://api.sky.blackbaud.com/query";
const EXECUTE_PATH: &str = "/queries/execute"; // POST ad-hoc ExecuteQueryRequest
const JOB_PATH: &str = "/jobs"; // GET {JOB_PATH}/{jobId}
const CREATE_PATH: &str = "/queries"; // POST QueryAdd
const SUBSCRIPTION_HEADER: &str = "Bb-Api-Subscription-Key";

// `product` and `module` are REQUIRED on every Query API call — execute, job
// status, and create alike. Omitting them is a 400 ("Product was not supplied").
const PRODUCT_MODULE: &str = "product=RE&module=None";

// The job's `sas_uri` is only returned when include_read_url asks for it; the
// default is Never, which would leave us polling until timeout.
const INCLUDE_READ_URL: &str = "include_read_url=OnceCompleted";

// ExecuteQueryRequest defaults, applied when the profile template doesn't set them:
//   Json  — results download as a JSON array, which is what normalize_rows and
//           DuckDB's read_json_auto want. (The API default is Csv.)
//   None  — raw SQL values, no UI formatting ("$5.00", localized dates).
//   Asynchronous — a throttled job still queues. Synchronous 429s instead, and
//           requires polling at least every 10s or the job is cancelled.
const DEFAULT_OUTPUT_FORMAT: &str = "Json";
const DEFAULT_FORMATTING_MODE: &str = "None";
const DEFAULT_UX_MODE: &str = "Asynchronous";

// ── Code Table API config ──────────────────────────────────────────────────────
// A separate SKY service from Query — different base, and none of the
// product/module params. Shapes follow "API reference/codetable.yaml".
// NOTE: every table-entry endpoint is marked PREVIEW in that spec and may change.
const CODETABLE_BASE: &str = "https://api.sky.blackbaud.com/codetable";
const CODETABLES_PATH: &str = "/v1/codetables";
// The list endpoints cap `limit` at 10000; we page until a short page comes back.
const CODETABLE_PAGE_SIZE: u32 = 5000;
const CODETABLE_MAX_PAGES: u32 = 20; // 100k entries — a runaway guard, not a real limit

// Interactive polling: a few seconds keeps an operational report responsive.
// ~3 min ceiling.
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

#[allow(dead_code)] // some kinds are only used to tag registry entries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReCallKind {
    QueryExecute,
    RestGet,
    RestPost,
    RestPatch,
    RestDelete,
}

pub struct ReCallDefinition {
    pub id: &'static str,
    #[allow(dead_code)] // selected on by the live executor; mock ignores transport
    pub kind: ReCallKind,
}

const REGISTRY: &[ReCallDefinition] = &[
    ReCallDefinition { id: "re.query.execute", kind: ReCallKind::QueryExecute },
    ReCallDefinition { id: "re.query.create", kind: ReCallKind::RestPost },
    ReCallDefinition { id: "re.codetable.list", kind: ReCallKind::RestGet },
    ReCallDefinition { id: "re.codetable.entries", kind: ReCallKind::RestGet },
    ReCallDefinition { id: "re.codetable.entry.create", kind: ReCallKind::RestPost },
    ReCallDefinition { id: "re.codetable.entry.update", kind: ReCallKind::RestPatch },
    ReCallDefinition { id: "re.codetable.entry.delete", kind: ReCallKind::RestDelete },
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
// Creates/saves a query in RE. `request` is the QueryAdd body built by
// report.rs (name + format + type_id + filter_fields). Returns the created
// query JSON (live) or a synthetic stub (mock).

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
            let url = format!("{}{}?{}", API_BASE, CREATE_PATH, PRODUCT_MODULE);
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

    // 1. Start the execution job. The API wants an ExecuteQueryRequest — the
    //    query definition nested under `query`, not at the top level.
    let execute_url = format!("{}{}?{}", API_BASE, EXECUTE_PATH, PRODUCT_MODULE);
    let start = post_json(token, key, &execute_url, &build_execute_body(request))?;
    let job_id = json_id(&start).ok_or_else(|| {
        AppError::ParseError(format!(
            "Execute response carried no job id: {}",
            truncate(&start.to_string(), 300)
        ))
    })?;

    // 2. Poll the job until it completes. product/module are required here too,
    //    and include_read_url is what makes `sas_uri` appear on the response.
    let job_url = format!(
        "{}{}/{}?{}&{}",
        API_BASE, JOB_PATH, job_id, PRODUCT_MODULE, INCLUDE_READ_URL
    );
    let mut sas_uri: Option<String> = None;
    let mut last_status = String::new();
    for attempt in 0..POLL_MAX_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(POLL_INTERVAL);
        }
        let job = get_json(token, key, &job_url)?;
        // QueryJobStatus: Pending | Running | Completed | Failed | Cancelling |
        // Cancelled | Throttled. Pending/Running/Throttled all keep polling —
        // a throttled job is queued, not dead. Cancelling may still complete.
        last_status = job
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        match last_status.to_lowercase().as_str() {
            "completed" => {
                sas_uri = job
                    .get("sas_uri")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if sas_uri.is_none() {
                    return Err(AppError::NetworkError(format!(
                        "Query job {} completed but returned no sas_uri (is {} still on the \
                         job request?): {}",
                        job_id,
                        INCLUDE_READ_URL,
                        truncate(&job.to_string(), 300)
                    )));
                }
                break;
            }
            "failed" | "cancelled" => {
                return Err(AppError::NetworkError(format!(
                    "Query job {} ended with status '{}': {}",
                    job_id,
                    last_status,
                    truncate(&job.to_string(), 300)
                )));
            }
            _ => {}
        }
    }
    let sas_uri = sas_uri.ok_or_else(|| {
        AppError::NetworkError(format!(
            "Query job {} did not complete within {}s (last status: '{}')",
            job_id,
            POLL_INTERVAL.as_secs() * POLL_MAX_ATTEMPTS as u64,
            last_status
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
    // The blob is a JSON array because we asked for output_format Json; a Csv or
    // Xlsx job would land here as non-JSON, so say so rather than "invalid JSON".
    let body = resp
        .text()
        .map_err(|e| AppError::NetworkError(format!("Result download failed: {}", e)))?;
    let result: Value = serde_json::from_str(&body).map_err(|e| {
        AppError::ParseError(format!(
            "Result body was not JSON ({}). Expected output_format \"{}\"; \
             body started: {}",
            e,
            DEFAULT_OUTPUT_FORMAT,
            truncate(&body, 200)
        ))
    })?;
    Ok(normalize_rows(result))
}

// Wrap a profile's query definition in the ExecuteQueryRequest envelope the API
// requires. Posting the bare definition is a 400: "The Query field is required."
//
// A profile template may supply either shape:
//   - just the ExecuteQueryDefinition (select_fields/filter_fields/type_id/…),
//     which gets nested under `query` here — the common case; or
//   - a full envelope already containing `query`, to control ux_mode,
//     output_format, results_file_name, time_zone_offset_in_minutes, etc.
// Defaults fill only what the template left unset.
fn build_execute_body(request: &Value) -> Value {
    let mut body = if request.get("query").is_some() {
        request.clone()
    } else {
        json!({ "query": request })
    };
    if let Some(map) = body.as_object_mut() {
        map.entry("ux_mode")
            .or_insert_with(|| json!(DEFAULT_UX_MODE));
        map.entry("output_format")
            .or_insert_with(|| json!(DEFAULT_OUTPUT_FORMAT));
        map.entry("formatting_mode")
            .or_insert_with(|| json!(DEFAULT_FORMATTING_MODE));
    }
    body
}

// ── HTTP helpers ────────────────────────────────────────────────────────────────

fn post_json(token: &str, key: &str, url: &str, body: &Value) -> Result<Value, AppError> {
    send_json(reqwest::Method::POST, token, key, url, Some(body))
}

fn get_json(token: &str, key: &str, url: &str) -> Result<Value, AppError> {
    send_json(reqwest::Method::GET, token, key, url, None)
}

// One request with the two headers every SKY call needs. `body` is attached
// when present (POST/PATCH); GET and DELETE pass None.
fn send_json(
    method: reqwest::Method,
    token: &str,
    key: &str,
    url: &str,
    body: Option<&Value>,
) -> Result<Value, AppError> {
    let client = reqwest::blocking::Client::new();
    let mut req = client
        .request(method, url)
        .bearer_auth(token)
        .header(SUBSCRIPTION_HEADER, key);
    if let Some(b) = body {
        req = req.json(b);
    }
    let resp = req
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
    // A 200 with no body (DELETE, and some PATCHes) is a success, not a parse error.
    if text.trim().is_empty() {
        return Ok(json!({ "ok": true }));
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

// ══ Code Tables ════════════════════════════════════════════════════════════════
// Read: fetch_code_table_entries pulls every entry of one table so a profile can
// join against it in SQL. Write: create/update/delete one entry.
//
// Mock mode reads fixtures/codetables/<name>.json (reads) and no-ops writes, so
// a profile that touches code tables still runs offline.

// What a code table is addressed by. A profile may name the table (resolved to
// its id with one extra GET) or give the id directly when it already knows it.
pub enum CodeTableSelector<'a> {
    Id(&'a str),
    Name(&'a str),
}

impl<'a> CodeTableSelector<'a> {
    pub fn describe(&self) -> String {
        match self {
            CodeTableSelector::Id(id) => format!("id {}", id),
            CodeTableSelector::Name(n) => format!("'{}'", n),
        }
    }
}

// Resolve a selector to a concrete code_tables_id (live only).
fn resolve_code_table_id(
    token: &str,
    key: &str,
    selector: &CodeTableSelector<'_>,
) -> Result<String, AppError> {
    match selector {
        CodeTableSelector::Id(id) => Ok((*id).to_string()),
        CodeTableSelector::Name(name) => {
            // `name` is an exact-match filter on the code table list.
            let url = format!(
                "{}{}?name={}",
                CODETABLE_BASE,
                CODETABLES_PATH,
                urlencode(name)
            );
            let body = get_json(token, key, &url)?;
            let first = body
                .get("value")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first());
            match first.and_then(|t| t.get("code_tables_id")).and_then(id_as_string) {
                Some(id) => Ok(id),
                None => Err(AppError::NetworkError(format!(
                    "No code table named '{}' was found in RE.",
                    name
                ))),
            }
        }
    }
}

// Every entry in one code table, as a JSON array of entry objects.
//
// `fixture_path` is the mock-mode source; `include_inactive` maps to the
// endpoint's own filter so a profile can choose whether retired entries appear.
pub fn fetch_code_table_entries(
    transport: &Transport,
    selector: &CodeTableSelector<'_>,
    include_inactive: bool,
    fixture_path: &Path,
) -> Result<Value, AppError> {
    match transport {
        Transport::Mock => read_fixture(fixture_path),
        Transport::Live { access_token, subscription_key } => {
            let table_id = resolve_code_table_id(access_token, subscription_key, selector)?;
            let mut all: Vec<Value> = Vec::new();
            for page in 0..CODETABLE_MAX_PAGES {
                let url = format!(
                    "{}{}/{}/tableentries?limit={}&offset={}&include_inactive={}",
                    CODETABLE_BASE,
                    CODETABLES_PATH,
                    urlencode(&table_id),
                    CODETABLE_PAGE_SIZE,
                    page * CODETABLE_PAGE_SIZE,
                    include_inactive
                );
                let body = get_json(access_token, subscription_key, &url)?;
                // TableEntryCollection is { count, value: [...] }.
                let batch = match body.get("value").and_then(|v| v.as_array()) {
                    Some(a) => a.clone(),
                    None => break,
                };
                let n = batch.len() as u32;
                all.extend(batch);
                if n < CODETABLE_PAGE_SIZE {
                    break;
                }
            }
            Ok(Value::Array(all))
        }
    }
}

// One write against a code table's entries. `entry` is a TableEntryCreate /
// TableEntryEdit body; `entry_id` names the target for update/delete.
pub enum CodeTableWrite {
    Create { entry: Value },
    Update { entry_id: String, entry: Value },
    Delete { entry_id: String },
}

impl CodeTableWrite {
    pub fn verb(&self) -> &'static str {
        match self {
            CodeTableWrite::Create { .. } => "create",
            CodeTableWrite::Update { .. } => "update",
            CodeTableWrite::Delete { .. } => "delete",
        }
    }
}

// Apply one write. Returns the API's response (create yields { id }); mock mode
// returns a stub without touching the network.
pub fn write_code_table_entry(
    transport: &Transport,
    selector: &CodeTableSelector<'_>,
    write: &CodeTableWrite,
) -> Result<Value, AppError> {
    match transport {
        Transport::Mock => Ok(json!({ "mock": true, "operation": write.verb() })),
        Transport::Live { access_token, subscription_key } => {
            let table_id = resolve_code_table_id(access_token, subscription_key, selector)?;
            let entries_url = format!(
                "{}{}/{}/tableentries",
                CODETABLE_BASE,
                CODETABLES_PATH,
                urlencode(&table_id)
            );
            match write {
                CodeTableWrite::Create { entry } => {
                    post_json(access_token, subscription_key, &entries_url, entry)
                }
                CodeTableWrite::Update { entry_id, entry } => {
                    let url = format!("{}/{}", entries_url, urlencode(entry_id));
                    send_json(
                        reqwest::Method::PATCH,
                        access_token,
                        subscription_key,
                        &url,
                        Some(entry),
                    )
                }
                CodeTableWrite::Delete { entry_id } => {
                    let url = format!("{}/{}", entries_url, urlencode(entry_id));
                    send_json(
                        reqwest::Method::DELETE,
                        access_token,
                        subscription_key,
                        &url,
                        None,
                    )
                }
            }
        }
    }
}

// code_tables_id comes back as a string in the spec, but accept a number too
// rather than silently failing to find the table.
fn id_as_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

// Minimal percent-encoding for path/query segments (table names contain spaces,
// and ids are opaque). Avoids pulling in another crate for two call sites.
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
