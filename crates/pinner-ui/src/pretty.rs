use std::collections::BTreeMap;
use std::io::Write;

use pinner_core::RunReport;

/// Human-oriented multi-line summary for TTY text mode (non-walkthrough).
pub fn emit_pretty_report(report: &RunReport, writer: &mut impl Write) -> std::io::Result<()> {
    writeln!(
        writer,
        "Pinner run  ·  pins {}  ·  rewrites {}  ·  findings {}  ·  drift {}",
        report.pins.len(),
        report.rewrites.len(),
        report.findings.len(),
        report.drift.len()
    )?;

    if !report.findings.is_empty() {
        let mut by_eco: BTreeMap<&str, usize> = BTreeMap::new();
        for finding in &report.findings {
            *by_eco.entry(finding.ecosystem.as_str()).or_default() += 1;
        }
        writeln!(writer, "Findings by ecosystem:")?;
        for (eco, count) in by_eco {
            writeln!(writer, "  {eco}: {count}")?;
        }
        for finding in &report.findings {
            writeln!(
                writer,
                "  • {} {}  requested={}  ({})",
                finding.ecosystem.as_str(),
                finding.name,
                finding.requested,
                finding.path.display()
            )?;
        }
    }

    if !report.drift.is_empty() {
        writeln!(writer, "Drift:")?;
        for item in &report.drift {
            writeln!(
                writer,
                "  • {}  expected={}  actual={}  ({})",
                item.name,
                item.expected,
                item.actual,
                item.path.display()
            )?;
        }
    }

    if !report.rewrites.is_empty() {
        writeln!(writer, "Rewrites:")?;
        for rewrite in &report.rewrites {
            writeln!(writer, "  • {}", rewrite.path.display())?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinner_core::DriftItem;
    use pinner_ecosystem::{EcosystemKind, Finding};
    use std::path::PathBuf;

    #[test]
    fn pretty_report_includes_counts_and_findings() {
        let report = RunReport {
            findings: vec![Finding {
                ecosystem: EcosystemKind::Mise,
                name: "node".into(),
                requested: "lts".into(),
                path: PathBuf::from(".mise.toml"),
                is_floating: true,
            }],
            drift: vec![DriftItem {
                path: PathBuf::from(".mise.toml"),
                name: "node".into(),
                expected: "20.0.0".into(),
                actual: "18.0.0".into(),
            }],
            ..Default::default()
        };
        let mut buf = Vec::new();
        emit_pretty_report(&report, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("pins 0"));
        assert!(text.contains("findings 1"));
        assert!(text.contains("mise: 1"));
        assert!(text.contains("Drift:"));
    }
}
