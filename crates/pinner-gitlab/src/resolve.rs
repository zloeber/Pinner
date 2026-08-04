use std::collections::HashMap;
use std::env;

use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin};
use pinner_iac_common::{
    parse_resolve_map, resolve_git_sha, resolve_image_digest, resolve_map_lookup,
};
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde_json::{Map, Value};

use crate::GitlabEcosystem;
use crate::extract::{include_ref, is_include_finding};

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
            pins.push(resolve_one(
                &runner,
                finding,
                ctx,
                &docker_map,
                &gitlab_map,
            )?);
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
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Gitlab
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Gitlab,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    if is_include_finding(finding) {
        resolve_include(runner, finding, ctx, gitlab_map)
    } else {
        resolve_image(runner, finding, ctx, docker_map, gitlab_map)
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
    use super::parse_bare_resolve_map;

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
}
