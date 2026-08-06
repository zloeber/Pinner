use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use globset::Glob;
use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Finding, Manifest, Pin, ResolveMode,
    Rewrite, repo_relative,
};

use crate::error::CoreError;
use crate::gitignore::RepoIgnore;
use crate::lock::{LockEntry, LockFile};
use crate::policy::{AllowFloating, Policy};
use crate::report::{DriftItem, RunReport};
use crate::walkthrough::WalkthroughOutcome;

pub(crate) const LOCK_NAME: &str = "pinner.lock.json";

pub struct RunOptions {
    pub repo: PathBuf,
    pub dry_run: bool,
    pub offline: bool,
    pub ecosystems_filter: Option<Vec<EcosystemKind>>,
}

struct StagedRewrite {
    rewrite: Rewrite,
}

struct ResolvedBatch {
    ecosystem: Arc<dyn Ecosystem>,
    manifests: Vec<Manifest>,
    all_findings: Vec<Finding>,
    candidates: Vec<Finding>,
    resolved: Vec<Pin>,
}

/// Callback invoked after resolve with proposed pins; may filter or abort.
pub type WalkthroughFilter<'a> = dyn FnMut(&[Pin]) -> Result<WalkthroughOutcome, CoreError> + 'a;

pub fn pin(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
) -> Result<RunReport, CoreError> {
    pin_with_filter(ecosystems, policy, opts, None)
}

/// Like [`pin`], but after resolve and before rewrite/lock an optional walkthrough
/// callback may accept/skip/edit resolved pins or abort with no writes.
pub fn pin_with_filter(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    walkthrough: Option<&mut WalkthroughFilter<'_>>,
) -> Result<RunReport, CoreError> {
    run_resolve_rewrite(ecosystems, policy, opts, walkthrough, ResolveMode::Pin)
}

pub fn upgrade(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
) -> Result<RunReport, CoreError> {
    upgrade_with_filter(ecosystems, policy, opts, None)
}

/// Like [`upgrade`], with an optional walkthrough before rewrite/lock.
pub fn upgrade_with_filter(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    walkthrough: Option<&mut WalkthroughFilter<'_>>,
) -> Result<RunReport, CoreError> {
    run_resolve_rewrite(ecosystems, policy, opts, walkthrough, ResolveMode::Upgrade)
}

fn run_resolve_rewrite(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    mut walkthrough: Option<&mut WalkthroughFilter<'_>>,
    resolve_mode: ResolveMode,
) -> Result<RunReport, CoreError> {
    let gitignore = RepoIgnore::new(&opts.repo);
    let lock_path = opts.repo.join(LOCK_NAME);
    let prior_lock = if lock_path.exists() {
        lock_to_pins(LockFile::read(&lock_path)?)
    } else {
        Vec::new()
    };

    let selected: Vec<_> = selected_ecosystems(ecosystems, policy, opts).collect();
    let selected_kinds: Vec<EcosystemKind> = selected.iter().map(|e| e.kind()).collect();

    // Phase 1: discover/extract/resolve for every selected ecosystem. On any resolve
    // failure, write nothing. Rewrites are staged only after an optional walkthrough.
    let mut batches = Vec::new();
    let mut all_resolved = Vec::new();
    {
        // Always pass prior lock pins. Pin mode may select from them; Upgrade mode
        // must ignore them for choosing newest (ecosystems branch on resolve_mode)
        // but may peek them for display-only `previous` metadata.
        let ctx = EcosystemCtx {
            repo: &opts.repo,
            lock_pins: &prior_lock,
            offline: opts.offline,
            pin_exact_ranges: policy.pin_exact_ranges,
            resolve_mode,
        };

        for ecosystem in &selected {
            let (manifests, all_findings) =
                discover_and_extract(ecosystem.as_ref(), policy, &opts.repo, &ctx, &gitignore)?;

            let candidates: Vec<_> = match resolve_mode {
                ResolveMode::Pin => all_findings
                    .iter()
                    .filter(|finding| {
                        finding.is_floating && !is_allowlisted(finding, policy, &opts.repo)
                    })
                    .cloned()
                    .collect(),
                // Allowlisted floating refs remain upgradeable; do not filter them out.
                ResolveMode::Upgrade => all_findings.clone(),
            };

            let mut resolved = ecosystem.resolve(&candidates, &ctx)?;
            if resolve_mode == ResolveMode::Upgrade {
                resolved.retain(|pin| !is_unchanged_upgrade(pin));
            }
            all_resolved.extend(resolved.iter().cloned());
            batches.push(ResolvedBatch {
                ecosystem: Arc::clone(ecosystem),
                manifests,
                all_findings,
                candidates,
                resolved,
            });
        }
    }

    if let Some(cb) = walkthrough.as_mut() {
        match cb(&all_resolved)? {
            WalkthroughOutcome::Aborted => return Ok(RunReport::default()),
            WalkthroughOutcome::Continue { pins } => {
                // Redistribute filtered resolved pins back onto each batch by key.
                for batch in &mut batches {
                    batch.resolved = pins
                        .iter()
                        .filter(|pin| pin.ecosystem == batch.ecosystem.kind())
                        .cloned()
                        .collect();
                }
                all_resolved = pins;
            }
        }
    }

    // Upgrade with nothing to bump: success, no writes.
    if resolve_mode == ResolveMode::Upgrade && all_resolved.is_empty() {
        let mut report = RunReport::default();
        for batch in &batches {
            report.findings.extend(batch.candidates.iter().cloned());
        }
        return Ok(report);
    }

    let mut report = RunReport::default();
    if resolve_mode == ResolveMode::Upgrade {
        report.upgraded = all_resolved.len();
    }
    let mut graph_pins = Vec::new();
    let mut staged: Vec<StagedRewrite> = Vec::new();

    for batch in &batches {
        for manifest in &batch.manifests {
            let rel = repo_relative(&opts.repo, &manifest.path);
            let manifest_pins: Vec<_> = batch
                .resolved
                .iter()
                .filter(|pin| pin.path == rel)
                .cloned()
                .collect();
            if manifest_pins.is_empty() {
                continue;
            }
            if let Some(mut rewrite) = batch.ecosystem.rewrite(manifest, &manifest_pins)? {
                rewrite.path = repo_relative(&opts.repo, &rewrite.path);
                staged.push(StagedRewrite { rewrite });
            }
        }
        report.findings.extend(batch.candidates.iter().cloned());
        graph_pins.extend(pins_for_full_graph(
            &batch.all_findings,
            &batch.resolved,
            &prior_lock,
            policy,
            &opts.repo,
        ));
    }

    // Preserve prior lock entries for ecosystems not visited this run, then overlay
    // selected-ecosystem graph pins and dedupe by (ecosystem, path, name).
    let mut combined: Vec<Pin> = prior_lock
        .into_iter()
        .filter(|pin| !selected_kinds.contains(&pin.ecosystem))
        .collect();
    combined.extend(graph_pins);
    report.pins = dedupe_pins(combined);

    // Phase 2: only after all resolves (and optional walkthrough) succeed, write.
    if !opts.dry_run {
        for staged_rw in &staged {
            let abs = if staged_rw.rewrite.path.is_absolute() {
                staged_rw.rewrite.path.clone()
            } else {
                opts.repo.join(&staged_rw.rewrite.path)
            };
            std::fs::write(&abs, &staged_rw.rewrite.new_contents)?;
        }
        write_lock_idempotent(&lock_path, &report.pins)?;
    }
    report.rewrites = staged.into_iter().map(|s| s.rewrite).collect();

    Ok(report)
}

fn is_unchanged_upgrade(pin: &Pin) -> bool {
    pin.metadata
        .get("previous")
        .and_then(|v| v.as_str())
        .is_some_and(|prev| prev == pin.pinned)
}

pub fn check(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
) -> Result<RunReport, CoreError> {
    let gitignore = RepoIgnore::new(&opts.repo);
    let lock_path = opts.repo.join(LOCK_NAME);
    if !lock_path.exists() {
        return Err(CoreError::MissingLock);
    }

    let lock_pins = lock_to_pins(LockFile::read(&lock_path)?);
    let ctx = EcosystemCtx {
        repo: &opts.repo,
        lock_pins: &lock_pins,
        offline: true,
        pin_exact_ranges: policy.pin_exact_ranges,
        resolve_mode: ResolveMode::Pin,
    };
    let mut report = RunReport {
        pins: lock_pins.clone(),
        ..RunReport::default()
    };

    for ecosystem in selected_ecosystems(ecosystems, policy, opts) {
        let (_manifests, extracted) =
            discover_and_extract(ecosystem.as_ref(), policy, &opts.repo, &ctx, &gitignore)?;

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

pub(crate) fn selected_ecosystems<'a>(
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

pub(crate) fn discover_manifests(
    ecosystem: &dyn Ecosystem,
    policy: &Policy,
    repo: &Path,
    gitignore: &RepoIgnore,
) -> Result<Vec<Manifest>, CoreError> {
    Ok(ecosystem
        .discover(repo)?
        .into_iter()
        .filter(|manifest| {
            let path = repo_relative(repo, &manifest.path);
            !policy.is_ignored(&path) && !gitignore.is_ignored(&path)
        })
        .collect())
}

pub(crate) fn is_allowlisted(finding: &Finding, policy: &Policy, repo: &Path) -> bool {
    policy.allow_floating.iter().any(|allowed| {
        allowed.ecosystem == finding.ecosystem
            && allowed.name == finding.name
            && path_matches(allowed, &finding.path, repo)
    })
}

/// Discover manifests and extract findings for one ecosystem (shared by pin/check/audit/explain).
/// Finding paths are normalized to repo-relative for portable lock/check comparisons.
/// Manifest paths remain absolute for I/O in rewrite.
pub(crate) fn discover_and_extract(
    ecosystem: &dyn Ecosystem,
    policy: &Policy,
    repo: &Path,
    ctx: &EcosystemCtx<'_>,
    gitignore: &RepoIgnore,
) -> Result<(Vec<Manifest>, Vec<Finding>), CoreError> {
    let manifests = discover_manifests(ecosystem, policy, repo, gitignore)?;
    let mut findings = Vec::new();
    for manifest in &manifests {
        for mut finding in ecosystem.extract(manifest, ctx)? {
            finding.path = repo_relative(repo, &finding.path);
            findings.push(finding);
        }
    }
    Ok((manifests, findings))
}

fn path_matches(allowed: &AllowFloating, path: &Path, repo: &Path) -> bool {
    let Some(pattern) = &allowed.path_glob else {
        return true;
    };
    let relative = repo_relative(repo, path);
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
        // Prefer a resolved pin when present (floating pin + exact upgrade bumps).
        if let Some(pin) = resolved.iter().find(|pin| same_item(finding, pin)) {
            pins.push(Pin {
                ecosystem: pin.ecosystem,
                name: pin.name.clone(),
                // Lock mirrors the rewritten source: requested == pinned.
                requested: pin.pinned.clone(),
                pinned: pin.pinned.clone(),
                path: repo_relative(repo, &pin.path),
                evidence: pin.evidence,
                metadata: pin.metadata.clone(),
            });
            continue;
        }

        if finding.is_floating {
            if is_allowlisted(finding, policy, repo) {
                continue;
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
            let candidate = LockFile::from_pins(pins, pinner_version, &existing.generated_at);
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

pub(crate) fn lock_to_pins(lock: LockFile) -> Vec<Pin> {
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
