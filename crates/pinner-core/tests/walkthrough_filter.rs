use std::path::{Path, PathBuf};
use std::sync::Arc;

use pinner_core::{
    CoreError, PinDecision, Policy, RunOptions, WalkthroughOutcome, apply_walkthrough_decisions,
    pin_with_filter,
};
use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Manifest, Pin,
    Rewrite,
};
use serde_json::json;
use tempfile::tempdir;

fn sample_pin(name: &str, pinned: &str) -> Pin {
    Pin {
        ecosystem: EcosystemKind::Mise,
        name: name.into(),
        requested: "latest".into(),
        pinned: pinned.into(),
        path: PathBuf::from(".mise.toml"),
        evidence: EvidenceKind::Tool,
        metadata: Default::default(),
    }
}

#[test]
fn skip_removes_pin_edit_sets_metadata() {
    let pins = vec![
        sample_pin("node", "22.11.0"),
        sample_pin("python", "3.12.0"),
        sample_pin("go", "1.23.0"),
    ];
    let decisions = vec![
        PinDecision::Accept,
        PinDecision::Skip,
        PinDecision::Edit {
            pinned: "1.22.5".into(),
        },
    ];

    let outcome = apply_walkthrough_decisions(&pins, &decisions).unwrap();
    let WalkthroughOutcome::Continue { pins: filtered } = outcome else {
        panic!("expected Continue");
    };

    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].name, "node");
    assert_eq!(filtered[0].pinned, "22.11.0");
    assert!(filtered[0].metadata.get("user_override").is_none());

    assert_eq!(filtered[1].name, "go");
    assert_eq!(filtered[1].pinned, "1.22.5");
    assert_eq!(
        filtered[1].metadata.get("user_override"),
        Some(&json!(true))
    );
}

#[test]
fn decision_length_mismatch_is_error() {
    let pins = vec![sample_pin("node", "22.11.0")];
    let err = apply_walkthrough_decisions(&pins, &[]).unwrap_err();
    assert!(matches!(
        err,
        CoreError::WalkthroughLengthMismatch {
            pins: 1,
            decisions: 0
        }
    ));
}

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
        continue_on_ecosystem_error: false,
        ecosystems_filter: None,
    }
}

#[test]
fn abort_outcome_when_signaled_by_caller() {
    let dir = tempdir().unwrap();
    let manifest = dir.path().join(".mise.toml");
    let original = "[tools]\nnode = \"latest\"\n";
    std::fs::write(&manifest, original).unwrap();
    let eco: Arc<dyn Ecosystem> = Arc::new(FakeEco);

    let mut walkthrough = |_pins: &[Pin]| -> Result<WalkthroughOutcome, CoreError> {
        Ok(WalkthroughOutcome::Aborted)
    };

    let report = pin_with_filter(
        &[eco],
        &Policy::default_policy(),
        &options(dir.path()),
        Some(&mut walkthrough),
    )
    .unwrap();

    assert!(report.pins.is_empty());
    assert!(report.rewrites.is_empty());
    assert_eq!(std::fs::read_to_string(&manifest).unwrap(), original);
    assert!(!dir.path().join("pinner.lock.json").exists());
}

#[test]
fn edit_decision_rewrites_with_override_and_sets_metadata() {
    let dir = tempdir().unwrap();
    let manifest = dir.path().join(".mise.toml");
    std::fs::write(&manifest, "[tools]\nnode = \"latest\"\n").unwrap();
    let eco: Arc<dyn Ecosystem> = Arc::new(FakeEco);

    let mut walkthrough = |pins: &[Pin]| -> Result<WalkthroughOutcome, CoreError> {
        let decisions: Vec<_> = pins
            .iter()
            .map(|_| PinDecision::Edit {
                pinned: "22.12.0".into(),
            })
            .collect();
        apply_walkthrough_decisions(pins, &decisions)
    };

    let report = pin_with_filter(
        &[eco],
        &Policy::default_policy(),
        &options(dir.path()),
        Some(&mut walkthrough),
    )
    .unwrap();

    assert_eq!(report.pins.len(), 1);
    assert_eq!(report.pins[0].pinned, "22.12.0");
    assert_eq!(
        report.pins[0].metadata.get("user_override"),
        Some(&json!(true))
    );
    let body = std::fs::read_to_string(&manifest).unwrap();
    assert!(body.contains("22.12.0"));
    assert!(dir.path().join("pinner.lock.json").exists());
}
