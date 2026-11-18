use std::fmt;

/// Canonical result type for PHPX subsystems.
pub type Result<T> = std::result::Result<T, PhpxError>;

/// Error conventions for M0: stable, actionable, no secrets in messages.
#[derive(Debug, thiserror::Error)]
pub enum PhpxError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("prerequisite missing: {0}")]
    Prerequisite(String),

    #[error("runtime error: {0}")]
    Runtime(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}

impl From<std::io::Error> for PhpxError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

/// Application identity, shared across subsystems.
#[derive(Debug, Clone)]
pub struct AppIdentity {
    pub name: String,
    pub version: String,
}

impl AppIdentity {
    pub fn phpx() -> Self {
        Self {
            name: "phpx".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl fmt::Display for AppIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.name, self.version)
    }
}

/// Initialize structured logging. JSON when `json` is true, pretty otherwise.
pub fn init_logging(json: bool) {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,phpx=debug".to_string());
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false);
    if json {
        let _ = subscriber.json().try_init();
    } else {
        let _ = subscriber.try_init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_are_actionable() {
        let err = PhpxError::Config("schema_version must be 1".to_string());
        assert!(err.to_string().contains("schema_version"));
    }

    #[test]
    fn app_identity_displays() {
        let id = AppIdentity::phpx();
        assert_eq!(id.name, "phpx");
        assert!(!id.version.is_empty());
    }
}
