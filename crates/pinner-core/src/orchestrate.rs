use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use globset::Glob;
use pinner_ecosystem::{Ecosystem, EcosystemCtx, EcosystemKind, Finding, Manifest, Pin};

use crate::error::CoreError;
use crate::lock::{LockEntry, LockFile};
use crate::policy::{AllowFloating, Policy};
use crate::report::{DriftItem, RunReport};

const LOCK_NAME: &str = "pinner.lock.json";

pub struct RunOptions {
    pub repo: PathBuf,
    pub dry_run: bool,
    pub offline: bool,
    pub ecosystems_filter: Option<Vec<EcosystemKind>>,
}

pub fn pin(
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
        lock_pins: &lock_pins,
        offline: opts.offline,
        pin_exact_ranges: policy.pin_exact_ranges,
    };

    let mut report = RunReport::default();
    for ecosystem in selected_ecosystems(ecosystems, policy, opts) {
        let manifests = discover_manifests(ecosystem.as_ref(), policy, &opts.repo)?;
        let mut findings = Vec::new();
        for manifest in &manifests {
            findings.extend(
                ecosystem
                    .extract(manifest, &ctx)?
                    .into_iter()
                    .filter(|finding| {
                        finding.is_floating && !is_allowlisted(finding, policy, &opts.repo)
                    }),
            );
        }

        let pins = ecosystem.resolve(&findings, &ctx)?;
        for manifest in &manifests {
            let manifest_pins: Vec<_> = pins
                .iter()
                .filter(|pin| pin.path == manifest.path)
                .cloned()
                .collect();
            if manifest_pins.is_empty() {
                continue;
            }
            if let Some(rewrite) = ecosystem.rewrite(manifest, &manifest_pins)? {
                if !opts.dry_run {
                    std::fs::write(&rewrite.path, &rewrite.new_contents)?;
                }
                report.rewrites.push(rewrite);
            }
        }
        report.findings.extend(findings);
        report.pins.extend(pins);
    }

    if !opts.dry_run {
        LockFile::from_pins(&report.pins, env!("CARGO_PKG_VERSION"), &generated_at())
            .write(&lock_path)?;
    }

    Ok(report)
}

pub fn check(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
) -> Result<RunReport, CoreError> {
    let lock_path = opts.repo.join(LOCK_NAME);
    if !lock_path.exists() {
        return Err(CoreError::MissingLock);
    }

    let lock_pins = lock_to_pins(LockFile::read(&lock_path)?);
    let ctx = EcosystemCtx {
        lock_pins: &lock_pins,
        offline: true,
        pin_exact_ranges: policy.pin_exact_ranges,
    };
    let mut report = RunReport {
        pins: lock_pins.clone(),
        ..RunReport::default()
    };

    for ecosystem in selected_ecosystems(ecosystems, policy, opts) {
        let manifests = discover_manifests(ecosystem.as_ref(), policy, &opts.repo)?;
        let mut extracted = Vec::new();
        for manifest in &manifests {
            extracted.extend(ecosystem.extract(manifest, &ctx)?);
        }

        for pin in lock_pins
            .iter()
            .filter(|pin| pin.ecosystem == ecosystem.kind())
        {
            let current = extracted.iter().find(|finding| same_item(finding, pin));
            if !matches!(current, Some(f) if !f.is_floating && f.requested == pin.pinned) {
                report.drift.push(DriftItem {
                    path: pin.path.clone(),
                    name: pin.name.clone(),
                    expected: pin.pinned.clone(),
                    actual: current
                        .map(|finding| finding.requested.clone())
                        .unwrap_or_else(|| "<missing>".to_string()),
                });
            }
        }

        for finding in extracted
            .into_iter()
            .filter(|finding| finding.is_floating && !is_allowlisted(finding, policy, &opts.repo))
        {
            if !lock_pins.iter().any(|pin| same_item(&finding, pin)) {
                report.drift.push(DriftItem {
                    path: finding.path.clone(),
                    name: finding.name.clone(),
                    expected: "<pinned>".to_string(),
                    actual: finding.requested.clone(),
                });
            }
            report.findings.push(finding);
        }
    }

    Ok(report)
}

fn selected_ecosystems<'a>(
    ecosystems: &'a [Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
) -> impl Iterator<Item = &'a Arc<dyn Ecosystem>> {
    ecosystems.iter().filter(move |ecosystem| {
        policy.is_enabled(ecosystem.kind())
            && opts
                .ecosystems_filter
                .as_ref()
                .is_none_or(|filter| filter.contains(&ecosystem.kind()))
    })
}

fn discover_manifests(
    ecosystem: &dyn Ecosystem,
    policy: &Policy,
    repo: &Path,
) -> Result<Vec<Manifest>, CoreError> {
    Ok(ecosystem
        .discover(repo)?
        .into_iter()
        .filter(|manifest| {
            let path = manifest.path.strip_prefix(repo).unwrap_or(&manifest.path);
            !policy.is_ignored(path)
        })
        .collect())
}

fn is_allowlisted(finding: &Finding, policy: &Policy, repo: &Path) -> bool {
    policy.allow_floating.iter().any(|allowed| {
        allowed.ecosystem == finding.ecosystem
            && allowed.name == finding.name
            && path_matches(allowed, &finding.path, repo)
    })
}

fn path_matches(allowed: &AllowFloating, path: &Path, repo: &Path) -> bool {
    let Some(pattern) = &allowed.path_glob else {
        return true;
    };
    let relative = path.strip_prefix(repo).unwrap_or(path);
    Glob::new(pattern)
        .map(|glob| glob.compile_matcher().is_match(relative))
        .unwrap_or(false)
}

fn same_item(finding: &Finding, pin: &Pin) -> bool {
    finding.ecosystem == pin.ecosystem && finding.name == pin.name && finding.path == pin.path
}

fn lock_to_pins(lock: LockFile) -> Vec<Pin> {
    lock.entries.into_iter().map(entry_to_pin).collect()
}

fn entry_to_pin(entry: LockEntry) -> Pin {
    Pin {
        ecosystem: entry.ecosystem,
        name: entry.name,
        requested: entry.requested,
        pinned: entry.pinned,
        path: entry.path,
        evidence: entry.evidence,
        metadata: entry.metadata,
    }
}

fn generated_at() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
