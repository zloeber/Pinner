use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin,
};
use pinner_iac_common::{parse_resolve_map, resolve_image_digest};
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde_json::{Map, Value};

use crate::extract::{image_tag, kind_lookup};
use crate::K8sEcosystem;

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
            pins.push(resolve_one(&runner, finding, ctx, &map, &kinds)?);
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
) -> Result<Pin, EcosystemError> {
    let kind = kinds
        .get(&(finding.path.clone(), finding.requested.clone()))
        .cloned()
        .unwrap_or_default();

    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::K8s
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::K8s,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    if let Some(pinned) = map.get(&finding.requested) {
        return Ok(k8s_pin(
            finding,
            pinned.clone(),
            EvidenceKind::Registry,
            &kind,
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
    Ok(k8s_pin(finding, pinned, EvidenceKind::Tool, &kind))
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
