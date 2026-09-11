// db.rs
//
// Owns the DuckDB connection and all query execution.
// Called by commands.rs — never touches the frontend directly.
//
// Responsibilities:
//   - Validate a file's columns against profile expectations
//   - Execute a profile's SQL transform against an input file
//   - Write the result to an output CSV
//   - Return row count and output path

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use duckdb::Connection;
use serde::Serialize;
use crate::errors::AppError;
use crate::profile::ColumnValidation;

// ── Result types returned to commands.rs ─────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ValidationError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub ok: bool,
    pub errors: Vec<ValidationError>,
    pub notices: Vec<Notice>,
}

#[derive(Debug, Serialize)]
pub struct Notice {
    pub label: String,
    pub description: Option<String>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct OutputFile {
    pub label: String,
    // Where the CSV was written — run_transform fills in the absolute path;
    // api.rs relativizes it to an opaque artifact id before it crosses the
    // wire, and save_output resolves it back inside the session.
    pub artifact_id: String,
    pub row_count: usize,
}

#[derive(Debug, Serialize)]
pub struct TransformResult {
    pub outputs: Vec<OutputFile>,
    pub notices: Vec<Notice>,
}

// In-memory result of a SELECT — every cell stringified so the frontend gets
// uniform JSON regardless of the underlying DuckDB column type. Used by the
// report pipeline (report.rs) to feed visualizations directly, with no CSV
// round-trip. Same shape as Notice (minus the label/description).
#[derive(Debug, Serialize)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

// Passed in from commands.rs — already-resolved SQL content for one notice.
pub struct NoticeInput<'a> {
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub sql_content: &'a str,
}

// A filesystem path as it may be embedded in a single-quoted SQL string
// literal: forward slashes (DuckDB accepts them on all platforms, including
// Windows) and embedded quotes doubled so a path containing `'` can't
// terminate the literal early. Every site that splices a path into SQL text
// goes through this.
pub fn sql_path(path: &str) -> String {
    path.replace('\\', "/").replace('\'', "''")
}

// An identifier as it may be embedded in a double-quoted SQL name: embedded
// double quotes doubled, so a header carrying one can't terminate the name
// early. The companion to sql_path for the other kind of quoting.
fn sql_ident(name: &str) -> String {
    name.replace('"', "\"\"")
}

// ── Flattened headers ─────────────────────────────────────────────────────────
// Spreadsheet headers are routinely typed as multi-line cells: "Award QTR/YR"
// reads as two stacked lines in Excel and arrives as a column literally named
// "Award \r\n QTR/YR". No profile author can quote that reliably — the line
// break and the spaces around it are invisible in both a .sql file and a
// structure.yaml label.
//
// So every place the harness hands an input file to DuckDB, it first projects
// the file's headers to their flattened form: each run of whitespace collapsed
// to a single space, the ends trimmed. `"Award QTR/YR"` in SQL, and
// `label: Award QTR/YR` in a `validation:` block, then hit the column the
// operator sees on screen.
//
// The rename happens in the projection, not in the file, and only when a header
// actually needs it — a file whose headers are already clean is read through
// the bare `read_*()` call exactly as before.

// The flattened form of one header. Also collapses the non-breaking spaces
// Excel exports like to leave behind, since char::is_whitespace covers them.
pub fn flatten_header(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

// The read function DuckDB should use for a file, chosen by extension. The
// path is spliced in already quoted, so the result is a complete relation.
fn read_call(path: &Path) -> String {
    let file_str = sql_path(&path.to_string_lossy());
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext == "xlsx" || ext == "xls" {
        format!("read_xlsx('{}')", file_str)
    } else {
        format!("read_csv_auto('{}')", file_str)
    }
}

// Wrap a relation in a projection that renames every header whose flattened
// form differs from the name the file carries. Returns the relation untouched
// when nothing needs flattening, or when it can't be described — an unreadable
// file should fail with its own error where the author's SQL runs, not here.
//
// A header is left under its raw name when flattening it would collide with
// another column: two names that differ only in whitespace can't both become
// the same identifier, and dropping one silently would be worse than leaving
// the author to quote it.
fn flattened_relation(conn: &Connection, relation: &str) -> String {
    let describe = format!("DESCRIBE SELECT * FROM {}", relation);
    let columns: Vec<String> = match conn.prepare(&describe) {
        Ok(mut stmt) => match stmt.query_map([], |row| row.get::<_, String>(0)) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(_) => return relation.to_string(),
        },
        Err(_) => return relation.to_string(),
    };

    let mut taken: Vec<String> = columns.clone();
    let mut renamed = false;
    let mut projection: Vec<String> = Vec::with_capacity(columns.len());
    for raw in &columns {
        let flat = flatten_header(raw);
        if &flat == raw || flat.is_empty() || taken.iter().any(|t| t == &flat) {
            projection.push(format!("\"{}\"", sql_ident(raw)));
        } else {
            projection.push(format!(
                "\"{}\" AS \"{}\"",
                sql_ident(raw),
                sql_ident(&flat)
            ));
            taken.push(flat);
            renamed = true;
        }
    }

    if !renamed {
        return relation.to_string();
    }
    format!("(SELECT {} FROM {})", projection.join(", "), relation)
}

// ── Input substitution ────────────────────────────────────────────────────────
// `{{input:Label}}` resolves to the input's path, as it always has. What the
// harness adds is the surrounding rewrite: where the placeholder is the quoted
// path argument of a `read_*()` call — which is how every profile reads an
// input — the *whole call* is replaced by the flattened projection over it, so
// the author's `FROM read_xlsx('{{input:Recipients}}')` reads the same file
// with headers they can quote.
//
// Anything else the placeholder might be doing falls back to plain path
// substitution, so an unusual call site still runs (just without flattening).

// The span of the `read_*(…)` call that has the placeholder at `at` as its
// quoted path argument, or None if that isn't the shape here. Options after
// the path are part of the span and survive the rewrite, so
// `read_csv('{{input:X}}', header = true)` keeps its options.
fn enclosing_read_call(sql: &str, at: usize, token_len: usize) -> Option<(usize, usize)> {
    let b = sql.as_bytes();

    // Left: the opening quote, the open paren, then a `read*` identifier.
    let quote = at.checked_sub(1)?;
    if b[quote] != b'\'' {
        return None;
    }
    let mut i = quote;
    while i > 0 && b[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let open = i.checked_sub(1)?;
    if b[open] != b'(' {
        return None;
    }
    let mut i = open;
    while i > 0 && b[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let name_end = i;
    while i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        i -= 1;
    }
    let name = &sql[i..name_end];
    if !name.to_ascii_lowercase().starts_with("read") {
        return None;
    }
    let start = i;

    // Right: the closing quote, then the call's own closing paren. Depth
    // counting skips over parens inside string literals and quoted names.
    let mut j = at + token_len;
    if b.get(j) != Some(&b'\'') {
        return None;
    }
    j += 1;
    let mut depth = 1usize;
    while j < b.len() {
        match b[j] {
            b'\'' | b'"' => {
                let q = b[j];
                j += 1;
                while j < b.len() {
                    if b[j] == q {
                        // A doubled quote is an escape, not the end.
                        if b.get(j + 1) == Some(&q) {
                            j += 2;
                            continue;
                        }
                        break;
                    }
                    j += 1;
                }
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((start, j + 1));
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

// Replace every occurrence of one input placeholder, expanding the read call
// around it wherever there is one.
fn substitute_input(conn: &Connection, sql: &str, token: &str, path: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut cursor = 0usize;
    while let Some(offset) = sql[cursor..].find(token) {
        let at = cursor + offset;
        match enclosing_read_call(sql, at, token.len()) {
            Some((start, end)) if start >= cursor => {
                out.push_str(&sql[cursor..start]);
                let call = sql[start..end].replace(token, path);
                out.push_str(&flattened_relation(conn, &call));
                cursor = end;
            }
            _ => {
                out.push_str(&sql[cursor..at]);
                out.push_str(path);
                cursor = at + token.len();
            }
        }
    }
    out.push_str(&sql[cursor..]);
    out
}

// Resolve every `{{input:Label}}` plus the legacy `{{input_file}}` alias.
// `strict_legacy` decides what `{{input_file}}` means in a multi-input
// transform: run_transform wants the loud error so authors disambiguate, while
// the row and notice queries leave the placeholder standing and let DuckDB name
// it.
fn substitute_inputs(
    conn: &Connection,
    sql_content: &str,
    file_paths: &HashMap<String, String>,
    strict_legacy: bool,
) -> Result<String, AppError> {
    let mut sql = sql_content.to_string();
    for (label, path) in file_paths {
        let token = format!("{{{{input:{}}}}}", label);
        sql = substitute_input(conn, &sql, &token, &sql_path(path));
    }
    if sql.contains("{{input_file}}") {
        if file_paths.len() == 1 {
            let only = sql_path(file_paths.values().next().unwrap());
            sql = substitute_input(conn, &sql, "{{input_file}}", &only);
        } else if strict_legacy {
            return Err(AppError::SqlError(format!(
                "SQL uses {{{{input_file}}}} but transform has {} inputs. \
                 Use {{{{input:Label}}}} placeholders to disambiguate.",
                file_paths.len()
            )));
        }
    }
    Ok(sql)
}

// ── validate_file ─────────────────────────────────────────────────────────────
// Opens the input file with DuckDB and checks:
//   1. All required columns exist
//   2. Required columns have no nulls
//   3. Columns with allowed values only contain those values
//   4. Number columns with digit constraints match
//
// Uses DuckDB to read the file so it handles CSV and XLSX the same way.

pub fn validate_file(
    file_path: &Path,
    validations: &[ColumnValidation],
) -> Result<ValidationResult, AppError> {
    let conn = Connection::open_in_memory()
        .map_err(|e| AppError::SqlError(e.to_string()))?;

    // Load the file into a DuckDB view — handles both CSV and XLSX, and reads
    // it through the same flattened-header projection the transforms get, so a
    // `label:` here names the column as it reads in Excel rather than as the
    // multi-line cell the file carries.
    let read_fn = flattened_relation(&conn, &read_call(file_path));

    // Get the actual column names from the file
    let columns_sql = format!("DESCRIBE SELECT * FROM {}", read_fn);
    let mut stmt = conn.prepare(&columns_sql)
        .map_err(|e| AppError::SqlError(format!("Cannot read file headers: {}", e)))?;

    let actual_columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| AppError::SqlError(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let mut errors: Vec<ValidationError> = Vec::new();
    let mut mismatches: Vec<(String, String)> = Vec::new(); // (expected_label, found_column)
    let push = |errors: &mut Vec<ValidationError>, column: &str, message: String| {
        errors.push(ValidationError {
            row: None,
            column: Some(column.to_string()),
            value: None,
            message,
        });
    };

    for v in validations {
        // Resolve which actual column maps to this validation rule:
        //   1. Case-sensitive exact match.
        //   2. Fall back to a normalized match (lowercased, special characters
        //      stripped). If that hits, the validation still applies and a
        //      notice records the mismatch so the author can see it.
        let resolved: Option<String> = if actual_columns.iter().any(|c| c == &v.label) {
            Some(v.label.clone())
        } else {
            let target = normalize_col(&v.label);
            if target.is_empty() {
                None
            } else {
                actual_columns
                    .iter()
                    .find(|c| normalize_col(c) == target)
                    .map(|c| {
                        mismatches.push((v.label.clone(), c.clone()));
                        c.clone()
                    })
            }
        };

        let col = match resolved {
            Some(c) => c,
            None => {
                if v.required {
                    push(&mut errors, &v.label, format!("Missing required column '{}'", v.label));
                }
                continue;
            }
        };

        // Check nulls in required columns
        if v.required {
            let null_sql = format!(
                "SELECT COUNT(*) FROM {} WHERE \"{}\" IS NULL OR TRIM(CAST(\"{}\" AS VARCHAR)) = ''",
                read_fn, col, col
            );
            if let Ok(mut stmt) = conn.prepare(&null_sql) {
                if let Ok(count) = stmt.query_row([], |r| r.get::<_, i64>(0)) {
                    if count > 0 {
                        push(&mut errors, &v.label, format!("{} empty or null value(s)", count));
                    }
                }
            }
        }

        // Check allowed values (controlled vocabulary)
        if let Some(allowed) = &v.value {
            let values_list = allowed
                .iter()
                .map(|v| format!("'{}'", v))
                .collect::<Vec<_>>()
                .join(", ");
            let bad_sql = format!(
                "SELECT COUNT(*) FROM {} WHERE \"{}\" NOT IN ({}) AND \"{}\" IS NOT NULL",
                read_fn, col, values_list, col
            );
            if let Ok(mut stmt) = conn.prepare(&bad_sql) {
                if let Ok(count) = stmt.query_row([], |r| r.get::<_, i64>(0)) {
                    if count > 0 {
                        push(
                            &mut errors,
                            &v.label,
                            format!(
                                "{} value(s) not in allowed list [{}]",
                                count,
                                allowed.join(", ")
                            ),
                        );
                    }
                }
            }
        }

        // Number-typed columns: first check that values are actually numeric,
        // then (optionally) check the digit count.
        if v.col_type == "number" {
            let non_num_sql = format!(
                "SELECT COUNT(*) FROM {} WHERE TRY_CAST(\"{}\" AS DOUBLE) IS NULL \
                 AND \"{}\" IS NOT NULL AND TRIM(CAST(\"{}\" AS VARCHAR)) != ''",
                read_fn, col, col, col
            );
            if let Ok(mut stmt) = conn.prepare(&non_num_sql) {
                if let Ok(count) = stmt.query_row([], |r| r.get::<_, i64>(0)) {
                    if count > 0 {
                        push(
                            &mut errors,
                            &v.label,
                            format!("{} non-numeric value(s)", count),
                        );
                    }
                }
            }

            if let Some(digits) = v.digits {
                let digit_sql = format!(
                    "SELECT COUNT(*) FROM {} WHERE LENGTH(CAST(\"{}\" AS VARCHAR)) != {}",
                    read_fn, col, digits
                );
                if let Ok(mut stmt) = conn.prepare(&digit_sql) {
                    if let Ok(count) = stmt.query_row([], |r| r.get::<_, i64>(0)) {
                        if count > 0 {
                            push(
                                &mut errors,
                                &v.label,
                                format!("{} value(s) are not exactly {} digits", count, digits),
                            );
                        }
                    }
                }
            }
        }
    }

    let mut notices: Vec<Notice> = Vec::new();
    if !mismatches.is_empty() {
        notices.push(Notice {
            label: "Column name mismatches".to_string(),
            description: Some(
                "These columns matched after ignoring case and special characters. \
                 Consider renaming them for an exact match."
                    .to_string(),
            ),
            columns: vec!["Expected".to_string(), "Found in file".to_string()],
            rows: mismatches
                .into_iter()
                .map(|(e, a)| vec![e, a])
                .collect(),
        });
    }

    Ok(ValidationResult {
        ok: errors.is_empty(),
        errors,
        notices,
    })
}

// Lowercased + alphanumeric-only form of a column header — used to compare
// expected vs. actual column names while ignoring case and punctuation.
fn normalize_col(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_alphanumeric())
        .collect()
}

// ── run_transform ─────────────────────────────────────────────────────────────
// Executes the profile's SQL against one or more input files and writes one
// CSV per declared output label.
//
// Input substitution:
//   - `{{input:Label}}` resolves to the path for the input with that label.
//   - `{{input_file}}` is a legacy single-input alias — substituted to the
//     sole path when the transform has exactly one input. Erroring if used
//     with multiple inputs so authors disambiguate.
//
// Output shapes:
//
// 1. Multi-output: the SQL contains one or more {{output:LabelName}}
//    placeholders. Each placeholder resolves to a temp-dir path for that
//    declared output. The SQL is executed as a batch — author writes the
//    COPY statements themselves, one per output.
//
// 2. Single-output legacy: the SQL is a bare SELECT (no {{output:...}}
//    placeholders). It is wrapped in COPY (...) TO 'path' and written to the
//    one declared output. Requires exactly one entry in `output_labels`.

pub fn run_transform(
    file_paths: &HashMap<String, String>,
    sources: &SqlSources,
    sql_content: &str,
    output_labels: &[String],
    notices: &[NoticeInput<'_>],
    out_dir: &Path,
) -> Result<TransformResult, AppError> {
    if output_labels.is_empty() {
        return Err(AppError::SqlError(
            "Transform has no declared outputs".to_string(),
        ));
    }
    if file_paths.is_empty() {
        return Err(AppError::SqlError(
            "Transform was called with no input files".to_string(),
        ));
    }

    let conn = Connection::open_in_memory()
        .map_err(|e| AppError::SqlError(e.to_string()))?;

    // Assign a path per output label up front so we can substitute
    // {{output:Label}} placeholders and remember which path belongs to which.
    // `out_dir` is unique per run (workspace.rs), so plain label-based
    // filenames can't collide across runs or users.
    let outputs: Vec<(String, PathBuf)> = output_labels
        .iter()
        .map(|label| {
            let filename = format!("{}.csv", label.to_lowercase().replace(' ', "_"));
            (label.clone(), out_dir.join(filename))
        })
        .collect();

    // Resolve {{input:Label}} placeholders first, then the legacy
    // {{input_file}} alias, then each {{output:Label}}.
    let mut sql = substitute_inputs(&conn, sql_content, file_paths, true)?;
    sql = sources.apply(&sql);
    let mut multi_output_mode = false;
    for (label, path) in &outputs {
        let placeholder = format!("{{{{output:{}}}}}", label);
        if sql.contains(&placeholder) {
            multi_output_mode = true;
            let path_str = sql_path(&path.to_string_lossy());
            sql = sql.replace(&placeholder, &path_str);
        }
    }

    if multi_output_mode {
        // The author wrote the COPY statements themselves — execute as-is.
        conn.execute_batch(&sql)
            .map_err(|e| AppError::SqlError(format!("Transform failed: {}", e)))?;
    } else {
        // Legacy single-output: wrap the SELECT in a COPY to the lone output.
        if outputs.len() != 1 {
            return Err(AppError::SqlError(format!(
                "Transform declares {} outputs but SQL contains no {{{{output:Label}}}} placeholders. \
                 Either declare exactly one output or add {{{{output:Label}}}} placeholders to the SQL.",
                outputs.len()
            )));
        }
        let output_str = sql_path(&outputs[0].1.to_string_lossy());
        let copy_sql = format!(
            "COPY ({}) TO '{}' (HEADER, DELIMITER ',')",
            sql.trim().trim_end_matches(';').trim(),
            output_str
        );
        conn.execute_batch(&copy_sql)
            .map_err(|e| AppError::SqlError(format!("Transform failed: {}", e)))?;
    }

    // Tally each declared output. A missing file in multi-output mode means
    // the author's SQL didn't actually write to that placeholder — surface
    // that as an error rather than silently returning a 0-row entry.
    let mut output_files: Vec<OutputFile> = Vec::with_capacity(outputs.len());
    for (label, path) in &outputs {
        if !path.exists() {
            return Err(AppError::SqlError(format!(
                "Output '{}' was declared but the SQL did not write to it (expected {{{{output:{}}}}})",
                label, label
            )));
        }
        let path_str = sql_path(&path.to_string_lossy());
        let count_sql = format!("SELECT COUNT(*) FROM read_csv_auto('{}')", path_str);
        let row_count: usize = conn
            .query_row(&count_sql, [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
            .unwrap_or(0);
        output_files.push(OutputFile {
            label: label.clone(),
            artifact_id: path.to_string_lossy().to_string(),
            row_count,
        });
    }

    // Run any attached notice queries against the same inputs. Notices are
    // informational — empty result set means nothing to surface. We discard
    // notices whose own query errors out (logged via the error string in the
    // label) rather than failing the whole transform.
    let notice_results: Vec<Notice> = notices
        .iter()
        .map(|n| run_notice(&conn, n, file_paths, sources))
        .collect();

    Ok(TransformResult {
        outputs: output_files,
        notices: notice_results,
    })
}

// ── RE-sourced placeholders ───────────────────────────────────────────────────
// Every placeholder family besides {{input:}} / {{output:}} resolves to a JSON
// file produced before the SQL runs, all read with read_json_auto:
//   {{codetable:Label}} — one code table's entries      (code_tables.rs)
//   {{query:Label}}     — one re_query step's results   (query_step.rs)
//   {{sync:Label}}      — one code_table_sync's outcome (code_tables.rs)
//   {{form:Label}}      — one user_input step's values  (user_input.rs)
// Kept separate from {{input:Label}} so a profile can tell "a file the user
// picked" apart from "data produced by an earlier step" at a glance.

pub fn substitute_labeled_paths(
    sql: &str,
    kind: &str,
    paths: &HashMap<String, String>,
) -> String {
    let mut out = sql.to_string();
    for (label, path) in paths {
        let placeholder = format!("{{{{{}:{}}}}}", kind, label);
        out = out.replace(&placeholder, &sql_path(path));
    }
    out
}

// The kinds in use. All resolve to a JSON file read with read_json_auto.
pub const KIND_CODETABLE: &str = "codetable";
pub const KIND_QUERY: &str = "query";
pub const KIND_SYNC: &str = "sync";
pub const KIND_FORM: &str = "form";

// ── SqlSources ────────────────────────────────────────────────────────────────
// The registry of label→path maps a SQL body can reference, keyed by placeholder
// family. Everything that runs profile SQL takes one of these instead of a
// parameter per family, so adding a family later is a `KIND_` const plus one
// `.with(...)` at the call site — not a new argument threaded through
// run_transform, select_rows and run_notice.

#[derive(Debug, Default, Clone)]
pub struct SqlSources {
    families: Vec<(&'static str, HashMap<String, String>)>,
}

impl SqlSources {
    pub fn new() -> Self {
        Self::default()
    }

    // Register one family. An empty map is kept rather than skipped so the
    // debug view still shows which families a call site considered.
    pub fn with(mut self, kind: &'static str, paths: &HashMap<String, String>) -> Self {
        self.families.push((kind, paths.clone()));
        self
    }

    // Resolve every registered family in one pass. Unregistered labels are left
    // as literal `{{kind:Label}}` text so DuckDB names them in its error rather
    // than the run silently reading nothing.
    pub fn apply(&self, sql: &str) -> String {
        let mut out = sql.to_string();
        for (kind, paths) in &self.families {
            out = substitute_labeled_paths(&out, kind, paths);
        }
        out
    }
}

// ── select_rows ───────────────────────────────────────────────────────────────
// Runs a bare SELECT over the profile's inputs and code tables and returns the
// rows. Used by code_table_sync steps, where each returned row becomes one write
// against RE — no output file is produced.

pub fn select_rows(
    file_paths: &HashMap<String, String>,
    sources: &SqlSources,
    sql_content: &str,
) -> Result<ResultSet, AppError> {
    let conn = Connection::open_in_memory().map_err(|e| AppError::SqlError(e.to_string()))?;
    let sql = substitute_inputs(&conn, sql_content, file_paths, false)?;
    let sql = sources.apply(&sql);
    query_to_result_set(&conn, &sql)
}

// ── run_notice ────────────────────────────────────────────────────────────────
// Executes a single notice query and returns the rows as plain strings.
// Notices are wrapped in `SELECT CAST(col AS VARCHAR)` so every value can be
// read as a String regardless of the underlying DuckDB column type — keeps
// the serialization to the frontend uniform.

fn run_notice(
    conn: &Connection,
    n: &NoticeInput<'_>,
    file_paths: &HashMap<String, String>,
    sources: &SqlSources,
) -> Notice {
    let user_sql = match substitute_inputs(conn, n.sql_content, file_paths, false) {
        Ok(sql) => sql,
        Err(e) => {
            return Notice {
                label: n.label.to_string(),
                description: n.description.map(|s| s.to_string()),
                columns: vec!["Error".to_string()],
                rows: vec![vec![format!("Notice query failed: {}", e)]],
            };
        }
    };
    let user_sql = sources.apply(&user_sql);
    let trimmed = user_sql.trim().trim_end_matches(';').trim();

    // Phase 1: discover the column names returned by the user's query.
    let describe_sql = format!("DESCRIBE {}", trimmed);
    let columns: Vec<String> = match conn.prepare(&describe_sql) {
        Ok(mut stmt) => stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default(),
        Err(e) => {
            return Notice {
                label: n.label.to_string(),
                description: n.description.map(|s| s.to_string()),
                columns: vec!["Error".to_string()],
                rows: vec![vec![format!("Notice query failed: {}", e)]],
            };
        }
    };

    if columns.is_empty() {
        return Notice {
            label: n.label.to_string(),
            description: n.description.map(|s| s.to_string()),
            columns: vec![],
            rows: vec![],
        };
    }

    // Phase 2: re-issue the query wrapped in a CAST-to-VARCHAR projection so
    // every cell deserializes as a String.
    let cast_list = columns
        .iter()
        .map(|c| format!("CAST(\"{}\" AS VARCHAR) AS \"{}\"", c, c))
        .collect::<Vec<_>>()
        .join(", ");
    let wrapped = format!("SELECT {} FROM ({}) _notice", cast_list, trimmed);

    let rows: Vec<Vec<String>> = match conn.prepare(&wrapped) {
        Ok(mut stmt) => {
            let col_count = columns.len();
            stmt.query_map([], |row| {
                let mut data: Vec<String> = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    let v: Option<String> = row.get(i).unwrap_or(None);
                    data.push(v.unwrap_or_default());
                }
                Ok(data)
            })
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        }
        Err(e) => vec![vec![format!("Notice query failed: {}", e)]],
    };

    Notice {
        label: n.label.to_string(),
        description: n.description.map(|s| s.to_string()),
        columns,
        rows,
    }
}

// ── query_to_result_set ─────────────────────────────────────────────────────────
// Runs a SELECT and returns its columns + rows as plain strings. Same two-phase
// approach as run_notice (DESCRIBE to discover columns, then re-issue wrapped in
// CAST(col AS VARCHAR) so every value reads as a String) — but surfaces failures
// as AppError instead of embedding an error row, since report transforms must
// fail loudly. The SQL is expected to be a bare SELECT (placeholders already
// substituted by the caller).

pub fn query_to_result_set(conn: &Connection, sql: &str) -> Result<ResultSet, AppError> {
    let trimmed = sql.trim().trim_end_matches(';').trim();

    // Phase 1: discover the column names returned by the query.
    let describe_sql = format!("DESCRIBE {}", trimmed);
    let columns: Vec<String> = {
        let mut stmt = conn
            .prepare(&describe_sql)
            .map_err(|e| AppError::SqlError(format!("Transform failed: {}", e)))?;
        stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| AppError::SqlError(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect()
    };

    if columns.is_empty() {
        return Ok(ResultSet { columns, rows: vec![] });
    }

    // Phase 2: re-issue the query wrapped in a CAST-to-VARCHAR projection so
    // every cell deserializes as a String.
    let cast_list = columns
        .iter()
        .map(|c| format!("CAST(\"{}\" AS VARCHAR) AS \"{}\"", c, c))
        .collect::<Vec<_>>()
        .join(", ");
    let wrapped = format!("SELECT {} FROM ({}) _rs", cast_list, trimmed);

    let col_count = columns.len();
    let mut stmt = conn
        .prepare(&wrapped)
        .map_err(|e| AppError::SqlError(format!("Transform failed: {}", e)))?;
    let rows: Vec<Vec<String>> = stmt
        .query_map([], |row| {
            let mut data: Vec<String> = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let v: Option<String> = row.get(i).unwrap_or(None);
                data.push(v.unwrap_or_default());
            }
            Ok(data)
        })
        .map_err(|e| AppError::SqlError(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(ResultSet { columns, rows })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile;

    // The scholarship workbook is the reason this harness exists: every one of
    // its stacked headers arrives as "Bronco\r\n ID", "Award\r\n QTR/YR" and so
    // on, and the profile quotes them as they read on screen.
    fn billings() -> HashMap<String, String> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../profiles/src/scholarship_recipients");
        let loaded = profile::load_from_dir(&dir).expect("scholarship_recipients should load");
        let xlsx = loaded.temp_dir.join("test-files").join("billings.xlsx");
        let mut m = HashMap::new();
        m.insert("Recipients".to_string(), xlsx.to_string_lossy().to_string());
        m
    }

    #[test]
    fn flatten_header_collapses_line_breaks() {
        assert_eq!(flatten_header("Award\r\n QTR/YR"), "Award QTR/YR");
        assert_eq!(flatten_header("Bronco\n ID"), "Bronco ID");
        assert_eq!(flatten_header("  Amount \t"), "Amount");
        // Already clean headers come back byte-identical, which is what keeps
        // the read call unwrapped for files that need no flattening.
        assert_eq!(flatten_header("Project Title"), "Project Title");
    }

    #[test]
    fn enclosing_read_call_spans_the_whole_call() {
        let token = "{{input:X}}";
        let sql = "FROM read_xlsx('{{input:X}}') v";
        let at = sql.find(token).unwrap();
        let (s, e) = enclosing_read_call(sql, at, token.len()).expect("call found");
        assert_eq!(&sql[s..e], "read_xlsx('{{input:X}}')");

        // Options after the path stay inside the span, so they survive the
        // rewrite rather than being dropped on the floor.
        let sql = "FROM read_csv('{{input:X}}', header = true, sep = ',') v";
        let at = sql.find(token).unwrap();
        let (s, e) = enclosing_read_call(sql, at, token.len()).expect("call found");
        assert_eq!(&sql[s..e], "read_csv('{{input:X}}', header = true, sep = ',')");
    }

    #[test]
    fn enclosing_read_call_declines_other_shapes() {
        let token = "{{input:X}}";
        // Not a read call — falls back to plain path substitution.
        let sql = "SELECT '{{input:X}}' AS path";
        let at = sql.find(token).unwrap();
        assert!(enclosing_read_call(sql, at, token.len()).is_none());
        // Not quoted at all.
        let sql = "FROM read_xlsx({{input:X}})";
        let at = sql.find(token).unwrap();
        assert!(enclosing_read_call(sql, at, token.len()).is_none());
    }

    // The end of the harness: SQL quoting the flattened names reads the file.
    #[test]
    fn transform_sql_sees_flattened_headers() {
        let sources = SqlSources::new();
        let rows = select_rows(
            &billings(),
            &sources,
            "SELECT \"Bronco ID\", \"Award QTR/YR\", \"Foundation Project\" \
             FROM read_xlsx('{{input:Recipients}}')",
        )
        .expect("flattened headers should resolve");
        assert_eq!(
            rows.columns,
            vec!["Bronco ID", "Award QTR/YR", "Foundation Project"]
        );
        assert!(!rows.rows.is_empty(), "the workbook has award rows");
    }

    // Numeric-looking identifiers are the other way a workbook column lies about
    // itself. Excel stores a Foundation Project like 10001 as a number, so
    // DuckDB types the column DOUBLE and the obvious cast renders "10001.0" —
    // which matches no id any API returns, and fails silently as "nothing
    // matched" rather than as an error. Reading the file with all_varchar=true
    // gives back what the cell shows; this pins both halves, because the
    // profiles rely on that option surviving the flattening rewrite.
    #[test]
    fn numeric_ids_read_as_written_only_with_all_varchar() {
        let sources = SqlSources::new();
        let naive = select_rows(
            &billings(),
            &sources,
            "SELECT CAST(\"Foundation Project\" AS VARCHAR) AS id \
             FROM read_xlsx('{{input:Recipients}}') LIMIT 1",
        )
        .expect("plain read resolves");
        assert!(
            naive.rows[0][0].ends_with(".0"),
            "expected the DOUBLE round-trip this guards against, got {:?}",
            naive.rows[0][0]
        );

        let text = select_rows(
            &billings(),
            &sources,
            "SELECT CAST(\"Foundation Project\" AS VARCHAR) AS id \
             FROM read_xlsx('{{input:Recipients}}', all_varchar=true) LIMIT 1",
        )
        .expect("all_varchar read resolves — and keeps the flattened headers");
        assert!(
            !text.rows[0][0].contains('.'),
            "all_varchar should give the cell as written, got {:?}",
            text.rows[0][0]
        );
        assert_eq!(text.rows[0][0], naive.rows[0][0].trim_end_matches(".0"));
    }

    // A header that needs no flattening is untouched, so existing profiles keep
    // reading their files exactly as before.
    #[test]
    fn clean_headers_are_left_alone() {
        let conn = Connection::open_in_memory().unwrap();
        let call = "read_csv_auto('nope.csv')";
        // Undescribable relation: returned as-is rather than guessed at.
        assert_eq!(flattened_relation(&conn, call), call);
    }

    // validate_file resolves a declared label against the flattened header
    // exactly — no "column name mismatch" notice, which is what the fuzzy
    // fallback used to produce for every stacked header in the workbook.
    #[test]
    fn validation_labels_match_flattened_headers() {
        let paths = billings();
        let file = paths.get("Recipients").unwrap();
        let validations = vec![
            ColumnValidation {
                label: "Bronco ID".to_string(),
                required: true,
                col_type: "number".to_string(),
                digits: None,
                value: None,
            },
            ColumnValidation {
                label: "Award QTR/YR".to_string(),
                required: true,
                col_type: "string".to_string(),
                digits: None,
                value: None,
            },
            ColumnValidation {
                label: "Foundation Project".to_string(),
                required: true,
                col_type: "string".to_string(),
                digits: None,
                value: None,
            },
        ];
        let result = validate_file(Path::new(file), &validations).expect("validation runs");
        assert!(
            result.errors.is_empty(),
            "no column should be reported missing: {:?}",
            result.errors
        );
        assert!(
            result.notices.is_empty(),
            "an exact flattened match is not a mismatch: {:?}",
            result.notices
        );
    }
}
