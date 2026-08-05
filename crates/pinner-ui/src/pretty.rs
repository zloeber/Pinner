use std::collections::BTreeMap;
use std::io::Write;

use crossterm::style::{Color, ResetColor, SetForegroundColor};
use pinner_core::RunReport;
use pinner_ecosystem::Finding;

/// Human-oriented multi-line summary for TTY text mode (non-walkthrough).
pub fn emit_pretty_report(report: &RunReport, writer: &mut impl Write) -> std::io::Result<()> {
    let upgraded = if report.upgraded > 0 {
        format!("  ·  upgraded {}", report.upgraded)
    } else {
        String::new()
    };
    writeln!(
        writer,
        "Pinner run  ·  pins {}  ·  rewrites {}  ·  findings {}  ·  drift {}{upgraded}",
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

/// Beautiful TTY audit summary: status panel, ecosystem breakdown, aligned finding table.
///
/// When `color` is true, applies crossterm ANSI colors (green clean / yellow findings).
/// Non-TTY / agent / JSON callers should keep using the plain audit text contract instead.
pub fn emit_pretty_audit(
    report: &RunReport,
    writer: &mut impl Write,
    color: bool,
) -> std::io::Result<()> {
    let findings = &report.findings;
    let by_eco = ecosystem_counts(findings);
    let inner_width = 56usize;

    write_rule(writer, '┌', '─', '┐', " audit ", inner_width)?;
    if findings.is_empty() {
        write_panel_line(
            writer,
            color,
            Color::Green,
            "Status",
            "clean · no floating findings",
            inner_width,
        )?;
    } else {
        let status = format!(
            "floating · {} finding{}",
            findings.len(),
            if findings.len() == 1 { "" } else { "s" }
        );
        write_panel_line(writer, color, Color::Yellow, "Status", &status, inner_width)?;
        let breakdown = by_eco
            .iter()
            .map(|(eco, count)| format!("{eco} {count}"))
            .collect::<Vec<_>>()
            .join(" · ");
        write_panel_line(
            writer,
            color,
            Color::DarkCyan,
            "Breakdown",
            &breakdown,
            inner_width,
        )?;
    }
    write_rule(writer, '└', '─', '┘', "", inner_width)?;

    if findings.is_empty() {
        return Ok(());
    }

    writeln!(writer)?;
    let eco_w = findings
        .iter()
        .map(|f| f.ecosystem.as_str().len())
        .max()
        .unwrap_or(3)
        .max(3);
    let name_w = findings
        .iter()
        .map(|f| f.name.len())
        .max()
        .unwrap_or(4)
        .clamp(4, 28);
    let req_w = findings
        .iter()
        .map(|f| f.requested.len())
        .max()
        .unwrap_or(9)
        .clamp(9, 24);

    if color {
        write!(writer, "{}", SetForegroundColor(Color::DarkGrey))?;
    }
    writeln!(
        writer,
        "  {:eco_w$}  {:name_w$}  {:req_w$}  PATH",
        "ECO", "NAME", "REQUESTED"
    )?;
    if color {
        write!(writer, "{}", ResetColor)?;
    }

    for finding in findings {
        let name = truncate(&finding.name, name_w);
        let requested = truncate(&finding.requested, req_w);
        if color {
            write!(writer, "{}", SetForegroundColor(Color::DarkCyan))?;
            write!(writer, "  {:eco_w$}", finding.ecosystem.as_str())?;
            write!(writer, "{}", ResetColor)?;
            write!(writer, "  {:name_w$}", name)?;
            write!(writer, "{}", SetForegroundColor(Color::Yellow))?;
            write!(writer, "  {:req_w$}", requested)?;
            write!(writer, "{}", ResetColor)?;
            write!(writer, "{}", SetForegroundColor(Color::DarkGrey))?;
            writeln!(writer, "  {}", finding.path.display())?;
            write!(writer, "{}", ResetColor)?;
        } else {
            writeln!(
                writer,
                "  {:eco_w$}  {:name_w$}  {:req_w$}  {}",
                finding.ecosystem.as_str(),
                name,
                requested,
                finding.path.display()
            )?;
        }
    }

    writeln!(writer)?;
    if color {
        write!(writer, "{}", SetForegroundColor(Color::DarkGrey))?;
    }
    writeln!(
        writer,
        "  hint: pinner pin --walkthrough   to review and freeze"
    )?;
    if color {
        write!(writer, "{}", ResetColor)?;
    }

    Ok(())
}

fn ecosystem_counts(findings: &[Finding]) -> BTreeMap<&str, usize> {
    let mut by_eco: BTreeMap<&str, usize> = BTreeMap::new();
    for finding in findings {
        *by_eco.entry(finding.ecosystem.as_str()).or_default() += 1;
    }
    by_eco
}

fn write_rule(
    writer: &mut impl Write,
    left: char,
    fill: char,
    right: char,
    title: &str,
    inner_width: usize,
) -> std::io::Result<()> {
    let title_len = title.chars().count();
    let fill_len = inner_width.saturating_sub(title_len);
    let fill_str = fill.to_string().repeat(fill_len);
    writeln!(writer, "{left}{title}{fill_str}{right}")
}

fn write_panel_line(
    writer: &mut impl Write,
    color: bool,
    label_color: Color,
    label: &str,
    value: &str,
    inner_width: usize,
) -> std::io::Result<()> {
    let content = format!("{label:<10} {value}");
    let pad = inner_width.saturating_sub(content.chars().count());
    write!(writer, "│ ")?;
    if color {
        write!(writer, "{}", SetForegroundColor(label_color))?;
        write!(writer, "{label:<10}")?;
        write!(writer, "{}", ResetColor)?;
        write!(writer, " {value}")?;
    } else {
        write!(writer, "{content}")?;
    }
    for _ in 0..pad {
        write!(writer, " ")?;
    }
    writeln!(writer, " │")
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let keep = max - 1;
    let mut out: String = value.chars().take(keep).collect();
    out.push('…');
    out
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

    #[test]
    fn pretty_audit_clean_panel() {
        let report = RunReport::default();
        let mut buf = Vec::new();
        emit_pretty_audit(&report, &mut buf, false).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("┌"));
        assert!(text.contains("audit"));
        assert!(text.contains("clean · no floating findings"));
        assert!(!text.contains("hint:"));
    }

    #[test]
    fn pretty_audit_findings_table_and_breakdown() {
        let report = RunReport {
            findings: vec![
                Finding {
                    ecosystem: EcosystemKind::Mise,
                    name: "node".into(),
                    requested: "latest".into(),
                    path: PathBuf::from(".mise.toml"),
                    is_floating: true,
                },
                Finding {
                    ecosystem: EcosystemKind::Node,
                    name: "lodash".into(),
                    requested: "^4.17.0".into(),
                    path: PathBuf::from("package.json"),
                    is_floating: true,
                },
            ],
            ..Default::default()
        };
        let mut buf = Vec::new();
        emit_pretty_audit(&report, &mut buf, false).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("floating · 2 findings"));
        assert!(text.contains("mise 1 · node 1") || text.contains("mise 1"));
        assert!(text.contains("REQUESTED"));
        assert!(text.contains("lodash"));
        assert!(text.contains("package.json"));
        assert!(text.contains("pinner pin --walkthrough"));
    }
}
