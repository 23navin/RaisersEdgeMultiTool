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
    pub path: String,
    pub row_count: usize,
}

#[derive(Debug, Serialize)]
pub struct TransformResult {
    pub outputs: Vec<OutputFile>,
    pub notices: Vec<Notice>,
}

// Passed in from commands.rs — already-resolved SQL content for one notice.
pub struct NoticeInput<'a> {
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub sql_content: &'a str,
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

    // Load the file into a DuckDB view — handles both CSV and XLSX
    let file_str = file_path.to_string_lossy().replace('\\', "/");
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let read_fn = if ext == "xlsx" || ext == "xls" {
        format!("read_xlsx('{}')", file_str)
    } else {
        format!("read_csv_auto('{}')", file_str)
    };

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

        // Check digit length for number columns
        if v.col_type == "number" {
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
    sql_content: &str,
    output_labels: &[String],
    notices: &[NoticeInput<'_>],
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

    // Forward slashes work on all platforms including Windows in DuckDB
    let normalized: HashMap<&str, String> = file_paths
        .iter()
        .map(|(k, v)| (k.as_str(), v.replace('\\', "/")))
        .collect();

    // Assign a temp-dir path per output label up front so we can substitute
    // {{output:Label}} placeholders and remember which path belongs to which.
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let outputs: Vec<(String, PathBuf)> = output_labels
        .iter()
        .map(|label| {
            let filename = format!(
                "{}_{}.csv",
                label.to_lowercase().replace(' ', "_"),
                timestamp
            );
            (label.clone(), std::env::temp_dir().join(filename))
        })
        .collect();

    // Resolve {{input:Label}} placeholders first, then the legacy
    // {{input_file}} alias, then each {{output:Label}}.
    let mut sql = sql_content.to_string();
    for (label, path) in &normalized {
        let placeholder = format!("{{{{input:{}}}}}", label);
        sql = sql.replace(&placeholder, path);
    }
    if sql.contains("{{input_file}}") {
        if normalized.len() != 1 {
            return Err(AppError::SqlError(format!(
                "SQL uses {{{{input_file}}}} but transform has {} inputs. \
                 Use {{{{input:Label}}}} placeholders to disambiguate.",
                normalized.len()
            )));
        }
        let only_path = normalized.values().next().unwrap();
        sql = sql.replace("{{input_file}}", only_path);
    }
    let mut multi_output_mode = false;
    for (label, path) in &outputs {
        let placeholder = format!("{{{{output:{}}}}}", label);
        if sql.contains(&placeholder) {
            multi_output_mode = true;
            let path_str = path.to_string_lossy().replace('\\', "/");
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
        let output_str = outputs[0].1.to_string_lossy().replace('\\', "/");
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
        let path_str = path.to_string_lossy().replace('\\', "/");
        let count_sql = format!("SELECT COUNT(*) FROM read_csv_auto('{}')", path_str);
        let row_count: usize = conn
            .query_row(&count_sql, [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
            .unwrap_or(0);
        output_files.push(OutputFile {
            label: label.clone(),
            path: path.to_string_lossy().to_string(),
            row_count,
        });
    }

    // Run any attached notice queries against the same inputs. Notices are
    // informational — empty result set means nothing to surface. We discard
    // notices whose own query errors out (logged via the error string in the
    // label) rather than failing the whole transform.
    let notice_results: Vec<Notice> = notices
        .iter()
        .map(|n| run_notice(&conn, n, &normalized))
        .collect();

    Ok(TransformResult {
        outputs: output_files,
        notices: notice_results,
    })
}

// ── run_notice ────────────────────────────────────────────────────────────────
// Executes a single notice query and returns the rows as plain strings.
// Notices are wrapped in `SELECT CAST(col AS VARCHAR)` so every value can be
// read as a String regardless of the underlying DuckDB column type — keeps
// the serialization to the frontend uniform.

fn run_notice(
    conn: &Connection,
    n: &NoticeInput<'_>,
    file_paths: &HashMap<&str, String>,
) -> Notice {
    let mut user_sql = n.sql_content.to_string();
    for (label, path) in file_paths {
        let placeholder = format!("{{{{input:{}}}}}", label);
        user_sql = user_sql.replace(&placeholder, path);
    }
    if user_sql.contains("{{input_file}}") && file_paths.len() == 1 {
        let only_path = file_paths.values().next().unwrap();
        user_sql = user_sql.replace("{{input_file}}", only_path);
    }
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