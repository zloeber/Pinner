use std::path::Path;
use std::sync::Arc;

use pinner_core::lock::LockFile;
use pinner_core::orchestrate::{RunOptions, check, pin, upgrade};
use pinner_core::policy::Policy;
use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Manifest, Pin,
    ResolveMode, Rewrite,
};
use serde_json::{Map, Value};
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
                if let Some(p) = ctx
                    .lock_pins
                    .iter()
                    .find(|p| p.ecosystem == f.ecosystem && p.name == f.name && p.path == f.path)
                {
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
                if let Some(p) = ctx
                    .lock_pins
                    .iter()
                    .find(|p| p.ecosystem == f.ecosystem && p.name == f.name && p.path == f.path)
                {
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
    let first_bytes = std::fs::read(&lock_path).unwrap();
    let first = LockFile::read(&lock_path).unwrap();
    assert_eq!(first.entries.len(), 1);
    assert_eq!(first.entries[0].pinned, "22.11.0");
    assert_eq!(first.entries[0].requested, "22.11.0");

    let clean = check(&[Arc::clone(&eco)], &policy, &opts).unwrap();
    assert!(clean.drift.is_empty());

    let toml_before = std::fs::read(dir.path().join(".mise.toml")).unwrap();
    pin(&[eco], &policy, &opts).unwrap();
    let second_bytes = std::fs::read(&lock_path).unwrap();
    let toml_after = std::fs::read(dir.path().join(".mise.toml")).unwrap();
    assert_eq!(
        second_bytes, first_bytes,
        "second pin must not change lock bytes"
    );
    assert_eq!(
        toml_after, toml_before,
        "second pin must not change manifest bytes"
    );
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
    assert!(
        first
            .entries
            .iter()
            .any(|e| e.ecosystem == EcosystemKind::Node)
    );
    assert!(
        first
            .entries
            .iter()
            .any(|e| e.ecosystem == EcosystemKind::Mise)
    );

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

#[test]
fn lock_paths_are_repo_relative_and_portable_across_copy() {
    let a = tempdir().unwrap();
    std::fs::write(a.path().join(".mise.toml"), "[tools]\nnode = \"latest\"\n").unwrap();
    let eco: Arc<dyn Ecosystem> = Arc::new(FakeEco);
    let policy = Policy::default_policy();

    pin(&[Arc::clone(&eco)], &policy, &options(a.path())).unwrap();

    let lock_raw = std::fs::read_to_string(a.path().join("pinner.lock.json")).unwrap();
    assert!(
        !lock_raw.contains(a.path().to_string_lossy().as_ref()),
        "lock must not embed absolute temp path: {lock_raw}"
    );
    let lock = LockFile::read(&a.path().join("pinner.lock.json")).unwrap();
    for entry in &lock.entries {
        assert!(
            entry.path.is_relative(),
            "expected relative lock path, got {}",
            entry.path.display()
        );
    }

    let b = tempdir().unwrap();
    for name in [".mise.toml", "pinner.lock.json"] {
        std::fs::copy(a.path().join(name), b.path().join(name)).unwrap();
    }

    let report = check(&[eco], &policy, &options(b.path())).unwrap();
    assert!(
        report.drift.is_empty(),
        "check after copy should be clean, drift={:?}",
        report.drift
    );
}

struct FailNodeEco;

impl Ecosystem for FailNodeEco {
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
        Ok(vec![Finding {
            ecosystem: EcosystemKind::Node,
            name: "left-pad".into(),
            requested: "^1.0.0".into(),
            path: manifest.path.clone(),
            is_floating: true,
        }])
    }

    fn resolve(
        &self,
        _findings: &[Finding],
        _ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        Err(EcosystemError::Resolve {
            name: "left-pad".into(),
            requested: "^1.0.0".into(),
            hint: "intentional failure for atomicity test".into(),
        })
    }

    fn rewrite(
        &self,
        _manifest: &Manifest,
        _pins: &[Pin],
    ) -> Result<Option<Rewrite>, EcosystemError> {
        unreachable!("rewrite must not run when resolve fails")
    }
}

#[test]
fn pin_writes_nothing_when_later_ecosystem_resolve_fails() {
    let dir = tempdir().unwrap();
    let mise_path = dir.path().join(".mise.toml");
    let original = "[tools]\nnode = \"latest\"\n";
    std::fs::write(&mise_path, original).unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        "{\n  \"dependencies\": {\n    \"left-pad\": \"^1.0.0\"\n  }\n}\n",
    )
    .unwrap();

    let mise: Arc<dyn Ecosystem> = Arc::new(FakeEco);
    let node: Arc<dyn Ecosystem> = Arc::new(FailNodeEco);
    let err = pin(
        &[mise, node],
        &Policy::default_policy(),
        &options(dir.path()),
    )
    .unwrap_err();
    assert!(matches!(err, pinner_core::CoreError::Ecosystem(_)));

    let body = std::fs::read_to_string(&mise_path).unwrap();
    assert_eq!(
        body, original,
        "manifest must be untouched on resolve failure"
    );
    assert!(
        !dir.path().join("pinner.lock.json").exists(),
        "lock must not be written on resolve failure"
    );
}

/// Exact-pin fixture: upgrade resolves to a newer version; pin ignores exact findings.
struct FakeExactUpgradeEco;

impl Ecosystem for FakeExactUpgradeEco {
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
        let requested = if text.contains("2.0.0") {
            "2.0.0"
        } else {
            "1.0.0"
        };
        Ok(vec![Finding {
            ecosystem: EcosystemKind::Mise,
            name: "tool".into(),
            requested: requested.into(),
            path: manifest.path.clone(),
            is_floating: false,
        }])
    }

    fn resolve(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        if ctx.resolve_mode != ResolveMode::Upgrade {
            return Ok(Vec::new());
        }
        findings
            .iter()
            .filter_map(|f| {
                // Display-only previous: prefer prior lock pin, else requested.
                // Selection always uses the map/newer value (never lock.pinned).
                let previous = ctx
                    .lock_pins
                    .iter()
                    .find(|p| p.ecosystem == f.ecosystem && p.name == f.name && p.path == f.path)
                    .map(|p| p.pinned.as_str())
                    .unwrap_or(f.requested.as_str());
                let newest = "2.0.0";
                if previous == newest {
                    return None;
                }
                let mut metadata = Map::new();
                metadata.insert("upgrade".into(), Value::Bool(true));
                metadata.insert("previous".into(), Value::String(previous.to_string()));
                metadata.insert("upgrade_channel".into(), Value::String("map".into()));
                Some(Ok(Pin {
                    ecosystem: f.ecosystem,
                    name: f.name.clone(),
                    requested: f.requested.clone(),
                    pinned: newest.into(),
                    path: f.path.clone(),
                    evidence: EvidenceKind::Registry,
                    metadata,
                }))
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
            new_contents: format!("[tools]\ntool = \"{}\"\n", pin.pinned),
        }))
    }
}

#[test]
fn upgrade_rewrites_exact_pins_pin_does_not() {
    let dir = tempdir().unwrap();
    let manifest = dir.path().join(".mise.toml");
    std::fs::write(&manifest, "[tools]\ntool = \"1.0.0\"\n").unwrap();
    let eco: Arc<dyn Ecosystem> = Arc::new(FakeExactUpgradeEco);
    let policy = Policy::default_policy();
    let opts = options(dir.path());

    let report = upgrade(&[Arc::clone(&eco)], &policy, &opts).unwrap();
    assert!(
        report.upgraded >= 1,
        "upgrade must count at least one bump, got {}",
        report.upgraded
    );
    let body = std::fs::read_to_string(&manifest).unwrap();
    assert!(
        body.contains("2.0.0"),
        "upgrade must rewrite exact pin, body={body}"
    );

    std::fs::write(&manifest, "[tools]\ntool = \"1.0.0\"\n").unwrap();
    pin(&[eco], &policy, &opts).unwrap();
    let body = std::fs::read_to_string(&manifest).unwrap();
    assert!(
        body.contains("1.0.0") && !body.contains("2.0.0"),
        "pin must leave exact pins alone, body={body}"
    );
}

#[test]
fn upgrade_sets_previous_from_prior_lock_while_choosing_newer() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(".mise.toml"), "[tools]\ntool = \"1.0.0\"\n").unwrap();
    LockFile::from_pins(
        &[Pin {
            ecosystem: EcosystemKind::Mise,
            name: "tool".into(),
            requested: "1.0.0".into(),
            pinned: "1.5.0".into(),
            path: Path::new(".mise.toml").to_path_buf(),
            evidence: EvidenceKind::Tool,
            metadata: Default::default(),
        }],
        "0.1.0",
        "2026-08-05T00:00:00Z",
    )
    .write(&dir.path().join("pinner.lock.json"))
    .unwrap();

    let eco: Arc<dyn Ecosystem> = Arc::new(FakeExactUpgradeEco);
    let mut opts = options(dir.path());
    opts.dry_run = true;
    let report = upgrade(&[eco], &Policy::default_policy(), &opts).unwrap();

    assert_eq!(report.upgraded, 1);
    let bump = report
        .pins
        .iter()
        .find(|p| p.metadata.get("previous").is_some())
        .expect("upgrade pin carries previous metadata");
    assert_eq!(
        bump.pinned, "2.0.0",
        "must choose map/newer, not lock 1.5.0"
    );
    assert_eq!(
        bump.metadata["previous"], "1.5.0",
        "previous must peek prior lock, not only requested"
    );
    assert!(
        report.rewrites[0].new_contents.contains("2.0.0"),
        "rewrite must apply newer value"
    );
}
