use std::fmt;

#[derive(Debug)]
pub enum AppError {
    SqlError(String),
    IoError(String),
    ParseError(String),
    NetworkError(String),
    AuthError(String),
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
            AppError::NetworkError(msg) =>
                write!(f, "Network error: {}", msg),
            AppError::AuthError(msg) =>
                write!(f, "Auth error: {}", msg),
        }
    }
}