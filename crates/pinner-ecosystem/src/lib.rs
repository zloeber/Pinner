use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EcosystemKind {
    Mise,
    Node,
    Python,
    Docker,
    Actions,
}

impl EcosystemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mise => "mise",
            Self::Node => "node",
            Self::Python => "python",
            Self::Docker => "docker",
            Self::Actions => "actions",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub ecosystem: EcosystemKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub ecosystem: EcosystemKind,
    pub name: String,
    pub requested: String,
    pub path: PathBuf,
    pub is_floating: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Lock,
    NativeLock,
    Registry,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub ecosystem: EcosystemKind,
    pub name: String,
    pub requested: String,
    pub pinned: String,
    pub path: PathBuf,
    pub evidence: EvidenceKind,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rewrite {
    pub path: PathBuf,
    pub new_contents: String,
}

#[derive(Debug, Clone)]
pub struct EcosystemCtx<'a> {
    pub repo: &'a Path,
    pub lock_pins: &'a [Pin],
    pub offline: bool,
    pub pin_exact_ranges: bool,
}

/// Resolve a manifest/finding path for I/O: absolute paths are used as-is;
/// repo-relative paths are joined onto `repo`.
pub fn absolute_in_repo(repo: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}

/// Strip `repo` prefix when present so lock/finding/pin paths stay portable.
pub fn repo_relative(repo: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(repo)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Debug, Error)]
pub enum EcosystemError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error in {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("resolve error for {name} ({requested}): {hint}")]
    Resolve {
        name: String,
        requested: String,
        hint: String,
    },
    #[error("offline: cannot resolve {name} ({requested})")]
    Offline { name: String, requested: String },
}

pub trait Ecosystem: Send + Sync {
    fn kind(&self) -> EcosystemKind;
    fn discover(&self, repo: &Path) -> Result<Vec<Manifest>, EcosystemError>;
    fn extract(
        &self,
        manifest: &Manifest,
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, EcosystemError>;
    fn resolve(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError>;
    fn rewrite(&self, manifest: &Manifest, pins: &[Pin])
    -> Result<Option<Rewrite>, EcosystemError>;
}
