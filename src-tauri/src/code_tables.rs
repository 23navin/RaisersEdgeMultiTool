// ── code_tables.rs ─────────────────────────────────────────────────────────────
// The bridge between a profile's `code_tables:` section and RE's Code Table API.
//
//   read:  fetch_all()  — pull every entry of each declared table, write it to
//                         the run's temp dir as JSON, and hand back the
//                         label → path map that SQL's {{codetable:Label}}
//                         placeholders resolve against (db::substitute_code_tables).
//   write: run_sync()   — run a code_table_sync step's SQL and turn each row
//                         into one create / update / delete against RE.
//
// Both halves go through re_calls::Transport, so a profile that touches code
// tables still runs offline against fixtures.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::db;
use crate::errors::AppError;
use crate::profile::{LoadedProfile, Step};
use crate::re_calls::{self, CodeTableSelector, CodeTableWrite, Transport};

// Entry fields a profile's SQL may set. Anything else in a row is ignored, so a
// sync query can carry extra columns for its own joins/filtering without them
// leaking into the request body.
const WRITABLE_FIELDS: &[&str] = &[
    "long_description",
    "short_description",
    "numeric_value",
    "sequence",
    "is_active",
    "phone_format",
    "phone_type",
];

// The column naming the entry to update/delete. Matches the API's own field name.
const ID_COLUMN: &str = "table_entries_id";

// ── read ───────────────────────────────────────────────────────────────────────

// Fetch every declared code table into `run_dir`, returning output label → path.
// Returns an empty map when the profile declares none, so callers can always
// call this unconditionally.
pub fn fetch_all(
    loaded: &LoadedProfile,
    transport: &Transport,
    run_dir: &Path,
) -> Result<HashMap<String, String>, AppError> {
    let refs = &loaded.structure.code_tables;
    if refs.is_empty() {
        return Ok(HashMap::new());
    }
    fs::create_dir_all(run_dir)
        .map_err(|e| AppError::IoError(format!("Cannot create code table dir: {}", e)))?;

    let mut paths = HashMap::new();
    for ct in refs {
        let selector = ct.selector()?;
        // Mock source, mirroring the report pipeline's fixtures/ convention.
        let fixture = loaded
            .temp_dir
            .join("fixtures")
            .join("codetables")
            .join(format!("{}.json", ct.output));

        let entries = re_calls::fetch_code_table_entries(
            transport,
            &selector,
            ct.include_inactive.unwrap_or(false),
            &fixture,
        )
        .map_err(|e| {
            AppError::NetworkError(format!(
                "Code table {} (declared as '{}'): {}",
                selector.describe(),
                ct.id,
                e
            ))
        })?;

        let path: PathBuf = run_dir.join(format!("codetable_{}.json", ct.output));
        fs::write(&path, serde_json::to_vec(&entries).unwrap_or_default()).map_err(|e| {
            AppError::IoError(format!("Cannot write code table {}: {}", ct.output, e))
        })?;
        paths.insert(ct.output.clone(), path.to_string_lossy().to_string());
    }
    Ok(paths)
}

// ── write ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub ok: bool,
    pub operation: String,
    pub code_table: String,
    pub attempted: usize,
    pub succeeded: usize,
    // One entry per failed row — the run keeps going so a single bad row can't
    // hide the rest, and the user sees exactly which ones need attention.
    pub failures: Vec<SyncFailure>,
    pub message: String,
    pub mode: String, // "live" | "mock"
}

#[derive(Debug, Serialize)]
pub struct SyncFailure {
    pub row: usize,
    pub identifier: String, // long_description or entry id, for a human-readable pointer
    pub error: String,
}

// Execute one code_table_sync step: run its SQL over the uploaded inputs, then
// apply one write per returned row.
pub fn run_sync(
    loaded: &LoadedProfile,
    step: &Step,
    file_paths: &HashMap<String, String>,
    transport: &Transport,
    run_dir: &Path,
) -> Result<SyncResult, AppError> {
    let operation = step
        .operation
        .as_deref()
        .ok_or_else(|| {
            AppError::ParseError(format!(
                "code_table_sync step '{}' has no `operation` (create | update | delete)",
                step.label
            ))
        })?
        .to_lowercase();
    if !matches!(operation.as_str(), "create" | "update" | "delete") {
        return Err(AppError::ParseError(format!(
            "code_table_sync step '{}' has operation '{}'; expected create, update, or delete",
            step.label, operation
        )));
    }

    let selector = step_selector(step)?;
    let sql_name = step.sql.as_deref().ok_or_else(|| {
        AppError::ParseError(format!(
            "code_table_sync step '{}' has no `sql` naming the rows to push",
            step.label
        ))
    })?;
    let sql = loaded.sql_files.get(sql_name).ok_or_else(|| {
        AppError::ParseError(format!("SQL file '{}' not found in profile", sql_name))
    })?;

    // Code tables are fetched first so a sync query can join against the live
    // table — the usual shape being "rows in my file that aren't in RE yet".
    let code_table_paths = fetch_all(loaded, transport, run_dir)?;
    let rows = db::select_rows(file_paths, &code_table_paths, sql)?;

    let mut succeeded = 0usize;
    let mut failures: Vec<SyncFailure> = Vec::new();

    for (i, row) in rows.rows.iter().enumerate() {
        let record = row_to_map(&rows.columns, row);
        let identifier = record
            .get("long_description")
            .or_else(|| record.get(ID_COLUMN))
            .and_then(|v| v.as_str())
            .unwrap_or("(unnamed)")
            .to_string();

        let result = build_write(&operation, &record, step).and_then(|w| {
            re_calls::write_code_table_entry(transport, &selector, &w)
        });

        match result {
            Ok(_) => succeeded += 1,
            Err(e) => failures.push(SyncFailure {
                row: i + 1,
                identifier,
                error: e.to_string(),
            }),
        }
    }

    let attempted = rows.rows.len();
    let table_label = selector.describe();
    let message = if attempted == 0 {
        format!("Nothing to {} — the query returned no rows.", operation)
    } else if failures.is_empty() {
        format!(
            "{}d {} entr{} in code table {}.",
            operation,
            succeeded,
            if succeeded == 1 { "y" } else { "ies" },
            table_label
        )
    } else {
        format!(
            "{}d {} of {} entries in code table {}; {} failed.",
            operation,
            succeeded,
            attempted,
            table_label,
            failures.len()
        )
    };

    Ok(SyncResult {
        ok: failures.is_empty(),
        operation,
        code_table: table_label,
        attempted,
        succeeded,
        failures,
        message,
        mode: transport.label().to_string(),
    })
}

// A step addresses its table the same way a code_tables entry does.
fn step_selector(step: &Step) -> Result<CodeTableSelector<'_>, AppError> {
    if let Some(id) = step.code_table_id.as_deref() {
        return Ok(CodeTableSelector::Id(id));
    }
    if let Some(name) = step.code_table.as_deref() {
        return Ok(CodeTableSelector::Name(name));
    }
    Err(AppError::ParseError(format!(
        "code_table_sync step '{}' needs either `code_table` or `code_table_id`",
        step.label
    )))
}

// Turn one SQL row into the write it describes.
fn build_write(
    operation: &str,
    record: &Map<String, Value>,
    step: &Step,
) -> Result<CodeTableWrite, AppError> {
    match operation {
        "create" => {
            if !record.contains_key("long_description") {
                return Err(AppError::ParseError(format!(
                    "code_table_sync step '{}': create needs a `long_description` column",
                    step.label
                )));
            }
            Ok(CodeTableWrite::Create {
                entry: entry_body(record),
            })
        }
        "update" => {
            let id = entry_id(record, step)?;
            Ok(CodeTableWrite::Update {
                entry_id: id.to_string(),
                entry: entry_body(record),
            })
        }
        "delete" => Ok(CodeTableWrite::Delete {
            entry_id: entry_id(record, step)?.to_string(),
        }),
        _ => unreachable!("operation validated by caller"),
    }
}

fn entry_id<'a>(record: &'a Map<String, Value>, step: &Step) -> Result<&'a str, AppError> {
    record
        .get(ID_COLUMN)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AppError::ParseError(format!(
                "code_table_sync step '{}': {} needs a non-empty `{}` column naming the entry",
                step.label,
                step.operation.as_deref().unwrap_or("this operation"),
                ID_COLUMN
            ))
        })
}

// Keep only the fields the API accepts, coercing the VARCHAR-cast values DuckDB
// hands back into the JSON types the schema expects.
fn entry_body(record: &Map<String, Value>) -> Value {
    let mut body = Map::new();
    for field in WRITABLE_FIELDS {
        let Some(raw) = record.get(*field).and_then(|v| v.as_str()) else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        let value = match *field {
            "numeric_value" => raw
                .parse::<f64>()
                .map(|n| json_number(n))
                .unwrap_or_else(|_| Value::String(raw.to_string())),
            "sequence" => raw
                .parse::<i64>()
                .map(|n| Value::Number(n.into()))
                .unwrap_or_else(|_| Value::String(raw.to_string())),
            "is_active" => match raw.to_lowercase().as_str() {
                "true" | "1" | "t" | "yes" => Value::Bool(true),
                "false" | "0" | "f" | "no" => Value::Bool(false),
                _ => Value::String(raw.to_string()),
            },
            _ => Value::String(raw.to_string()),
        };
        body.insert((*field).to_string(), value);
    }
    Value::Object(body)
}

fn json_number(n: f64) -> Value {
    serde_json::Number::from_f64(n)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

// ResultSet rows are Vec<String> aligned to `columns`.
fn row_to_map(columns: &[String], row: &[String]) -> Map<String, Value> {
    let mut m = Map::new();
    for (i, col) in columns.iter().enumerate() {
        m.insert(
            col.clone(),
            Value::String(row.get(i).cloned().unwrap_or_default()),
        );
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile;
    use std::path::Path;

    fn load() -> LoadedProfile {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../profiles/src/code_table_demo");
        profile::load_from_dir(&dir).expect("code_table_demo should load")
    }

    fn sample_inputs(loaded: &LoadedProfile) -> HashMap<String, String> {
        let csv = loaded
            .temp_dir
            .join("test-files")
            .join("sample_classification.csv");
        let mut m = HashMap::new();
        m.insert("Classification".to_string(), csv.to_string_lossy().to_string());
        m
    }

    fn run_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codetable-test-{}", name))
    }

    // The declared table lands on disk under its output label, ready for
    // {{codetable:...}} to point at it.
    #[test]
    fn fetch_all_writes_declared_tables() {
        let loaded = load();
        let dir = run_dir("fetch");
        let paths = fetch_all(&loaded, &Transport::Mock, &dir).expect("fetch ok");

        let path = paths.get("ConstituentCodes").expect("output label present");
        let body = fs::read_to_string(path).expect("file written");
        let rows: Vec<Value> = serde_json::from_str(&body).expect("valid JSON array");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["long_description"], "Alpha");
    }

    // A profile with no code_tables section costs nothing and returns an empty map.
    #[test]
    fn fetch_all_noop_without_declarations() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../profiles/src/test1");
        let loaded = profile::load_from_dir(&dir).expect("test1 loads");
        let paths =
            fetch_all(&loaded, &Transport::Mock, &run_dir("noop")).expect("fetch ok");
        assert!(paths.is_empty());
    }

    // End-to-end read path: the transform SQL joins the pulled table and
    // resolves RE's entry id for the classes that exist in it.
    #[test]
    fn transform_joins_pulled_code_table() {
        let loaded = load();
        let paths = fetch_all(&loaded, &Transport::Mock, &run_dir("join")).expect("fetch ok");
        let sql = loaded.sql_files.get("primary_transform.sql").expect("sql present");

        let rows = db::select_rows(&sample_inputs(&loaded), &paths, sql).expect("select ok");
        let id_col = rows.columns.iter().position(|c| c == "re_code_id").unwrap();
        let class_col = rows.columns.iter().position(|c| c == "vendor_class").unwrap();

        // Alpha is in the fixture (id 6); Delta is not, so it joins to NULL.
        let alpha = rows.rows.iter().find(|r| r[class_col] == "Alpha").unwrap();
        assert_eq!(alpha[id_col], "6");
        let delta = rows.rows.iter().find(|r| r[class_col] == "Delta").unwrap();
        assert!(delta[id_col].is_empty(), "Delta should not match an entry");
    }

    // The write path: the sync query finds exactly the classes RE is missing,
    // and mock mode reports them as written without touching the network.
    #[test]
    fn sync_creates_missing_entries() {
        let loaded = load();
        let step = loaded
            .structure
            .steps
            .iter()
            .find(|s| s.step_type == "code_table_sync")
            .expect("demo has a sync step");

        let res = run_sync(
            &loaded,
            step,
            &sample_inputs(&loaded),
            &Transport::Mock,
            &run_dir("sync"),
        )
        .expect("sync ok");

        // Gamma and Delta are absent from the fixture; Alpha and Beta are present.
        assert_eq!(res.attempted, 2, "message was: {}", res.message);
        assert_eq!(res.succeeded, 2);
        assert!(res.ok && res.failures.is_empty());
        assert_eq!(res.mode, "mock");
        assert_eq!(res.operation, "create");
    }

    // Only API-writable columns reach the request body; helper columns are dropped
    // and DuckDB's VARCHAR casts are coerced back to their JSON types.
    #[test]
    fn entry_body_filters_and_coerces() {
        let mut record = Map::new();
        for (k, v) in [
            ("long_description", "Employee"),
            ("short_description", "EMP"),
            ("numeric_value", "2.5"),
            ("sequence", "3"),
            ("is_active", "true"),
            ("vendor_notes", "ignore me"),
            ("short_description_blank", ""),
        ] {
            record.insert(k.to_string(), Value::String(v.to_string()));
        }
        let body = entry_body(&record);

        assert_eq!(body["long_description"], "Employee");
        assert_eq!(body["numeric_value"], serde_json::json!(2.5));
        assert_eq!(body["sequence"], serde_json::json!(3));
        assert_eq!(body["is_active"], Value::Bool(true));
        assert!(body.get("vendor_notes").is_none(), "extra columns are dropped");
    }

    // update/delete need an entry id; a query that forgets it fails loudly
    // rather than silently writing to the wrong entry.
    #[test]
    fn update_requires_entry_id() {
        let loaded = load();
        let mut step = loaded
            .structure
            .steps
            .iter()
            .find(|s| s.step_type == "code_table_sync")
            .cloned()
            .unwrap();
        step.operation = Some("update".to_string());

        let res = run_sync(
            &loaded,
            &step,
            &sample_inputs(&loaded),
            &Transport::Mock,
            &run_dir("update"),
        )
        .expect("sync runs");

        assert!(!res.ok);
        assert_eq!(res.succeeded, 0);
        assert!(
            res.failures[0].error.contains("table_entries_id"),
            "error was: {}",
            res.failures[0].error
        );
    }
}
