use std::io::{self, Write};

use crossterm::style::{Color, ResetColor, SetForegroundColor};
use pinner_core::{AuditEvent, AuditPhase, AuditProgress};

pub struct StderrAuditProgress {
    color: bool,
}

impl StderrAuditProgress {
    pub fn new(color: bool) -> Self {
        Self { color }
    }
}

pub fn format_audit_event(event: &AuditEvent) -> String {
    match event {
        AuditEvent::AuditStarted { ecosystems } => format!(
            "pinner audit · {} ecosystem{} · parallel",
            ecosystems.len(),
            if ecosystems.len() == 1 { "" } else { "s" }
        ),
        AuditEvent::EcosystemStarted { kind } => {
            format!("  … {:<12} starting", kind.as_str())
        }
        AuditEvent::EcosystemPhase { kind, phase } => {
            let phase = match phase {
                AuditPhase::Discover => "discover",
                AuditPhase::Extract => "extract",
            };
            format!("  … {:<12} {phase}", kind.as_str())
        }
        AuditEvent::EcosystemFinished {
            kind,
            manifests,
            floating,
        } => format!(
            "  ✓  {:<12} {manifests} manifest{} · {floating} floating",
            kind.as_str(),
            if *manifests == 1 { "" } else { "s" }
        ),
        AuditEvent::EcosystemFailed { kind, error } => {
            format!("  ✗  {:<12} {error}", kind.as_str())
        }
        AuditEvent::AuditFinished { findings } => format!(
            "pinner audit · done · {findings} finding{}",
            if *findings == 1 { "" } else { "s" }
        ),
    }
}

impl AuditProgress for StderrAuditProgress {
    fn on_event(&self, event: AuditEvent) {
        let line = format_audit_event(&event);
        let mut err = io::stderr().lock();
        let _ = match (&event, self.color) {
            (AuditEvent::EcosystemFinished { .. }, true) => {
                write!(err, "{}", SetForegroundColor(Color::Green)).and_then(|_| {
                    writeln!(err, "{line}")?;
                    write!(err, "{}", ResetColor)
                })
            }
            (AuditEvent::EcosystemFailed { .. }, true) => {
                write!(err, "{}", SetForegroundColor(Color::Red)).and_then(|_| {
                    writeln!(err, "{line}")?;
                    write!(err, "{}", ResetColor)
                })
            }
            (AuditEvent::AuditStarted { .. } | AuditEvent::AuditFinished { .. }, true) => {
                write!(err, "{}", SetForegroundColor(Color::DarkCyan)).and_then(|_| {
                    writeln!(err, "{line}")?;
                    write!(err, "{}", ResetColor)
                })
            }
            _ => writeln!(err, "{line}"),
        };
        let _ = err.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinner_core::AuditEvent;
    use pinner_ecosystem::EcosystemKind;

    #[test]
    fn formats_started_banner() {
        let s = format_audit_event(&AuditEvent::AuditStarted {
            ecosystems: vec![EcosystemKind::Mise, EcosystemKind::Cargo],
        });
        assert!(s.contains("audit"));
        assert!(s.contains("2"));
        assert!(s.contains("parallel"));
    }

    #[test]
    fn formats_finished_ecosystem() {
        let s = format_audit_event(&AuditEvent::EcosystemFinished {
            kind: EcosystemKind::Cargo,
            manifests: 3,
            floating: 2,
        });
        assert!(s.contains("cargo"));
        assert!(s.contains("3"));
        assert!(s.contains("2"));
    }
}
