use std::collections::HashMap;
use std::env;

use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin};
use pinner_iac_common::{
    parse_resolve_map, resolve_image_digest, resolve_map_lookup,
};
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde_json::{Map, Value};

use crate::AzureEcosystem;
use crate::extract::is_task_finding;

impl AzureEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let docker_map = docker_resolve_map_from_env();
        let azure_map = azure_resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            pins.push(resolve_one(
                &runner,
                finding,
                ctx,
                &docker_map,
                &azure_map,
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
    azure_map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Azure
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Azure,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    if is_task_finding(finding) {
        resolve_task(finding, ctx, azure_map)
    } else {
        resolve_image(runner, finding, ctx, docker_map, azure_map)
    }
}

fn resolve_image(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    docker_map: &HashMap<String, String>,
    azure_map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    if let Some(pinned) = docker_map.get(&finding.requested) {
        return Ok(azure_pin(
            finding,
            pinned.clone(),
            EvidenceKind::Registry,
            "image",
        ));
    }

    if let Some(pinned) = resolve_map_lookup(azure_map, &finding.name, &finding.requested) {
        return Ok(azure_pin(
            finding,
            pinned,
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
    Ok(azure_pin(finding, pinned, EvidenceKind::Tool, "image"))
}

fn resolve_task(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    azure_map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    if let Some(pinned) = resolve_map_lookup(azure_map, &finding.name, &finding.requested) {
        return Ok(azure_pin(
            finding,
            normalize_task_pin(&finding.name, &pinned),
            EvidenceKind::Registry,
            "task",
        ));
    }
    if let Some(pinned) = azure_map.get(&finding.requested) {
        return Ok(azure_pin(
            finding,
            normalize_task_pin(&finding.name, pinned),
            EvidenceKind::Registry,
            "task",
        ));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    Err(EcosystemError::Resolve {
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        hint: "set PINNER_AZURE_RESOLVE_MAP (Name@Name@Major=Name@x.y.z or Name@Major=x.y.z)"
            .into(),
    })
}

/// Accept map values as `UseNode@1.2.3` or bare `1.2.3`.
fn normalize_task_pin(name: &str, pinned: &str) -> String {
    let pinned = pinned.trim();
    if let Some((n, ver)) = crate::extract::parse_task_ref(pinned) {
        return format!("{n}@{ver}");
    }
    format!("{name}@{pinned}")
}

fn azure_pin(finding: &Finding, pinned: String, evidence: EvidenceKind, kind: &str) -> Pin {
    let mut metadata = Map::new();
    metadata.insert("kind".into(), Value::String(kind.to_string()));
    Pin {
        ecosystem: EcosystemKind::Azure,
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

fn azure_resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_AZURE_RESOLVE_MAP") else {
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
    use super::{normalize_task_pin, parse_bare_resolve_map};

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
    fn normalize_task_pin_accepts_bare_or_full() {
        assert_eq!(
            normalize_task_pin("UseNode", "1.2.3"),
            "UseNode@1.2.3"
        );
        assert_eq!(
            normalize_task_pin("UseNode", "UseNode@1.2.3"),
            "UseNode@1.2.3"
        );
    }
}
