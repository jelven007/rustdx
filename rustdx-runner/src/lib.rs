//! Synchronous single-host control plane. Workers never receive database credentials.
//! Publication fixes immutable content before a transactional catalog compare-and-swap.

pub mod config;
pub mod consumer;
pub mod database;
pub mod ledger;
pub mod model;
pub mod ops;
pub mod pipeline;
pub mod storage;
pub mod supervisor;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
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
    #[error("operation cancelled or deadline exhausted")]
    Cancelled,
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
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
