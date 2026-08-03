use std::collections::HashMap;
use std::env;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin,
};
use pinner_iac_common::parse_resolve_map;
use serde_json::{Map, Value};

use crate::HelmEcosystem;

impl HelmEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            pins.push(resolve_one(finding, ctx, &map)?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Helm
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Helm,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    if let Some(pinned) = map.get(&finding.requested) {
        return Ok(helm_pin(finding, pinned.clone(), EvidenceKind::Registry));
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
        hint: "set PINNER_HELM_RESOLVE_MAP (requested=pinned) for offline/tests, or enable network helm repo/OCI resolve".into(),
    })
}

fn helm_pin(finding: &Finding, pinned: String, evidence: EvidenceKind) -> Pin {
    let mut metadata = Map::new();
    metadata.insert("chart".into(), Value::String(finding.name.clone()));
    Pin {
        ecosystem: EcosystemKind::Helm,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence,
        metadata,
    }
}

fn resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_HELM_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_resolve_map(&raw)
}
