use std::fmt;

#[derive(Debug)]
pub enum AppError {
    ProfileNotFound(String),
    InvalidFileType { got: String, expected: Vec<String> },
    MissingColumns(Vec<String>),
    SqlError(String),
    IoError(String),
    ParseError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::ProfileNotFound(p) =>
                write!(f, "Profile not found: {}", p),
            AppError::InvalidFileType { got, expected } =>
                write!(f, "Invalid file type '{}', expected one of: {}", got, expected.join(", ")),
            AppError::MissingColumns(cols) =>
                write!(f, "Missing expected columns: {}", cols.join(", ")),
            AppError::SqlError(msg) =>
                write!(f, "SQL error: {}", msg),
            AppError::IoError(msg) =>
                write!(f, "IO error: {}", msg),
            AppError::ParseError(msg) =>
                write!(f, "Parse error: {}", msg),
        }
    }
}