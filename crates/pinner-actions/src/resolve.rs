use std::collections::HashMap;
use std::env;

use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin};
use pinner_toolchain::{CommandRunner, RealCommandRunner};

use crate::ActionsEcosystem;

impl ActionsEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            pins.push(resolve_one(&runner, finding, ctx, &map)?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Actions
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Actions,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    // PINNER_ACTIONS_RESOLVE_MAP is checked before gh api (test seam).
    if let Some(pinned) = map.get(&finding.requested) {
        return Ok(registry_pin(finding, pinned.clone()));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let pinned = resolve_via_gh(runner, finding).map_err(|hint| EcosystemError::Resolve {
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        hint,
    })?;
    Ok(registry_pin(finding, pinned))
}

fn registry_pin(finding: &Finding, pinned: String) -> Pin {
    Pin {
        ecosystem: EcosystemKind::Actions,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence: EvidenceKind::Registry,
        metadata: Default::default(),
    }
}

/// Resolve `owner/repo[/path]@ref` via `gh api repos/{owner}/{repo}/commits/{ref}`.
fn resolve_via_gh(runner: &dyn CommandRunner, finding: &Finding) -> Result<String, String> {
    let (owner_repo, ref_) = split_requested(&finding.requested)
        .ok_or_else(|| format!("invalid action ref: {}", finding.requested))?;
    let api_path = format!("repos/{owner_repo}/commits/{ref_}");
    let output = runner
        .run("gh", &["api", &api_path, "--jq", ".sha"])
        .map_err(|err| format!("gh api {api_path}: {err}"))?;
    if output.status != 0 {
        return Err(format!(
            "gh api {api_path} failed (status {}): {}",
            output.status,
            output.stderr.trim()
        ));
    }
    let sha = first_nonempty_line(&output.stdout);
    if sha.is_empty() {
        return Err(format!("gh api {api_path} returned empty sha"));
    }
    // Prefer JSON object fallback if --jq unavailable / ignored.
    if sha.starts_with('{') {
        let value: serde_json::Value = serde_json::from_str(&output.stdout)
            .map_err(|e| format!("parse gh api response: {e}"))?;
        let sha = value
            .get("sha")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("gh api {api_path} response missing sha"))?;
        return Ok(sha.to_string());
    }
    Ok(sha)
}

/// `owner/repo/path@ref` → API owner/repo is first two path segments.
fn split_requested(requested: &str) -> Option<(String, &str)> {
    let at = requested.rfind('@')?;
    let name = &requested[..at];
    let ref_ = &requested[at + 1..];
    if name.is_empty() || ref_.is_empty() {
        return None;
    }
    let mut parts = name.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((format!("{owner}/{repo}"), ref_))
}

fn first_nonempty_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Parse `PINNER_ACTIONS_RESOLVE_MAP=actions/checkout@v4=11bd7190…,other/action@v1=…`.
fn resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_ACTIONS_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_resolve_map(&raw)
}

fn parse_resolve_map(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::{parse_resolve_map, split_requested};

    #[test]
    fn parse_resolve_map_entries() {
        let map = parse_resolve_map("actions/checkout@v4=11bd71901bbe5b1630ceea73d27597364c9af683");
        assert_eq!(
            map.get("actions/checkout@v4").map(String::as_str),
            Some("11bd71901bbe5b1630ceea73d27597364c9af683")
        );
    }

    #[test]
    fn split_requested_nested_path() {
        assert_eq!(
            split_requested("owner/repo/subdir/action@v1"),
            Some(("owner/repo".to_string(), "v1"))
        );
    }
}
