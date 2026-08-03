use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, absolute_in_repo,
};
use pinner_iac_common::{parse_resolve_map, resolve_map_lookup};
use serde::Deserialize;
use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use crate::HelmEcosystem;

impl HelmEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let map = resolve_map_from_env();
        let mut repos = RepositoryQueue::load(ctx.repo, findings)?;
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            let repository = repos.take(finding);
            pins.push(resolve_one(finding, ctx, &map, repository)?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
    repository: String,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Helm
            && pin.name == finding.name
            && pin.requested == finding.requested
            && repository_matches(pin, &repository)
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

    if let Some(pinned) = resolve_map_lookup(map, &finding.name, &finding.requested) {
        return Ok(helm_pin(
            finding,
            pinned,
            EvidenceKind::Registry,
            repository,
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
        hint: "set PINNER_HELM_RESOLVE_MAP (name@requested=pinned) for offline/tests; Helm repo/OCI HTTP resolve is not implemented".into(),
    })
}

fn helm_pin(finding: &Finding, pinned: String, evidence: EvidenceKind, repository: String) -> Pin {
    let mut metadata = Map::new();
    metadata.insert("chart".into(), Value::String(finding.name.clone()));
    metadata.insert("repository".into(), Value::String(repository));
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

fn repository_matches(pin: &Pin, repository: &str) -> bool {
    match pin.metadata.get("repository").and_then(|v| v.as_str()) {
        Some(repo) => repo == repository,
        None => repository.is_empty(),
    }
}

fn resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_HELM_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_resolve_map(&raw)
}

/// Ordered (name, requested, repository) rows per manifest so same-named charts
/// from different repos stay distinct when assigning pin metadata.
struct RepositoryQueue {
    by_path: HashMap<PathBuf, Vec<(String, String, String)>>,
}

impl RepositoryQueue {
    fn load(repo: &Path, findings: &[Finding]) -> Result<Self, EcosystemError> {
        let mut by_path = HashMap::new();
        let mut seen = std::collections::HashSet::new();
        for finding in findings {
            if !seen.insert(finding.path.clone()) {
                continue;
            }
            let abs = absolute_in_repo(repo, &finding.path);
            let rows = load_repository_rows(&abs)?;
            by_path.insert(finding.path.clone(), rows);
        }
        Ok(Self { by_path })
    }

    fn take(&mut self, finding: &Finding) -> String {
        let Some(rows) = self.by_path.get_mut(&finding.path) else {
            return String::new();
        };
        if let Some(i) = rows.iter().position(|(name, requested, _)| {
            name == &finding.name && requested == &finding.requested
        }) {
            return rows.remove(i).2;
        }
        String::new()
    }
}

fn load_repository_rows(path: &Path) -> Result<Vec<(String, String, String)>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if matches!(file_name, "Chart.yaml" | "Chart.yml") {
        return chart_yaml_rows(&contents, path);
    }

    gitops_rows(&contents, path)
}

fn chart_yaml_rows(
    contents: &str,
    path: &Path,
) -> Result<Vec<(String, String, String)>, EcosystemError> {
    let value: YamlValue = serde_yaml::from_str(contents).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let mut rows = Vec::new();
    let Some(deps) = value.get("dependencies").and_then(|d| d.as_sequence()) else {
        return Ok(rows);
    };
    for dep in deps {
        let Some(name) = dep.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        let requested = dep
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        let repository = dep
            .get("repository")
            .and_then(|r| r.as_str())
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        rows.push((name.to_string(), requested, repository));
    }
    Ok(rows)
}

fn gitops_rows(
    contents: &str,
    path: &Path,
) -> Result<Vec<(String, String, String)>, EcosystemError> {
    let mut rows = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(contents) {
        let value = YamlValue::deserialize(doc).map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        if let Some(row) = gitops_row(&value) {
            rows.push(row);
        }
    }
    Ok(rows)
}

fn gitops_row(value: &YamlValue) -> Option<(String, String, String)> {
    let kind = value.get("kind")?.as_str()?;
    match kind {
        "HelmRelease" => {
            let chart_spec = value.get("spec")?.get("chart")?.get("spec")?;
            let name = chart_spec.get("chart")?.as_str()?.to_string();
            let requested = chart_spec
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let repository = chart_spec
                .get("sourceRef")
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            Some((name, requested, repository))
        }
        "Application" => {
            let source = value.get("spec")?.get("source")?;
            let name = source.get("chart")?.as_str()?.to_string();
            let requested = source
                .get("targetRevision")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let repository = source
                .get("repoURL")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            Some((name, requested, repository))
        }
        _ => None,
    }
}
