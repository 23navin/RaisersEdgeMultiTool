// ── query_step.rs ──────────────────────────────────────────────────────────────
// Runs an RE query in the middle of an import pipeline.
//
//   uploaded files ─▶ params_sql ─▶ {{rows:}}/{{value:}} into `template`
//                                 ─▶ RE query (re_calls) ─▶ JSON on disk
//                                 ─▶ {{query:Label}} in later SQL steps
//
// The step is the only place an import profile talks to the Query API. Its
// result is written to the run temp dir and handed back to the frontend as a
// path; App.tsx passes that path into whichever later transform declared the
// label in `query_input`. Nothing is cached backend-side — see the plan's note
// on why invalidation stays in the frontend.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::db::{self, ResultSet};
use crate::errors::AppError;
use crate::profile::{LoadedProfile, Step};
use crate::re_calls::{self, Transport};
use crate::report::apply_subs;

#[derive(Debug, Serialize)]
pub struct QueryStepResult {
    pub query_output: String, // the label later SQL uses as {{query:<label>}}
    pub path: String,         // JSON file the rows were written to
    pub row_count: usize,
    pub mode: String, // "live" | "mock"
    // The request actually sent, after substitution — the same debugging aid
    // report.rs exposes as QueryDebug.resolved_request.
    pub resolved_request: Value,
}

// Execute one re_query step.
pub fn run_query(
    loaded: &LoadedProfile,
    step: &Step,
    file_paths: &HashMap<String, String>,
    code_table_paths: &HashMap<String, String>,
    transport: &Transport,
    run_dir: &Path,
) -> Result<QueryStepResult, AppError> {
    let query_output = step.query_output.as_deref().ok_or_else(|| {
        AppError::ParseError(format!(
            "re_query step '{}' has no `query_output` naming its result",
            step.label
        ))
    })?;

    // 1. Values from the uploaded files, if the step asks for any. A step with
    //    a fully static template needs no params_sql.
    let subs = match step.params_sql.as_deref() {
        Some(name) => {
            let sql = loaded.sql_files.get(name).ok_or_else(|| {
                AppError::ParseError(format!(
                    "re_query step '{}': params_sql file '{}' not found in profile",
                    step.label, name
                ))
            })?;
            // Code tables only: a params query reads uploaded files and code
            // tables, never another step's results (that would need ordering
            // guarantees the step model doesn't provide).
            let sources = db::SqlSources::new().with(db::KIND_CODETABLE, code_table_paths);
            let rows = db::select_rows(file_paths, &sources, sql)?;
            Some(rows)
        }
        None => None,
    };

    // 2. The request: the inline `template`, else `bind` (saved-query style).
    let base = match &step.template {
        Some(tpl) => tpl.clone(),
        None => Value::Object(
            step.bind
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
    };
    let request = match &subs {
        Some(rows) => resolve_request(&base, rows),
        None => base,
    };

    // 3. Run it — live SKY API or the bundle's fixture.
    let fixture = loaded
        .temp_dir
        .join("fixtures")
        .join("queries")
        .join(format!("{}.json", query_output));

    let rows = re_calls::execute_query(
        transport,
        step.call_ref.as_deref(),
        step.template.is_some(),
        &request,
        &fixture,
    )
    .map_err(|e| {
        AppError::NetworkError(format!("re_query step '{}': {}", step.label, e))
    })?;

    // 4. Write the rows where later SQL will read them.
    fs::create_dir_all(run_dir)
        .map_err(|e| AppError::IoError(format!("Cannot create query run dir: {}", e)))?;
    let path: PathBuf = run_dir.join(format!("query_{}.json", query_output));
    fs::write(&path, serde_json::to_vec(&rows).unwrap_or_default())
        .map_err(|e| AppError::IoError(format!("Cannot write query result: {}", e)))?;

    let row_count = rows.as_array().map(|a| a.len()).unwrap_or(0);

    Ok(QueryStepResult {
        query_output: query_output.to_string(),
        path: path.to_string_lossy().to_string(),
        row_count,
        mode: transport.label().to_string(),
        resolved_request: request,
    })
}

// ── params substitution ────────────────────────────────────────────────────────
// Two forms, because a query filter needs both shapes:
//
//   {{rows:col}}   the WHOLE string is replaced by a JSON array of that column's
//                  values — what an `OneOf` filter_values wants.
//   {{value:col}}  the first row's cell, substituted inline like {{param:}}.
//
// report.rs::resolve_value only rewrites text inside strings, so it can't produce
// an array; that's why {{rows:}} is handled here before delegating.

fn resolve_request(base: &Value, rows: &ResultSet) -> Value {
    let scalar_subs = value_subs(rows);
    resolve_with_arrays(base, rows, &scalar_subs)
}

fn resolve_with_arrays(
    value: &Value,
    rows: &ResultSet,
    scalar_subs: &[(String, String)],
) -> Value {
    match value {
        // A string that is exactly one {{rows:col}} becomes the column's values.
        Value::String(s) => match rows_placeholder_column(s) {
            Some(col) => match column_values(rows, &col) {
                Some(vals) => Value::Array(vals),
                // Unknown column: leave the placeholder in place so the API's
                // error names it, rather than silently sending an empty filter.
                None => Value::String(s.clone()),
            },
            None => Value::String(apply_subs(s, scalar_subs)),
        },
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| resolve_with_arrays(v, rows, scalar_subs))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), resolve_with_arrays(v, rows, scalar_subs)))
                .collect(),
        ),
        other => other.clone(),
    }
}

// Some("col") when the string is exactly "{{rows:col}}" (whitespace tolerated).
fn rows_placeholder_column(s: &str) -> Option<String> {
    let t = s.trim();
    let inner = t.strip_prefix("{{")?.strip_suffix("}}")?;
    let col = inner.trim().strip_prefix("rows:")?;
    Some(col.trim().to_string())
}

// A column's values: non-empty, deduped, original order — the same treatment
// report.rs gives the ids it collects for a saved query.
fn column_values(rows: &ResultSet, column: &str) -> Option<Vec<Value>> {
    let idx = rows.columns.iter().position(|c| c == column)?;
    let mut seen = std::collections::HashSet::new();
    Some(
        rows.rows
            .iter()
            .filter_map(|r| r.get(idx))
            .filter(|v| !v.is_empty())
            .filter(|v| seen.insert((*v).clone()))
            .map(|v| Value::String(v.clone()))
            .collect(),
    )
}

// {{value:col}} → the first row's cell, for scalar substitution.
fn value_subs(rows: &ResultSet) -> Vec<(String, String)> {
    rows.columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let first = rows
                .rows
                .first()
                .and_then(|r| r.get(i))
                .cloned()
                .unwrap_or_default();
            (format!("{{{{value:{}}}}}", col), first)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile;
    use std::path::Path;

    fn load() -> LoadedProfile {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../profiles/src/re_query_demo");
        profile::load_from_dir(&dir).expect("re_query_demo should load")
    }

    fn inputs(loaded: &LoadedProfile) -> HashMap<String, String> {
        let csv = loaded.temp_dir.join("test-files").join("sample_vendor.csv");
        let mut m = HashMap::new();
        m.insert("Vendor".to_string(), csv.to_string_lossy().to_string());
        m
    }

    fn query_step(loaded: &LoadedProfile) -> &Step {
        loaded
            .structure
            .steps
            .iter()
            .find(|s| s.step_type == "re_query")
            .expect("demo has an re_query step")
    }

    fn run_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("query-step-test-{}", name))
    }

    fn rows_fixture() -> ResultSet {
        ResultSet {
            columns: vec!["record_id".to_string(), "region".to_string()],
            rows: vec![
                vec!["12345".to_string(), "East".to_string()],
                vec!["23456".to_string(), "West".to_string()],
                vec!["12345".to_string(), "East".to_string()], // duplicate
                vec!["".to_string(), "South".to_string()],     // empty id
            ],
        }
    }

    // The crux: "{{rows:col}}" must become a JSON ARRAY, not a string. This is
    // what report.rs::resolve_value cannot do.
    #[test]
    fn rows_placeholder_becomes_json_array() {
        let base = serde_json::json!({
            "filter_fields": [{ "operator": "OneOf", "filter_values": "{{rows:record_id}}" }]
        });
        let out = resolve_request(&base, &rows_fixture());
        let vals = &out["filter_fields"][0]["filter_values"];

        assert!(vals.is_array(), "expected an array, got: {}", vals);
        // Deduped, empties dropped, order preserved.
        assert_eq!(*vals, serde_json::json!(["12345", "23456"]));
    }

    // {{value:col}} stays a scalar and substitutes inline.
    #[test]
    fn value_placeholder_substitutes_inline() {
        let base = serde_json::json!({ "name": "Batch for {{value:region}}" });
        let out = resolve_request(&base, &rows_fixture());
        assert_eq!(out["name"], "Batch for East");
    }

    // An unknown column leaves the placeholder intact rather than silently
    // sending an empty filter that would match everything.
    #[test]
    fn unknown_column_is_left_visible() {
        let base = serde_json::json!({ "filter_values": "{{rows:nope}}" });
        let out = resolve_request(&base, &rows_fixture());
        assert_eq!(out["filter_values"], "{{rows:nope}}");
    }

    // End to end in mock mode: params SQL runs, the fixture is read, rows land
    // on disk under the declared label.
    #[test]
    fn run_query_writes_result_for_downstream_sql() {
        let loaded = load();
        let res = run_query(
            &loaded,
            query_step(&loaded),
            &inputs(&loaded),
            &HashMap::new(),
            &Transport::Mock,
            &run_dir("run"),
        )
        .expect("query runs");

        assert_eq!(res.query_output, "RERecords");
        assert_eq!(res.mode, "mock");
        assert_eq!(res.row_count, 4);

        // The params SQL's ids reached the request as an array.
        let vals = &res.resolved_request["filter_fields"][0]["filter_values"];
        assert!(vals.is_array(), "filter_values was: {}", vals);
        assert_eq!(vals.as_array().unwrap().len(), 4);

        let body = fs::read_to_string(&res.path).expect("result written");
        assert!(body.contains("Ada Lovelace"));
    }

    // The whole point: a later transform reads {{query:Label}} and joins it.
    #[test]
    fn downstream_transform_joins_query_result() {
        let loaded = load();
        let res = run_query(
            &loaded,
            query_step(&loaded),
            &inputs(&loaded),
            &HashMap::new(),
            &Transport::Mock,
            &run_dir("join"),
        )
        .expect("query runs");

        let mut query_paths = HashMap::new();
        query_paths.insert(res.query_output.clone(), res.path.clone());

        let sql = loaded.sql_files.get("build_import.sql").expect("sql present");
        let sources = db::SqlSources::new().with(db::KIND_QUERY, &query_paths);
        let rows = db::select_rows(&inputs(&loaded), &sources, sql).expect("transform runs");

        // 12345 and 34567 changed email; 23456 matches RE already; 99999 is not
        // in RE at all.
        assert_eq!(rows.rows.len(), 2, "columns: {:?}", rows.columns);
        let id = rows.columns.iter().position(|c| c == "record_id").unwrap();
        let ids: Vec<&String> = rows.rows.iter().map(|r| &r[id]).collect();
        assert!(ids.contains(&&"12345".to_string()));
        assert!(ids.contains(&&"34567".to_string()));
    }

    // A step with no query_output can't be referenced, so it's rejected.
    #[test]
    fn missing_query_output_errors() {
        let loaded = load();
        let mut step = query_step(&loaded).clone();
        step.query_output = None;

        let err = run_query(
            &loaded,
            &step,
            &inputs(&loaded),
            &HashMap::new(),
            &Transport::Mock,
            &run_dir("nooutput"),
        )
        .expect_err("should reject");
        assert!(err.to_string().contains("query_output"), "was: {}", err);
    }
}
