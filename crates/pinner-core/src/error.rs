use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported lockfile version {0}; expected version 1")]
    UnsupportedVersion(u32),
    #[error("toml error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("glob error: {0}")]
    Glob(#[from] globset::Error),
}
