use pinner_ecosystem::EcosystemKind;

#[derive(Debug, Clone)]
pub enum AuditPhase {
    Discover,
    Extract,
}

#[derive(Debug, Clone)]
pub enum AuditEvent {
    AuditStarted {
        ecosystems: Vec<EcosystemKind>,
    },
    EcosystemStarted {
        kind: EcosystemKind,
    },
    EcosystemPhase {
        kind: EcosystemKind,
        phase: AuditPhase,
    },
    EcosystemFinished {
        kind: EcosystemKind,
        manifests: usize,
        floating: usize,
    },
    EcosystemFailed {
        kind: EcosystemKind,
        error: String,
    },
    AuditFinished {
        findings: usize,
    },
}

pub trait AuditProgress: Send + Sync {
    fn on_event(&self, event: AuditEvent);
}
