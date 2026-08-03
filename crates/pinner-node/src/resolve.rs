use std::collections::HashMap;
use std::path::Path;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin,
};
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde_json::Value;

use crate::NodeEcosystem;

impl NodeEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let mut pins = Vec::with_capacity(findings.len());
        // Cache lockfile parses by directory.
        let mut lock_cache: HashMap<std::path::PathBuf, Option<HashMap<String, String>>> =
            HashMap::new();

        for finding in findings {
            pins.push(resolve_one(&runner, finding, ctx, &mut lock_cache)?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    lock_cache: &mut HashMap<std::path::PathBuf, Option<HashMap<String, String>>>,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Node
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Node,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    let parent = finding
        .path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    if !lock_cache.contains_key(&parent) {
        let map = read_package_lock_versions(&parent.join("package-lock.json"))?;
        lock_cache.insert(parent.clone(), map);
    }

    if let Some(Some(versions)) = lock_cache.get(&parent)
        && let Some(version) = versions.get(&finding.name)
    {
        return Ok(Pin {
            ecosystem: EcosystemKind::Node,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: version.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::NativeLock,
            metadata: Default::default(),
        });
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let pinned = resolve_via_npm(runner, &finding.name).map_err(|hint| {
        EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        }
    })?;

    Ok(Pin {
        ecosystem: EcosystemKind::Node,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence: EvidenceKind::Registry,
        metadata: Default::default(),
    })
}

fn read_package_lock_versions(
    lock_path: &Path,
) -> Result<Option<HashMap<String, String>>, EcosystemError> {
    if !lock_path.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(lock_path)?;
    let value: Value = serde_json::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: lock_path.to_path_buf(),
        message: e.to_string(),
    })?;

    let Some(packages) = value.get("packages").and_then(|p| p.as_object()) else {
        return Ok(None);
    };

    let mut map = HashMap::new();
    for (key, entry) in packages {
        let Some(version) = entry.get("version").and_then(|v| v.as_str()) else {
            continue;
        };
        // Support both packages["ms"] and packages["node_modules/ms"].
        let name = key
            .strip_prefix("node_modules/")
            .unwrap_or(key.as_str());
        if !is_top_level_package_name(name) {
            continue;
        }
        map.insert(name.to_string(), version.to_string());
    }
    Ok(Some(map))
}

fn is_top_level_package_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if let Some(rest) = name.strip_prefix('@') {
        // Scoped: @scope/pkg
        return rest.matches('/').count() == 1 && !rest.starts_with('/') && !rest.ends_with('/');
    }
    // Unscoped top-level: no slash (skip nested node_modules paths).
    !name.contains('/')
}

fn resolve_via_npm(runner: &dyn CommandRunner, name: &str) -> Result<String, String> {
    let output = runner
        .run("npm", &["view", name, "version"])
        .map_err(|err| format!("npm view {name} version: {err}"))?;
    if output.status != 0 {
        return Err(format!(
            "npm view {name} version failed (status {}): {}",
            output.status,
            output.stderr.trim()
        ));
    }
    let version = output
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string();
    if version.is_empty() {
        return Err(format!("npm view {name} version returned empty output"));
    }
    Ok(version)
}
