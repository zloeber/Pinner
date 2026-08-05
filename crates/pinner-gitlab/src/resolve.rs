use std::collections::HashMap;
use std::env;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
    upgrade_pin,
};
use pinner_iac_common::{
    parse_resolve_map, resolve_git_sha, resolve_image_digest, resolve_map_lookup,
};
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde_json::{Map, Value};

use crate::GitlabEcosystem;
use crate::extract::{include_ref, is_full_git_sha, is_include_finding};

impl GitlabEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let docker_map = docker_resolve_map_from_env();
        let gitlab_map = gitlab_resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            if let Some(pin) = resolve_one(&runner, finding, ctx, &docker_map, &gitlab_map)? {
                pins.push(pin);
            }
        }
        Ok(pins)
    }
}

fn resolve_one(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    docker_map: &HashMap<String, String>,
    gitlab_map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    if ctx.resolve_mode == ResolveMode::Upgrade {
        return resolve_upgrade(runner, finding, ctx, docker_map, gitlab_map);
    }

    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Gitlab
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Gitlab,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        }));
    }

    if is_include_finding(finding) {
        resolve_include(runner, finding, ctx, gitlab_map).map(Some)
    } else {
        resolve_image(runner, finding, ctx, docker_map, gitlab_map).map(Some)
    }
}

fn resolve_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    docker_map: &HashMap<String, String>,
    gitlab_map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    if is_include_finding(finding) {
        resolve_include_upgrade(runner, finding, ctx, gitlab_map)
    } else {
        resolve_image_upgrade(runner, finding, ctx, docker_map, gitlab_map)
    }
}

fn resolve_image(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    docker_map: &HashMap<String, String>,
    gitlab_map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    // PINNER_DOCKER_RESOLVE_MAP uses bare requested keys (docker crate style).
    if let Some(pinned) = docker_map.get(&finding.requested) {
        return Ok(gitlab_pin(
            finding,
            pinned.clone(),
            EvidenceKind::Registry,
            "image",
        ));
    }

    if let Some(pinned) = resolve_map_lookup(gitlab_map, &finding.name, &finding.requested) {
        return Ok(gitlab_pin(
            finding,
            pinned.clone(),
            EvidenceKind::Registry,
            "image",
        ));
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
    Ok(gitlab_pin(finding, pinned, EvidenceKind::Tool, "image"))
}

fn resolve_image_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    docker_map: &HashMap<String, String>,
    gitlab_map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    let Some(inspect_ref) = upgrade_image_ref(&finding.requested) else {
        return Ok(None);
    };

    let previous = previous_for_image_upgrade(finding, ctx);

    if let Some(newest) = docker_map
        .get(&finding.requested)
        .cloned()
        .or_else(|| docker_map.get(&inspect_ref).cloned())
        .or_else(|| resolve_map_lookup(gitlab_map, &finding.name, &finding.requested))
        .or_else(|| gitlab_map.get(&finding.requested).cloned())
    {
        return Ok(upgrade_kind_pin(
            finding,
            &previous,
            &newest,
            EvidenceKind::Registry,
            "map",
            "image",
        ));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let newest =
        resolve_image_digest(runner, &inspect_ref).map_err(|hint| EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        })?;

    Ok(upgrade_kind_pin(
        finding,
        &previous,
        &newest,
        EvidenceKind::Tool,
        "docker",
        "image",
    ))
}

fn resolve_include(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    gitlab_map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    let ref_ = include_ref(&finding.requested).unwrap_or(finding.requested.as_str());

    if let Some(pinned) = resolve_map_lookup(gitlab_map, &finding.name, &finding.requested) {
        return Ok(gitlab_pin(
            finding,
            include_pin_value(&finding.name, &pinned),
            EvidenceKind::Registry,
            "include",
        ));
    }
    // Also allow bare `project@ref` / `requested` keys without name@ prefix helper.
    if let Some(pinned) = gitlab_map.get(&finding.requested) {
        return Ok(gitlab_pin(
            finding,
            include_pin_value(&finding.name, pinned),
            EvidenceKind::Registry,
            "include",
        ));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let repo_url = format!("https://gitlab.com/{}.git", finding.name);
    let pinned =
        resolve_git_sha(runner, &repo_url, ref_).map_err(|hint| EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint: format!("{hint}; set PINNER_GITLAB_RESOLVE_MAP (name@requested=sha)"),
        })?;
    Ok(gitlab_pin(
        finding,
        include_pin_value(&finding.name, &pinned),
        EvidenceKind::Tool,
        "include",
    ))
}

fn resolve_include_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    gitlab_map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    let previous = previous_for_include_upgrade(finding, ctx);

    if let Some(mapped) = resolve_map_lookup(gitlab_map, &finding.name, &finding.requested)
        .or_else(|| gitlab_map.get(&finding.requested).cloned())
    {
        let newest = include_pin_value(&finding.name, &mapped);
        return Ok(upgrade_kind_pin(
            finding,
            &previous,
            &newest,
            EvidenceKind::Registry,
            "map",
            "include",
        ));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let repo_url = format!("https://gitlab.com/{}.git", finding.name);
    // Upgrade resolves newest tip (HEAD), not the current floating branch name alone.
    let sha =
        resolve_git_sha(runner, &repo_url, "HEAD").map_err(|hint| EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint: format!("{hint}; set PINNER_GITLAB_RESOLVE_MAP (name@requested=sha)"),
        })?;
    let newest = include_pin_value(&finding.name, &sha);

    Ok(upgrade_kind_pin(
        finding,
        &previous,
        &newest,
        EvidenceKind::Tool,
        "git",
        "include",
    ))
}

/// Lock/check form matches extract: `project@sha` (rewrite writes only the SHA to `ref:`).
fn include_pin_value(project: &str, pinned: &str) -> String {
    let pinned = pinned.trim();
    let prefix = format!("{project}@");
    if pinned.starts_with(&prefix) {
        pinned.to_string()
    } else {
        format!("{prefix}{pinned}")
    }
}

fn upgrade_kind_pin(
    finding: &Finding,
    previous: &str,
    newest: &str,
    evidence: EvidenceKind,
    channel: &str,
    kind: &str,
) -> Option<Pin> {
    let mut pin = upgrade_pin(finding, previous, newest, evidence, channel)?;
    pin.metadata
        .insert("kind".into(), Value::String(kind.to_string()));
    Some(pin)
}

fn previous_for_image_upgrade(finding: &Finding, ctx: &EcosystemCtx<'_>) -> String {
    if finding.requested.contains("@sha256:") {
        return finding.requested.clone();
    }
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Gitlab
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return lock.pinned.clone();
    }
    finding.requested.clone()
}

fn previous_for_include_upgrade(finding: &Finding, ctx: &EcosystemCtx<'_>) -> String {
    if let Some(ref_) = include_ref(&finding.requested)
        && is_full_git_sha(ref_)
    {
        return include_pin_value(&finding.name, ref_);
    }
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Gitlab
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return lock.pinned.clone();
    }
    finding.requested.clone()
}

/// Tag/name form to re-resolve. Digest-only without `:tag` → None.
fn upgrade_image_ref(requested: &str) -> Option<String> {
    let requested = requested.trim();
    if requested.is_empty() {
        return None;
    }
    if let Some((left, _digest)) = requested.split_once("@sha256:") {
        if image_has_tag(left) {
            return Some(left.to_string());
        }
        return None;
    }
    Some(requested.to_string())
}

fn image_has_tag(image: &str) -> bool {
    let after_slash = image.rfind('/').map(|i| i + 1).unwrap_or(0);
    image[after_slash..].contains(':')
}

fn gitlab_pin(finding: &Finding, pinned: String, evidence: EvidenceKind, kind: &str) -> Pin {
    let mut metadata = Map::new();
    metadata.insert("kind".into(), Value::String(kind.to_string()));
    Pin {
        ecosystem: EcosystemKind::Gitlab,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence,
        metadata,
    }
}

fn docker_resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_DOCKER_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_bare_resolve_map(&raw)
}

fn gitlab_resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_GITLAB_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_resolve_map(&raw)
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
    use super::{parse_bare_resolve_map, upgrade_image_ref};

    #[test]
    fn parse_docker_style_map() {
        let map = parse_bare_resolve_map(
            "node:latest=node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        assert_eq!(
            map.get("node:latest").map(String::as_str),
            Some("node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn upgrade_image_ref_skips_digest_only() {
        assert_eq!(
            upgrade_image_ref(
                "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            None
        );
        assert_eq!(
            upgrade_image_ref(
                "node:20@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            )
            .as_deref(),
            Some("node:20")
        );
        assert_eq!(
            upgrade_image_ref("node:latest").as_deref(),
            Some("node:latest")
        );
    }
}
