use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub jwt_secret: String,
    pub storage_dir: PathBuf,
}

impl AppState {
    pub fn new(jwt_secret: String, storage_dir: impl AsRef<Path>) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                jwt_secret,
                storage_dir: storage_dir.as_ref().to_path_buf(),
            }),
        }
    }

    /// Validates and sanitizes a calendar identifier to prevent directory traversal attacks.
    /// Only allows alphanumeric characters, hyphens, and underscores.
    pub fn sanitize_identifier(identifier: &str) -> Result<String, &'static str> {
        let trimmed = identifier.trim();
        if trimmed.is_empty() {
            return Err("Identifier cannot be empty");
        }
        if trimmed.len() > 128 {
            return Err("Identifier exceeds maximum length of 128 characters");
        }
        if !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err("Identifier may only contain alphanumeric characters, hyphens, and underscores");
        }
        Ok(trimmed.to_string())
    }

    /// Resolves the absolute filesystem path for a calendar file.
    pub fn calendar_path(&self, identifier: &str) -> Result<PathBuf, &'static str> {
        let sanitized = Self::sanitize_identifier(identifier)?;
        Ok(self.inner.storage_dir.join(format!("{}.ics", sanitized)))
    }
}
