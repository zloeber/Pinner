use std::path::{Path, PathBuf};
use std::sync::Arc;

use pinner_ecosystem::{Ecosystem, EcosystemCtx, EvidenceKind, Pin, ResolveMode};
use serde::Serialize;

use crate::error::CoreError;
use crate::lock::LockFile;
use crate::orchestrate::{
    LOCK_NAME, RunOptions, discover_and_extract, is_allowlisted, lock_to_pins, selected_ecosystems,
};
use crate::policy::Policy;
use crate::report::RunReport;

#[derive(Debug, Clone, Serialize)]
pub struct ExplainReport {
    pub name: String,
    pub path: PathBuf,
    pub requested: String,
    pub pinned: String,
    pub evidence: EvidenceKind,
    pub detail: String,
}

/// Report floating, non-allowlisted findings without writing.
pub fn audit(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
) -> Result<RunReport, CoreError> {
    let lock_path = opts.repo.join(LOCK_NAME);
    let lock_pins = if lock_path.exists() {
        lock_to_pins(LockFile::read(&lock_path)?)
    } else {
        Vec::new()
    };
    let ctx = EcosystemCtx {
        repo: &opts.repo,
        lock_pins: &lock_pins,
        offline: opts.offline,
        pin_exact_ranges: policy.pin_exact_ranges,
        resolve_mode: ResolveMode::Pin,
    };

    let mut report = RunReport::default();
    for ecosystem in selected_ecosystems(ecosystems, policy, opts) {
        let (_manifests, extracted) =
            discover_and_extract(ecosystem.as_ref(), policy, &opts.repo, &ctx)?;
        report.findings.extend(
            extracted.into_iter().filter(|finding| {
                finding.is_floating && !is_allowlisted(finding, policy, &opts.repo)
            }),
        );
    }
    Ok(report)
}

/// Explain a pin by matching a lock entry (name or path substring), or by fresh resolve.
pub fn explain(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    target: &str,
) -> Result<ExplainReport, CoreError> {
    let lock_path = opts.repo.join(LOCK_NAME);
    if lock_path.exists() {
        let lock = LockFile::read(&lock_path)?;
        if let Some(entry) = lock
            .entries
            .iter()
            .find(|entry| matches_target(&entry.name, &entry.path, target))
        {
            return Ok(ExplainReport {
                name: entry.name.clone(),
                path: entry.path.clone(),
                requested: entry.requested.clone(),
                pinned: entry.pinned.clone(),
                evidence: entry.evidence,
                detail: format!(
                    "matched lock entry (evidence: {})",
                    evidence_label(entry.evidence)
                ),
            });
        }
    }

    explain_via_resolve(ecosystems, policy, opts, target)
}

fn explain_via_resolve(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    target: &str,
) -> Result<ExplainReport, CoreError> {
    let lock_path = opts.repo.join(LOCK_NAME);
    let lock_pins = if lock_path.exists() {
        lock_to_pins(LockFile::read(&lock_path)?)
    } else {
        Vec::new()
    };
    let ctx = EcosystemCtx {
        repo: &opts.repo,
        lock_pins: &lock_pins,
        offline: opts.offline,
        pin_exact_ranges: policy.pin_exact_ranges,
        resolve_mode: ResolveMode::Pin,
    };

    for ecosystem in selected_ecosystems(ecosystems, policy, opts) {
        let (_manifests, extracted) =
            discover_and_extract(ecosystem.as_ref(), policy, &opts.repo, &ctx)?;
        let Some(finding) = extracted
            .iter()
            .find(|finding| matches_target(&finding.name, &finding.path, target))
            .cloned()
        else {
            continue;
        };

        if !finding.is_floating {
            return Ok(ExplainReport {
                name: finding.name,
                path: finding.path,
                requested: finding.requested.clone(),
                pinned: finding.requested,
                evidence: EvidenceKind::Tool,
                detail: "already exact in manifest".to_string(),
            });
        }

        let resolved = ecosystem.resolve(std::slice::from_ref(&finding), &ctx)?;
        let pin = resolved
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::ExplainTargetNotFound(target.to_string()))?;
        return Ok(explain_from_pin(pin, "fresh resolve"));
    }

    Err(CoreError::ExplainTargetNotFound(target.to_string()))
}

fn explain_from_pin(pin: Pin, source: &str) -> ExplainReport {
    ExplainReport {
        name: pin.name,
        path: pin.path,
        requested: pin.requested,
        pinned: pin.pinned,
        evidence: pin.evidence,
        detail: format!("{source} (evidence: {})", evidence_label(pin.evidence)),
    }
}

fn matches_target(name: &str, path: &Path, target: &str) -> bool {
    name == target || path.to_string_lossy().contains(target)
}

fn evidence_label(evidence: EvidenceKind) -> &'static str {
    match evidence {
        EvidenceKind::Lock => "lock",
        EvidenceKind::NativeLock => "native_lock",
        EvidenceKind::Registry => "registry",
        EvidenceKind::Tool => "tool",
    }
}
