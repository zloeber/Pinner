use pinner_core::{AuditEvent, AuditPhase, AuditProgress, Policy, RunOptions, audit};
use pinner_ecosystem::{Ecosystem, EcosystemError, EcosystemKind, Finding, Manifest};
use std::sync::{Arc, Mutex};

struct RecordingSink {
    events: Mutex<Vec<AuditEvent>>,
}

impl AuditProgress for RecordingSink {
    fn on_event(&self, event: AuditEvent) {
        self.events.lock().unwrap().push(event);
    }
}

/// StubEco: discover one .mise.toml, extract one floating finding.
struct StubEco;

impl Ecosystem for StubEco {
    fn kind(&self) -> EcosystemKind {
        EcosystemKind::Mise
    }

    fn discover(
        &self,
        repo: &std::path::Path,
    ) -> Result<Vec<Manifest>, pinner_ecosystem::EcosystemError> {
        Ok(vec![Manifest {
            ecosystem: self.kind(),
            path: repo.join(".mise.toml"),
        }])
    }

    fn extract(
        &self,
        manifest: &Manifest,
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, pinner_ecosystem::EcosystemError> {
        Ok(vec![Finding {
            ecosystem: self.kind(),
            name: "node".into(),
            requested: "latest".into(),
            path: manifest.path.clone(),
            is_floating: true,
        }])
    }

    fn resolve(
        &self,
        _findings: &[Finding],
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<pinner_ecosystem::Pin>, pinner_ecosystem::EcosystemError> {
        Ok(vec![])
    }

    fn rewrite(
        &self,
        _manifest: &Manifest,
        _pins: &[pinner_ecosystem::Pin],
    ) -> Result<Option<pinner_ecosystem::Rewrite>, pinner_ecosystem::EcosystemError> {
        Ok(None)
    }
}

#[test]
fn audit_emits_phase_events() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".mise.toml"), "[tools]\nnode=\"latest\"\n").unwrap();
    let sink = RecordingSink {
        events: Mutex::new(Vec::new()),
    };
    let ecosystems: Vec<Arc<dyn Ecosystem>> = vec![Arc::new(StubEco)];
    let policy = Policy::default_policy();
    let opts = RunOptions {
        repo: dir.path().to_path_buf(),
        dry_run: true,
        offline: true,
        ecosystems_filter: Some(vec![EcosystemKind::Mise]),
    };
    let report = audit(&ecosystems, &policy, &opts, Some(&sink)).unwrap();
    assert_eq!(report.findings.len(), 1);
    let events = sink.events.lock().unwrap().clone();
    assert!(matches!(
        events.first(),
        Some(AuditEvent::AuditStarted { .. })
    ));
    assert!(events.iter().any(|e| {
        matches!(
            e,
            AuditEvent::EcosystemPhase {
                phase: AuditPhase::Discover,
                ..
            }
        )
    }));
    assert!(events.iter().any(|e| {
        matches!(
            e,
            AuditEvent::EcosystemPhase {
                phase: AuditPhase::Extract,
                ..
            }
        )
    }));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AuditEvent::EcosystemFinished { floating: 1, .. }))
    );
    assert!(matches!(
        events.last(),
        Some(AuditEvent::AuditFinished { findings: 1 })
    ));
}

/// Stub that fails discover so audit can exercise the failure progress contract.
struct FailDiscoverEco;

impl Ecosystem for FailDiscoverEco {
    fn kind(&self) -> EcosystemKind {
        EcosystemKind::Mise
    }

    fn discover(&self, _repo: &std::path::Path) -> Result<Vec<Manifest>, EcosystemError> {
        Err(EcosystemError::Parse {
            path: std::path::PathBuf::from(".mise.toml"),
            message: "forced discover failure".into(),
        })
    }

    fn extract(
        &self,
        _manifest: &Manifest,
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, EcosystemError> {
        Ok(vec![])
    }

    fn resolve(
        &self,
        _findings: &[Finding],
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<pinner_ecosystem::Pin>, EcosystemError> {
        Ok(vec![])
    }

    fn rewrite(
        &self,
        _manifest: &Manifest,
        _pins: &[pinner_ecosystem::Pin],
    ) -> Result<Option<pinner_ecosystem::Rewrite>, EcosystemError> {
        Ok(None)
    }
}

#[test]
fn audit_emits_ecosystem_failed_without_finished() {
    let dir = tempfile::tempdir().unwrap();
    let sink = RecordingSink {
        events: Mutex::new(Vec::new()),
    };
    let ecosystems: Vec<Arc<dyn Ecosystem>> = vec![Arc::new(FailDiscoverEco)];
    let policy = Policy::default_policy();
    let opts = RunOptions {
        repo: dir.path().to_path_buf(),
        dry_run: true,
        offline: true,
        ecosystems_filter: Some(vec![EcosystemKind::Mise]),
    };
    let result = audit(&ecosystems, &policy, &opts, Some(&sink));
    assert!(result.is_err());
    let events = sink.events.lock().unwrap().clone();
    assert!(events.iter().any(|e| {
        matches!(
            e,
            AuditEvent::EcosystemFailed {
                kind: EcosystemKind::Mise,
                ..
            }
        )
    }));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AuditEvent::AuditFinished { .. }))
    );
}

struct MultiStub {
    kind: EcosystemKind,
    file: &'static str,
}

impl Ecosystem for MultiStub {
    fn kind(&self) -> EcosystemKind {
        self.kind
    }
    fn discover(
        &self,
        repo: &std::path::Path,
    ) -> Result<Vec<Manifest>, pinner_ecosystem::EcosystemError> {
        Ok(vec![Manifest {
            ecosystem: self.kind,
            path: repo.join(self.file),
        }])
    }
    fn extract(
        &self,
        manifest: &Manifest,
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, pinner_ecosystem::EcosystemError> {
        Ok(vec![Finding {
            ecosystem: self.kind,
            name: "dep".into(),
            requested: "latest".into(),
            path: manifest.path.clone(),
            is_floating: true,
        }])
    }
    fn resolve(
        &self,
        _: &[Finding],
        _: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<pinner_ecosystem::Pin>, pinner_ecosystem::EcosystemError> {
        Ok(vec![])
    }
    fn rewrite(
        &self,
        _: &Manifest,
        _: &[pinner_ecosystem::Pin],
    ) -> Result<Option<pinner_ecosystem::Rewrite>, pinner_ecosystem::EcosystemError> {
        Ok(None)
    }
}

#[test]
fn audit_findings_are_sorted_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.toml"), "").unwrap();
    std::fs::write(dir.path().join("a.toml"), "").unwrap();
    let ecosystems: Vec<Arc<dyn Ecosystem>> = vec![
        Arc::new(MultiStub {
            kind: EcosystemKind::Node,
            file: "b.toml",
        }),
        Arc::new(MultiStub {
            kind: EcosystemKind::Mise,
            file: "a.toml",
        }),
    ];
    let policy = Policy::default_policy();
    let opts = RunOptions {
        repo: dir.path().to_path_buf(),
        dry_run: true,
        offline: true,
        ecosystems_filter: Some(vec![EcosystemKind::Mise, EcosystemKind::Node]),
    };
    let report = audit(&ecosystems, &policy, &opts, None).unwrap();
    let keys: Vec<_> = report
        .findings
        .iter()
        .map(|f| {
            (
                f.ecosystem.as_str().to_string(),
                f.path.to_string_lossy().into_owned(),
                f.name.clone(),
            )
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
}
