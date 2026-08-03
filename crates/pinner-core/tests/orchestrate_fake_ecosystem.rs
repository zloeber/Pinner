use std::path::Path;
use std::sync::Arc;

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
                if let Some(p) = ctx.lock_pins.iter().find(|p| p.name == f.name) {
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
