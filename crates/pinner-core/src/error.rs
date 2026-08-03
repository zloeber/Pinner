use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("pinner.lock.json is required for check")]
    MissingLock,
    #[error("nothing to explain for target: {0}")]
    ExplainTargetNotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ecosystem error: {0}")]
    Ecosystem(#[from] pinner_ecosystem::EcosystemError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported lockfile version {0}; expected version 1")]
    UnsupportedVersion(u32),
    #[error("toml error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("glob error: {0}")]
    Glob(#[from] globset::Error),
}
