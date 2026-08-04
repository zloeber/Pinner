use std::collections::HashMap;
use std::env;

use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin};
use pinner_iac_common::{parse_resolve_map, resolve_image_digest, resolve_map_lookup};
use pinner_toolchain::{CommandRunner, RealCommandRunner};

use crate::ActionsEcosystem;
use crate::extract::is_image_finding;

impl ActionsEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let actions_map = actions_resolve_map_from_env();
        let docker_map = docker_resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            pins.push(resolve_one(
                &runner,
                finding,
                ctx,
                &actions_map,
                &docker_map,
            )?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    actions_map: &HashMap<String, String>,
    docker_map: &HashMap<String, String>,
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

    if is_image_finding(finding) {
        resolve_image(runner, finding, ctx, actions_map, docker_map)
    } else {
        resolve_action(runner, finding, ctx, actions_map)
    }
}

fn resolve_action(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    // PINNER_ACTIONS_RESOLVE_MAP is checked before gh api (test seam).
    if let Some(pinned) = map.get(&finding.requested) {
        return Ok(registry_pin(finding, pinned.clone()));
    }
    if let Some(pinned) = resolve_map_lookup(map, &finding.name, &finding.requested) {
        return Ok(registry_pin(finding, pinned));
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

fn resolve_image(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    actions_map: &HashMap<String, String>,
    docker_map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    // PINNER_DOCKER_RESOLVE_MAP uses bare requested keys (docker crate style).
    if let Some(pinned) = docker_map.get(&finding.requested) {
        return Ok(registry_pin(finding, pinned.clone()));
    }

    if let Some(pinned) = resolve_map_lookup(actions_map, &finding.name, &finding.requested) {
        return Ok(registry_pin(finding, pinned));
    }
    if let Some(pinned) = actions_map.get(&finding.requested) {
        return Ok(registry_pin(finding, pinned.clone()));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let pinned = resolve_image_digest(runner, &finding.requested).map_err(|hint| {
        EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        }
    })?;
    Ok(Pin {
        ecosystem: EcosystemKind::Actions,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence: EvidenceKind::Tool,
        metadata: Default::default(),
    })
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

fn actions_resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_ACTIONS_RESOLVE_MAP") else {
        return HashMap::new();
    };
    // Prefer iac-common parser (name@requested keys); also accepts bare requested=pinned.
    parse_resolve_map(&raw)
}

fn docker_resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_DOCKER_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_bare_resolve_map(&raw)
}

/// Docker-style map: `requested=pinned` (first `=`), matching pinner-docker.
fn parse_bare_resolve_map(raw: &str) -> HashMap<String, String> {
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
    use super::{parse_bare_resolve_map, split_requested};

    #[test]
    fn parse_docker_style_map() {
        let map = parse_bare_resolve_map(
            "node:20=node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        assert_eq!(
            map.get("node:20").map(String::as_str),
            Some("node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn split_requested_nested_path() {
        assert_eq!(
            split_requested("owner/repo/subdir/action@v1"),
            Some(("owner/repo".to_string(), "v1"))
        );
        assert_eq!(
            split_requested("org/repo/.github/workflows/reuse.yml@v1"),
            Some(("org/repo".to_string(), "v1"))
        );
    }
}
