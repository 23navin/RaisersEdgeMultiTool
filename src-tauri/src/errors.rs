use std::fmt;

#[derive(Debug)]
pub enum AppError {
    SqlError(String),
    IoError(String),
    ParseError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::SqlError(msg) =>
                write!(f, "SQL error: {}", msg),
            AppError::IoError(msg) =>
                write!(f, "IO error: {}", msg),
            AppError::ParseError(msg) =>
                write!(f, "Parse error: {}", msg),
        }
    }
}