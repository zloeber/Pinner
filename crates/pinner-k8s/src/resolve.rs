use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
    upgrade_pin,
};
use pinner_iac_common::{parse_resolve_map, resolve_image_digest, resolve_map_lookup};
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde_json::{Map, Value};

use crate::K8sEcosystem;
use crate::extract::{image_tag, kind_lookup};

impl K8sEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let map = resolve_map_from_env();
        let kinds = kind_lookup(ctx.repo, findings)?;
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            if let Some(pin) = resolve_one(&runner, finding, ctx, &map, &kinds)? {
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
    map: &HashMap<String, String>,
    kinds: &HashMap<(PathBuf, String), String>,
) -> Result<Option<Pin>, EcosystemError> {
    let kind = kinds
        .get(&(finding.path.clone(), finding.requested.clone()))
        .cloned()
        .unwrap_or_default();

    if ctx.resolve_mode == ResolveMode::Upgrade {
        return resolve_upgrade(runner, finding, ctx, map, &kind);
    }

    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::K8s
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::K8s,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        }));
    }

    if let Some(pinned) = resolve_map_lookup(map, &finding.name, &finding.requested) {
        return Ok(Some(k8s_pin(finding, pinned, EvidenceKind::Registry, &kind)));
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
    Ok(Some(k8s_pin(finding, pinned, EvidenceKind::Tool, &kind)))
}

fn resolve_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
    kind: &str,
) -> Result<Option<Pin>, EcosystemError> {
    let Some(inspect_ref) = upgrade_image_ref(&finding.requested) else {
        return Ok(None);
    };

    let previous = previous_for_upgrade(finding, ctx);

    if let Some(newest) = resolve_map_lookup(map, &finding.name, &finding.requested)
        .or_else(|| map.get(&finding.requested).cloned())
        .or_else(|| map.get(&inspect_ref).cloned())
    {
        return Ok(upgrade_k8s_pin(
            finding, &previous, &newest, kind, EvidenceKind::Registry, "map",
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

    Ok(upgrade_k8s_pin(
        finding,
        &previous,
        &newest,
        kind,
        EvidenceKind::Tool,
        "docker",
    ))
}

fn upgrade_k8s_pin(
    finding: &Finding,
    previous: &str,
    newest: &str,
    kind: &str,
    evidence: EvidenceKind,
    channel: &str,
) -> Option<Pin> {
    let mut pin = upgrade_pin(finding, previous, newest, evidence, channel)?;
    pin.metadata.insert(
        "tag".into(),
        Value::String(image_tag(&finding.requested)),
    );
    pin.metadata
        .insert("kind".into(), Value::String(kind.to_string()));
    Some(pin)
}

fn previous_for_upgrade(finding: &Finding, ctx: &EcosystemCtx<'_>) -> String {
    if finding.requested.contains("@sha256:") {
        return finding.requested.clone();
    }
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::K8s
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return lock.pinned.clone();
    }
    finding.requested.clone()
}

/// Tag/name form to re-resolve. Digest-only (`name@sha256:…` without `:tag`) → None.
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

fn k8s_pin(finding: &Finding, pinned: String, evidence: EvidenceKind, kind: &str) -> Pin {
    let mut metadata = Map::new();
    metadata.insert("tag".into(), Value::String(image_tag(&finding.requested)));
    metadata.insert("kind".into(), Value::String(kind.to_string()));
    Pin {
        ecosystem: EcosystemKind::K8s,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence,
        metadata,
    }
}

fn resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_K8S_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_resolve_map(&raw)
}

#[cfg(test)]
mod tests {
    use super::upgrade_image_ref;

    #[test]
    fn upgrade_image_ref_skips_digest_only() {
        assert_eq!(
            upgrade_image_ref(
                "nginx@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            None
        );
        assert_eq!(
            upgrade_image_ref(
                "nginx:latest@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            )
            .as_deref(),
            Some("nginx:latest")
        );
        assert_eq!(
            upgrade_image_ref("nginx:latest").as_deref(),
            Some("nginx:latest")
        );
    }
}
