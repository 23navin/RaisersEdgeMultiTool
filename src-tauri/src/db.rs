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

use std::path::{Path, PathBuf};
use duckdb::Connection;
use serde::Serialize;
use crate::errors::AppError;
use crate::profile::{LoadedProfile, ColumnValidation};

// ── Result types returned to commands.rs ─────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TransformResult {
    pub output_path: String,
    pub row_count: usize,
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

    let mut errors: Vec<String> = Vec::new();

    for v in validations {
        // Check column exists
        if !actual_columns.iter().any(|c| c == &v.label) {
            if v.required {
                errors.push(format!("Missing required column: '{}'", v.label));
            }
            continue; // skip further checks if column absent
        }

        // Check nulls in required columns
        if v.required {
            let null_sql = format!(
                "SELECT COUNT(*) FROM {} WHERE \"{}\" IS NULL OR TRIM(CAST(\"{}\" AS VARCHAR)) = ''",
                read_fn, v.label, v.label
            );
            if let Ok(mut stmt) = conn.prepare(&null_sql) {
                if let Ok(count) = stmt.query_row([], |r| r.get::<_, i64>(0)) {
                    if count > 0 {
                        errors.push(format!(
                            "Column '{}' has {} empty or null value(s)",
                            v.label, count
                        ));
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
                read_fn, v.label, values_list, v.label
            );
            if let Ok(mut stmt) = conn.prepare(&bad_sql) {
                if let Ok(count) = stmt.query_row([], |r| r.get::<_, i64>(0)) {
                    if count > 0 {
                        errors.push(format!(
                            "Column '{}' has {} value(s) not in allowed list: {}",
                            v.label,
                            count,
                            allowed.join(", ")
                        ));
                    }
                }
            }
        }

        // Check digit length for number columns
        if v.col_type == "number" {
            if let Some(digits) = v.digits {
                let digit_sql = format!(
                    "SELECT COUNT(*) FROM {} WHERE LENGTH(CAST(\"{}\" AS VARCHAR)) != {}",
                    read_fn, v.label, digits
                );
                if let Ok(mut stmt) = conn.prepare(&digit_sql) {
                    if let Ok(count) = stmt.query_row([], |r| r.get::<_, i64>(0)) {
                        if count > 0 {
                            errors.push(format!(
                                "Column '{}' has {} value(s) that are not exactly {} digits",
                                v.label, count, digits
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(ValidationResult {
        ok: errors.is_empty(),
        errors,
    })
}

// ── run_transform ─────────────────────────────────────────────────────────────
// Executes the profile's SQL against the input file and writes a CSV output.
// {{input_file}} in the SQL is replaced with the actual file path.
// Output is written to the same directory as the input file with a timestamp.

pub fn run_transform(
    file_path: &Path,
    sql_content: &str,
    output_label: &str,
) -> Result<TransformResult, AppError> {
    let conn = Connection::open_in_memory()
        .map_err(|e| AppError::SqlError(e.to_string()))?;

    // Replace the placeholder with the actual file path
    // Forward slashes work on all platforms including Windows in DuckDB
    let file_str = file_path.to_string_lossy().replace('\\', "/");
    let sql = sql_content.replace("{{input_file}}", &file_str);

    // Build output path: same dir as input, labeled with output name + timestamp
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let output_filename = format!(
        "{}_{}.csv",
        output_label.to_lowercase().replace(' ', "_"),
        timestamp
    );
    let output_path: PathBuf = file_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&output_filename);

    let output_str = output_path.to_string_lossy().replace('\\', "/");

    // Execute the transform and write directly to CSV via DuckDB COPY
    let copy_sql = format!(
        "COPY ({}) TO '{}' (HEADER, DELIMITER ',')",
        sql.trim_end_matches(';'),
        output_str
    );

    conn.execute_batch(&copy_sql)
        .map_err(|e| AppError::SqlError(format!("Transform failed: {}", e)))?;

    // Count output rows
    let count_sql = format!("SELECT COUNT(*) FROM read_csv_auto('{}')", output_str);
    let row_count: usize = conn
        .query_row(&count_sql, [], |r| r.get::<_, i64>(0))
        .map(|n| n as usize)
        .unwrap_or(0);

    Ok(TransformResult {
        output_path: output_path.to_string_lossy().to_string(),
        row_count,
    })
}