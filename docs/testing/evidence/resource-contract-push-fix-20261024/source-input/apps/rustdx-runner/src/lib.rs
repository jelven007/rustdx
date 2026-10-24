//! Synchronous single-host control plane. Workers never receive database credentials.
//! Publication fixes immutable content before a transactional catalog compare-and-swap.

pub mod config;
pub mod consumer;
pub mod database;
mod database_process;
pub mod ledger;
pub mod model;
pub mod ops;
pub mod pipeline;
pub mod storage;
pub mod supervisor;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("CSV: {0}")]
    Csv(#[from] csv::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("catalog changed; reevaluate against version {0}")]
    Conflict(u64),
    #[error("operation cancelled")]
    Cancelled,
    #[error("operation deadline exhausted")]
    TimedOut,
    #[error("{stage}: {source}")]
    Stage {
        stage: &'static str,
        source: Box<Error>,
    },
}

impl Error {
    pub fn at(self, stage: &'static str) -> Self {
        Self::Stage {
            stage,
            source: Box::new(self),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        match self {
            Self::Cancelled => true,
            Self::Stage { source, .. } => source.is_cancelled(),
            _ => false,
        }
    }

    pub fn is_timed_out(&self) -> bool {
        match self {
            Self::TimedOut => true,
            Self::Stage { source, .. } => source.is_timed_out(),
            _ => false,
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timeout",
            Self::Io(_) => "io",
            Self::Json(_) => "json",
            Self::Sqlite(_) => "sqlite",
            Self::Csv(_) => "csv",
            Self::Invalid(_) => "validation",
            Self::Conflict(_) => "catalog_conflict",
            Self::Stage { source, .. } => source.category(),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        // CheckedIo wraps terminal Interrupted to escape std's retry loops.
        let mut source = &error;
        while let Some(inner) = source
            .get_ref()
            .and_then(|e| e.downcast_ref::<std::io::Error>())
        {
            source = inner;
        }
        match source.kind() {
            std::io::ErrorKind::Interrupted => Self::Cancelled,
            std::io::ErrorKind::TimedOut => Self::TimedOut,
            _ => Self::Io(error),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn cancellation_and_deadline_keep_their_category_through_stages_and_io_wrappers() {
        let cancelled = Error::from(std::io::Error::other(std::io::Error::from(
            std::io::ErrorKind::Interrupted,
        )))
        .at("copy");
        assert!(cancelled.is_cancelled());
        assert!(!cancelled.is_timed_out());
        assert_eq!(cancelled.category(), "cancelled");
        let timeout = Error::from(std::io::Error::from(std::io::ErrorKind::TimedOut)).at("worker");
        assert!(!timeout.is_cancelled());
        assert!(timeout.is_timed_out());
        assert_eq!(timeout.category(), "timeout");
    }
}
