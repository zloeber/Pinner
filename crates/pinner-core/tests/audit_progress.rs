use pinner_core::{AuditEvent, AuditPhase, AuditProgress, Policy, RunOptions, audit};
use pinner_ecosystem::{Ecosystem, EcosystemKind, Finding, Manifest};
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
