use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, absolute_in_repo,
};

use crate::CargoEcosystem;

impl CargoEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        let mut lock_cache: HashMap<PathBuf, Option<HashMap<String, String>>> = HashMap::new();

        for finding in findings {
            pins.push(resolve_one(finding, ctx, &map, &mut lock_cache)?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<(String, String), String>,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Cargo
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Cargo,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    let abs_path = absolute_in_repo(ctx.repo, &finding.path);
    let dir = abs_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    if let Some(version) =
        find_cargo_lock_version(ctx.repo, &dir, &finding.name, lock_cache)?
    {
        return Ok(Pin {
            ecosystem: EcosystemKind::Cargo,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: version,
            path: finding.path.clone(),
            evidence: EvidenceKind::NativeLock,
            metadata: Default::default(),
        });
    }

    if let Some(pinned) = map
        .get(&(finding.name.clone(), finding.requested.clone()))
        .cloned()
    {
        return Ok(Pin {
            ecosystem: EcosystemKind::Cargo,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned,
            path: finding.path.clone(),
            evidence: EvidenceKind::Registry,
            metadata: Default::default(),
        });
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
        hint: "set PINNER_CARGO_RESOLVE_MAP (name=requested:pinned) or provide Cargo.lock".into(),
    })
}

fn find_cargo_lock_version(
    repo: &Path,
    start: &Path,
    name: &str,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Option<String>, EcosystemError> {
    let mut current = start.to_path_buf();
    loop {
        if !lock_cache.contains_key(&current) {
            let map = read_cargo_lock_versions(&current.join("Cargo.lock"))?;
            lock_cache.insert(current.clone(), map);
        }
        if let Some(Some(versions)) = lock_cache.get(&current)
            && let Some(version) = versions.get(name)
        {
            return Ok(Some(version.clone()));
        }
        if current == repo {
            break;
        }
        if !current.pop() {
            break;
        }
    }
    Ok(None)
}

fn read_cargo_lock_versions(
    lock_path: &Path,
) -> Result<Option<HashMap<String, String>>, EcosystemError> {
    if !lock_path.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(lock_path)?;
    let value: toml::Value = toml::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: lock_path.to_path_buf(),
        message: e.to_string(),
    })?;

    let Some(packages) = value.get("package").and_then(|p| p.as_array()) else {
        return Ok(None);
    };

    let mut map = HashMap::new();
    for entry in packages {
        let Some(table) = entry.as_table() else {
            continue;
        };
        let Some(name) = table.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(version) = table.get("version").and_then(|v| v.as_str()) else {
            continue;
        };
        // Prefer first occurrence (workspace root packages typically listed once).
        map.entry(name.to_string())
            .or_insert_with(|| version.to_string());
    }
    Ok(Some(map))
}

/// Parse `PINNER_CARGO_RESOLVE_MAP` entries shaped as `name=requested:pinned`
/// (comma- or newline-separated).
fn resolve_map_from_env() -> HashMap<(String, String), String> {
    let Ok(raw) = env::var("PINNER_CARGO_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_cargo_resolve_map(&raw)
}

fn parse_cargo_resolve_map(raw: &str) -> HashMap<(String, String), String> {
    let mut map = HashMap::new();
    for entry in raw.split([',', '\n']) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((name, rest)) = entry.split_once('=') else {
            continue;
        };
        let Some((requested, pinned)) = rest.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let requested = requested.trim();
        let pinned = pinned.trim();
        if !name.is_empty() && !pinned.is_empty() {
            map.insert(
                (name.to_string(), requested.to_string()),
                pinned.to_string(),
            );
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::{find_cargo_lock_version, parse_cargo_resolve_map, read_cargo_lock_versions};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn parse_name_requested_pinned() {
        let map = parse_cargo_resolve_map("serde=1:1.0.210,tokio=^1:1.40.0");
        assert_eq!(
            map.get(&("serde".into(), "1".into())).map(String::as_str),
            Some("1.0.210")
        );
        assert_eq!(
            map.get(&("tokio".into(), "^1".into())).map(String::as_str),
            Some("1.40.0")
        );
    }

    #[test]
    fn reads_cargo_lock_packages() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Cargo.lock");
        fs::write(
            &path,
            "version = 3\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.210\"\n",
        )
        .unwrap();
        let map = read_cargo_lock_versions(&path).unwrap().unwrap();
        assert_eq!(map.get("serde").map(String::as_str), Some("1.0.210"));
    }

    #[test]
    fn find_cargo_lock_ignores_parent_outside_repo() {
        let outer = tempdir().unwrap();
        let repo = outer.path().join("repo");
        let sub = repo.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            outer.path().join("Cargo.lock"),
            "version = 3\n\n[[package]]\nname = \"serde\"\nversion = \"9.9.9\"\n",
        )
        .unwrap();
        fs::write(
            repo.join("Cargo.lock"),
            "version = 3\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.210\"\n",
        )
        .unwrap();

        let mut cache = HashMap::<PathBuf, _>::new();
        let version = find_cargo_lock_version(&repo, &sub, "serde", &mut cache)
            .unwrap()
            .unwrap();
        assert_eq!(version, "1.0.210");
    }

    #[test]
    fn find_cargo_lock_stops_at_repo_root_without_lock() {
        let outer = tempdir().unwrap();
        let repo = outer.path().join("repo");
        let sub = repo.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            outer.path().join("Cargo.lock"),
            "version = 3\n\n[[package]]\nname = \"serde\"\nversion = \"9.9.9\"\n",
        )
        .unwrap();

        let mut cache = HashMap::<PathBuf, _>::new();
        assert!(
            find_cargo_lock_version(&repo, &sub, "serde", &mut cache)
                .unwrap()
                .is_none()
        );
    }
}
