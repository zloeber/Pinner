use std::path::{Path, PathBuf};

use pinner_ecosystem::{EvidenceKind, EcosystemKind, Pin};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::CoreError;

const LOCK_VERSION: u32 = 1;
const ENTRY_SOURCE: &str = "manifest";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockFile {
    pub version: u32,
    pub generated_at: String,
    pub pinner_version: String,
    pub entries: Vec<LockEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    pub ecosystem: EcosystemKind,
    pub name: String,
    pub requested: String,
    pub pinned: String,
    pub source: String,
    pub path: PathBuf,
    pub evidence: EvidenceKind,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl LockFile {
    pub fn from_pins(pins: &[Pin], pinner_version: &str, generated_at: &str) -> Self {
        Self {
            version: LOCK_VERSION,
            generated_at: generated_at.to_string(),
            pinner_version: pinner_version.to_string(),
            entries: pins.iter().map(LockEntry::from_pin).collect(),
        }
    }

    pub fn read(path: &Path) -> Result<Self, CoreError> {
        let contents = std::fs::read_to_string(path)?;
        let lock: Self = serde_json::from_str(&contents)?;
        if lock.version != LOCK_VERSION {
            return Err(CoreError::UnsupportedVersion(lock.version));
        }
        Ok(lock)
    }

    pub fn write(&self, path: &Path) -> Result<(), CoreError> {
        let mut lock = self.clone();
        lock.entries.sort_by(|a, b| {
            (
                a.ecosystem.as_str(),
                a.path.as_path(),
                a.name.as_str(),
            )
                .cmp(&(
                    b.ecosystem.as_str(),
                    b.path.as_path(),
                    b.name.as_str(),
                ))
        });
        let bytes = serde_json::to_vec_pretty(&lock)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

impl LockEntry {
    fn from_pin(pin: &Pin) -> Self {
        Self {
            ecosystem: pin.ecosystem,
            name: pin.name.clone(),
            requested: pin.requested.clone(),
            pinned: pin.pinned.clone(),
            source: ENTRY_SOURCE.to_string(),
            path: pin.path.clone(),
            evidence: pin.evidence,
            metadata: pin.metadata.clone(),
        }
    }
}
