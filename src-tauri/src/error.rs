use serde::{Serialize, Serializer};

/// Every fallible path in the app funnels through this type.
///
/// Tauri commands must return something serializable, so `Serialize` flattens the
/// error to its `Display` string — the frontend shows that text directly.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("audio error: {0}")]
    Audio(String),

    #[error("keychain error: {0}")]
    Keychain(String),

    /// A provider is selected but unusable — missing key, bad base URL, no model.
    #[error("{0}")]
    Config(String),

    /// The upstream model provider returned a non-success response.
    #[error("{provider} returned {status}: {body}")]
    Provider {
        provider: String,
        status: u16,
        body: String,
    },

    #[error("not found: {0}")]
    NotFound(String),

    #[error("{0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Other(e.to_string())
    }
}

impl From<hound::Error> for AppError {
    fn from(e: hound::Error) -> Self {
        AppError::Audio(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
