pub mod apache;
pub mod caddy;
pub mod config;

use std::fmt;

#[derive(Debug)]
pub enum SiteError {
    Invalid(String),
    NotFound(String),
    Io(String),
}

impl fmt::Display for SiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SiteError::Invalid(message) | SiteError::NotFound(message) | SiteError::Io(message) => {
                f.write_str(message)
            }
        }
    }
}

impl std::error::Error for SiteError {}

pub(crate) fn io_err(context: &str, err: impl fmt::Display) -> SiteError {
    SiteError::Io(format!("{context}: {err}"))
}
