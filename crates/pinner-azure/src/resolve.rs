use std::collections::HashMap;
use std::env;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
    upgrade_pin,
};
use pinner_iac_common::{parse_resolve_map, resolve_image_digest, resolve_map_lookup};
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde_json::{Map, Value};

use crate::AzureEcosystem;
use crate::extract::{is_exact_task_version, is_task_finding, parse_task_ref};

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
            if let Some(pin) = resolve_one(&runner, finding, ctx, &docker_map, &azure_map)? {
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
    azure_map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    if ctx.resolve_mode == ResolveMode::Upgrade {
        return resolve_upgrade(runner, finding, ctx, docker_map, azure_map);
    }

    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Azure
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Azure,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        }));
    }

    if is_task_finding(finding) {
        resolve_task(finding, ctx, azure_map).map(Some)
    } else {
        resolve_image(runner, finding, ctx, docker_map, azure_map).map(Some)
    }
}

fn resolve_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    docker_map: &HashMap<String, String>,
    azure_map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    if is_task_finding(finding) {
        resolve_task_upgrade(finding, ctx, azure_map)
    } else {
        resolve_image_upgrade(runner, finding, ctx, docker_map, azure_map)
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
        return Ok(azure_pin(finding, pinned, EvidenceKind::Registry, "image"));
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

fn resolve_image_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    docker_map: &HashMap<String, String>,
    azure_map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    let Some(inspect_ref) = upgrade_image_ref(&finding.requested) else {
        return Ok(None);
    };

    let previous = previous_for_image_upgrade(finding, ctx);

    if let Some(newest) = docker_map
        .get(&finding.requested)
        .cloned()
        .or_else(|| docker_map.get(&inspect_ref).cloned())
        .or_else(|| resolve_map_lookup(azure_map, &finding.name, &finding.requested))
        .or_else(|| azure_map.get(&finding.requested).cloned())
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

    // Gap: no Azure Marketplace / Visual Studio Marketplace HTTP resolver yet.
    Err(EcosystemError::Resolve {
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        hint: "set PINNER_AZURE_RESOLVE_MAP (Name@Name@Major=Name@x.y.z or Name@Major=x.y.z); Azure task marketplace HTTP upgrade is not implemented"
            .into(),
    })
}

fn resolve_task_upgrade(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    azure_map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    let previous = previous_for_task_upgrade(finding, ctx);

    if let Some(mapped) = resolve_map_lookup(azure_map, &finding.name, &finding.requested)
        .or_else(|| azure_map.get(&finding.requested).cloned())
    {
        let newest = normalize_task_pin(&finding.name, &mapped);
        return Ok(upgrade_kind_pin(
            finding,
            &previous,
            &newest,
            EvidenceKind::Registry,
            "map",
            "task",
        ));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    // Gap: map-only until marketplace/version HTTP exists (document in azure Gaps).
    Err(EcosystemError::Resolve {
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        hint: "set PINNER_AZURE_RESOLVE_MAP (Name@Name@Major=Name@x.y.z or Name@Major=x.y.z); Azure task marketplace HTTP upgrade is not implemented"
            .into(),
    })
}

/// Accept map values as `UseNode@1.2.3` or bare `1.2.3`.
fn normalize_task_pin(name: &str, pinned: &str) -> String {
    let pinned = pinned.trim();
    if let Some((n, ver)) = parse_task_ref(pinned) {
        return format!("{n}@{ver}");
    }
    format!("{name}@{pinned}")
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
        pin.ecosystem == EcosystemKind::Azure
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return lock.pinned.clone();
    }
    finding.requested.clone()
}

fn previous_for_task_upgrade(finding: &Finding, ctx: &EcosystemCtx<'_>) -> String {
    if let Some((_, ver)) = parse_task_ref(&finding.requested)
        && is_exact_task_version(ver)
    {
        return finding.requested.clone();
    }
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Azure
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
    use super::{normalize_task_pin, parse_bare_resolve_map, upgrade_image_ref};

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
        assert_eq!(normalize_task_pin("UseNode", "1.2.3"), "UseNode@1.2.3");
        assert_eq!(
            normalize_task_pin("UseNode", "UseNode@1.2.3"),
            "UseNode@1.2.3"
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
    }
}
