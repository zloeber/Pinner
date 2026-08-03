use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use globset::Glob;
use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Finding, Manifest, Pin,
};

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

    let selected: Vec<_> = selected_ecosystems(ecosystems, policy, opts).collect();
    let selected_kinds: Vec<EcosystemKind> = selected.iter().map(|e| e.kind()).collect();

    let mut report = RunReport::default();
    let mut graph_pins = Vec::new();
    for ecosystem in &selected {
        let manifests = discover_manifests(ecosystem.as_ref(), policy, &opts.repo)?;
        let mut all_findings = Vec::new();
        for manifest in &manifests {
            all_findings.extend(ecosystem.extract(manifest, &ctx)?);
        }

        let floating: Vec<_> = all_findings
            .iter()
            .filter(|finding| {
                finding.is_floating && !is_allowlisted(finding, policy, &opts.repo)
            })
            .cloned()
            .collect();

        let resolved = ecosystem.resolve(&floating, &ctx)?;
        for manifest in &manifests {
            let manifest_pins: Vec<_> = resolved
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
        report.findings.extend(floating);
        graph_pins.extend(pins_for_full_graph(&all_findings, &resolved, &lock_pins, policy, &opts.repo));
    }

    // Preserve prior lock entries for ecosystems not visited this run, then overlay
    // selected-ecosystem graph pins and dedupe by (ecosystem, path, name).
    let mut combined: Vec<Pin> = lock_pins
        .into_iter()
        .filter(|pin| !selected_kinds.contains(&pin.ecosystem))
        .collect();
    combined.extend(graph_pins);
    report.pins = dedupe_pins(combined);

    if !opts.dry_run {
        write_lock_idempotent(&lock_path, &report.pins)?;
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

fn pin_key(pin: &Pin) -> (EcosystemKind, &Path, &str) {
    (pin.ecosystem, pin.path.as_path(), pin.name.as_str())
}

fn pins_for_full_graph(
    all_findings: &[Finding],
    resolved: &[Pin],
    prior_lock: &[Pin],
    policy: &Policy,
    repo: &Path,
) -> Vec<Pin> {
    let mut pins = Vec::new();
    for finding in all_findings {
        if finding.is_floating {
            if is_allowlisted(finding, policy, repo) {
                continue;
            }
            // Lock mirrors the rewritten source: requested == pinned.
            if let Some(pin) = resolved.iter().find(|pin| same_item(finding, pin)) {
                pins.push(Pin {
                    ecosystem: pin.ecosystem,
                    name: pin.name.clone(),
                    requested: pin.pinned.clone(),
                    pinned: pin.pinned.clone(),
                    path: pin.path.clone(),
                    evidence: pin.evidence,
                    metadata: pin.metadata.clone(),
                });
            }
            continue;
        }

        if let Some(prior) = prior_lock
            .iter()
            .find(|pin| same_item(finding, pin) && pin.pinned == finding.requested)
        {
            pins.push(Pin {
                ecosystem: finding.ecosystem,
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                pinned: finding.requested.clone(),
                path: finding.path.clone(),
                // Preserve prior evidence so a second pin does not flip tool→lock.
                evidence: prior.evidence,
                metadata: prior.metadata.clone(),
            });
        } else {
            pins.push(Pin {
                ecosystem: finding.ecosystem,
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                pinned: finding.requested.clone(),
                path: finding.path.clone(),
                evidence: EvidenceKind::Tool,
                metadata: Default::default(),
            });
        }
    }
    pins
}

/// Skip rewriting the lock when entries/`pinner_version` are unchanged so a second
/// `pin` keeps byte-identical `generated_at` and file contents.
fn write_lock_idempotent(lock_path: &Path, pins: &[Pin]) -> Result<(), CoreError> {
    let pinner_version = env!("CARGO_PKG_VERSION");
    match LockFile::read(lock_path) {
        Ok(existing) => {
            let candidate =
                LockFile::from_pins(pins, pinner_version, &existing.generated_at);
            if lock_substantive_eq(&existing, &candidate) {
                return Ok(());
            }
            LockFile::from_pins(pins, pinner_version, &generated_at()).write(lock_path)?;
        }
        Err(_) => {
            LockFile::from_pins(pins, pinner_version, &generated_at()).write(lock_path)?;
        }
    }
    Ok(())
}

fn lock_substantive_eq(a: &LockFile, b: &LockFile) -> bool {
    if a.version != b.version || a.pinner_version != b.pinner_version {
        return false;
    }
    let mut ae = a.entries.clone();
    let mut be = b.entries.clone();
    let key = |e: &LockEntry| {
        (
            e.ecosystem.as_str().to_string(),
            e.path.clone(),
            e.name.clone(),
        )
    };
    ae.sort_by_key(key);
    be.sort_by_key(key);
    ae == be
}

fn dedupe_pins(pins: Vec<Pin>) -> Vec<Pin> {
    let mut out = Vec::new();
    for pin in pins {
        if let Some(existing) = out.iter_mut().find(|p| pin_key(p) == pin_key(&pin)) {
            *existing = pin;
        } else {
            out.push(pin);
        }
    }
    out
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
