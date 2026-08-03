use std::path::Path;
use std::sync::Arc;

use pinner_core::lock::LockFile;
use pinner_core::orchestrate::{RunOptions, check, pin};
use pinner_core::policy::Policy;
use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Manifest, Pin,
    Rewrite,
};
use tempfile::tempdir;

struct FakeEco;

impl Ecosystem for FakeEco {
    fn kind(&self) -> EcosystemKind {
        EcosystemKind::Mise
    }

    fn discover(&self, repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
        Ok(vec![Manifest {
            ecosystem: self.kind(),
            path: repo.join(".mise.toml"),
        }])
    }

    fn extract(
        &self,
        manifest: &Manifest,
        _ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, EcosystemError> {
        let text = std::fs::read_to_string(&manifest.path).unwrap();
        let floating = text.contains("latest");
        Ok(vec![Finding {
            ecosystem: EcosystemKind::Mise,
            name: "node".into(),
            requested: if floating {
                "latest".into()
            } else {
                "22.11.0".into()
            },
            path: manifest.path.clone(),
            is_floating: floating,
        }])
    }

    fn resolve(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        findings
            .iter()
            .map(|f| {
                if let Some(p) = ctx.lock_pins.iter().find(|p| {
                    p.ecosystem == f.ecosystem && p.name == f.name && p.path == f.path
                }) {
                    return Ok(p.clone());
                }
                if ctx.offline && f.is_floating {
                    return Err(EcosystemError::Offline {
                        name: f.name.clone(),
                        requested: f.requested.clone(),
                    });
                }
                Ok(Pin {
                    ecosystem: f.ecosystem,
                    name: f.name.clone(),
                    requested: f.requested.clone(),
                    pinned: "22.11.0".into(),
                    path: f.path.clone(),
                    evidence: EvidenceKind::Tool,
                    metadata: Default::default(),
                })
            })
            .collect()
    }

    fn rewrite(
        &self,
        manifest: &Manifest,
        pins: &[Pin],
    ) -> Result<Option<Rewrite>, EcosystemError> {
        let pin = &pins[0];
        Ok(Some(Rewrite {
            path: manifest.path.clone(),
            new_contents: format!("[tools]\nnode = \"{}\"\n", pin.pinned),
        }))
    }
}

struct FakeNodeEco;

impl Ecosystem for FakeNodeEco {
    fn kind(&self) -> EcosystemKind {
        EcosystemKind::Node
    }

    fn discover(&self, repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
        Ok(vec![Manifest {
            ecosystem: self.kind(),
            path: repo.join("package.json"),
        }])
    }

    fn extract(
        &self,
        manifest: &Manifest,
        _ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, EcosystemError> {
        let text = std::fs::read_to_string(&manifest.path).unwrap();
        let floating = text.contains("\"^\"");
        Ok(vec![Finding {
            ecosystem: EcosystemKind::Node,
            name: "left-pad".into(),
            requested: if floating {
                "^1.0.0".into()
            } else {
                "1.3.0".into()
            },
            path: manifest.path.clone(),
            is_floating: floating,
        }])
    }

    fn resolve(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        findings
            .iter()
            .map(|f| {
                if let Some(p) = ctx.lock_pins.iter().find(|p| {
                    p.ecosystem == f.ecosystem && p.name == f.name && p.path == f.path
                }) {
                    return Ok(p.clone());
                }
                if ctx.offline && f.is_floating {
                    return Err(EcosystemError::Offline {
                        name: f.name.clone(),
                        requested: f.requested.clone(),
                    });
                }
                Ok(Pin {
                    ecosystem: f.ecosystem,
                    name: f.name.clone(),
                    requested: f.requested.clone(),
                    pinned: "1.3.0".into(),
                    path: f.path.clone(),
                    evidence: EvidenceKind::Tool,
                    metadata: Default::default(),
                })
            })
            .collect()
    }

    fn rewrite(
        &self,
        manifest: &Manifest,
        pins: &[Pin],
    ) -> Result<Option<Rewrite>, EcosystemError> {
        let pin = &pins[0];
        Ok(Some(Rewrite {
            path: manifest.path.clone(),
            new_contents: format!(
                "{{\n  \"dependencies\": {{\n    \"{}\": \"{}\"\n  }}\n}}\n",
                pin.name, pin.pinned
            ),
        }))
    }
}

fn options(repo: &Path) -> RunOptions {
    RunOptions {
        repo: repo.to_path_buf(),
        dry_run: false,
        offline: false,
        ecosystems_filter: None,
    }
}

#[test]
fn pin_rewrites_and_writes_lock() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join(".mise.toml"),
        "[tools]\nnode = \"latest\"\n",
    )
    .unwrap();
    let eco: Arc<dyn Ecosystem> = Arc::new(FakeEco);

    let report = pin(&[eco], &Policy::default_policy(), &options(dir.path())).unwrap();

    assert_eq!(report.pins[0].pinned, "22.11.0");
    assert!(dir.path().join("pinner.lock.json").exists());
    let body = std::fs::read_to_string(dir.path().join(".mise.toml")).unwrap();
    assert!(body.contains("22.11.0"));
}

#[test]
fn pin_twice_preserves_lock_and_stays_clean() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join(".mise.toml"),
        "[tools]\nnode = \"latest\"\n",
    )
    .unwrap();
    let eco: Arc<dyn Ecosystem> = Arc::new(FakeEco);
    let policy = Policy::default_policy();
    let opts = options(dir.path());

    pin(&[Arc::clone(&eco)], &policy, &opts).unwrap();
    let lock_path = dir.path().join("pinner.lock.json");
    let first = LockFile::read(&lock_path).unwrap();
    assert_eq!(first.entries.len(), 1);
    assert_eq!(first.entries[0].pinned, "22.11.0");

    let clean = check(&[Arc::clone(&eco)], &policy, &opts).unwrap();
    assert!(clean.drift.is_empty());

    pin(&[eco], &policy, &opts).unwrap();
    let second = LockFile::read(&lock_path).unwrap();
    assert_eq!(second.entries.len(), first.entries.len());
    assert_eq!(second.entries[0].pinned, first.entries[0].pinned);
    assert_eq!(second.entries[0].name, first.entries[0].name);
}

#[test]
fn check_reports_drift_after_pinned_manifest_becomes_floating() {
    let dir = tempdir().unwrap();
    let manifest = dir.path().join(".mise.toml");
    std::fs::write(&manifest, "[tools]\nnode = \"latest\"\n").unwrap();
    let eco: Arc<dyn Ecosystem> = Arc::new(FakeEco);

    pin(
        &[Arc::clone(&eco)],
        &Policy::default_policy(),
        &options(dir.path()),
    )
    .unwrap();
    std::fs::write(&manifest, "[tools]\nnode = \"latest\"\n").unwrap();

    let report = check(&[eco], &Policy::default_policy(), &options(dir.path())).unwrap();

    assert!(!report.drift.is_empty());
}

#[test]
fn pin_with_ecosystem_filter_preserves_unselected_lock_entries() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join(".mise.toml"),
        "[tools]\nnode = \"latest\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        "{\n  \"dependencies\": {\n    \"left-pad\": \"^1.0.0\"\n  }\n}\n",
    )
    .unwrap();

    let mise: Arc<dyn Ecosystem> = Arc::new(FakeEco);
    let node: Arc<dyn Ecosystem> = Arc::new(FakeNodeEco);
    let ecosystems = [Arc::clone(&mise), Arc::clone(&node)];
    let policy = Policy::default_policy();

    pin(&ecosystems, &policy, &options(dir.path())).unwrap();
    let lock_path = dir.path().join("pinner.lock.json");
    let first = LockFile::read(&lock_path).unwrap();
    assert_eq!(first.entries.len(), 2);
    assert!(first.entries.iter().any(|e| e.ecosystem == EcosystemKind::Node));
    assert!(first.entries.iter().any(|e| e.ecosystem == EcosystemKind::Mise));

    let node_before = first
        .entries
        .iter()
        .find(|e| e.ecosystem == EcosystemKind::Node)
        .unwrap()
        .clone();

    let filtered = RunOptions {
        ecosystems_filter: Some(vec![EcosystemKind::Mise]),
        ..options(dir.path())
    };
    pin(&ecosystems, &policy, &filtered).unwrap();

    let second = LockFile::read(&lock_path).unwrap();
    assert_eq!(second.entries.len(), 2);
    let node_after = second
        .entries
        .iter()
        .find(|e| e.ecosystem == EcosystemKind::Node)
        .expect("Node lock entry must remain after Mise-only pin");
    assert_eq!(node_after.pinned, node_before.pinned);
    assert_eq!(node_after.name, node_before.name);
    assert_eq!(node_after.path, node_before.path);
}
