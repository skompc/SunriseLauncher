use serde::Serializer;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("{context}: {source}")]
    Io {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("GitHub could not be reached. Check your connection and try again: {0}")]
    Network(#[from] reqwest::Error),
    #[error("A downloaded archive could not be read: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("Local installer data is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("The operation was cancelled.")]
    Cancelled,
}

impl AppError {
    pub fn message(value: impl Into<String>) -> Self {
        Self::Message(value.into())
    }

    pub fn io(context: &'static str, source: std::io::Error) -> Self {
        Self::Io { context, source }
    }
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
