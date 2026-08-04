use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, absolute_in_repo,
};

use crate::GoEcosystem;

impl GoEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        let mut sum_cache: HashMap<PathBuf, Option<HashMap<String, String>>> = HashMap::new();

        for finding in findings {
            pins.push(resolve_one(finding, ctx, &map, &mut sum_cache)?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<(String, String), String>,
    sum_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Go
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Go,
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

    if let Some(version) = find_go_sum_version(ctx.repo, &dir, &finding.name, sum_cache)? {
        return Ok(Pin {
            ecosystem: EcosystemKind::Go,
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
            ecosystem: EcosystemKind::Go,
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
        hint: "set PINNER_GO_RESOLVE_MAP (name=requested:pinned) or provide go.sum".into(),
    })
}

fn find_go_sum_version(
    repo: &Path,
    start: &Path,
    name: &str,
    sum_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Option<String>, EcosystemError> {
    let mut current = start.to_path_buf();
    loop {
        if !sum_cache.contains_key(&current) {
            let map = read_go_sum_versions(&current.join("go.sum"))?;
            sum_cache.insert(current.clone(), map);
        }
        if let Some(Some(versions)) = sum_cache.get(&current)
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

/// Parse `go.sum`: first version seen for each module path wins.
fn read_go_sum_versions(
    sum_path: &Path,
) -> Result<Option<HashMap<String, String>>, EcosystemError> {
    if !sum_path.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(sum_path)?;
    let mut map = HashMap::new();
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(module_path) = parts.next() else {
            continue;
        };
        let Some(version_token) = parts.next() else {
            continue;
        };
        // go.sum versions may be `v1.2.3` or `v1.2.3/go.mod` — strip the suffix.
        let version = version_token
            .strip_suffix("/go.mod")
            .unwrap_or(version_token);
        map.entry(module_path.to_string())
            .or_insert_with(|| version.to_string());
    }
    Ok(Some(map))
}

/// Parse `PINNER_GO_RESOLVE_MAP` entries shaped as `name=requested:pinned`
/// (comma- or newline-separated).
fn resolve_map_from_env() -> HashMap<(String, String), String> {
    let Ok(raw) = env::var("PINNER_GO_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_go_resolve_map(&raw)
}

fn parse_go_resolve_map(raw: &str) -> HashMap<(String, String), String> {
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
    use super::{find_go_sum_version, parse_go_resolve_map, read_go_sum_versions};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn parse_name_requested_pinned() {
        let map = parse_go_resolve_map("github.com/example/lib=latest:v1.2.3");
        assert_eq!(
            map.get(&("github.com/example/lib".into(), "latest".into()))
                .map(String::as_str),
            Some("v1.2.3")
        );
    }

    #[test]
    fn reads_go_sum_first_version() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("go.sum");
        fs::write(
            &path,
            "github.com/example/lib v1.2.3 h1:abc=\ngithub.com/example/lib v1.2.3/go.mod h1:def=\ngithub.com/example/lib v9.9.9 h1:old=\n",
        )
        .unwrap();
        let map = read_go_sum_versions(&path).unwrap().unwrap();
        assert_eq!(
            map.get("github.com/example/lib").map(String::as_str),
            Some("v1.2.3")
        );
    }

    #[test]
    fn find_go_sum_ignores_parent_outside_repo() {
        let outer = tempdir().unwrap();
        let repo = outer.path().join("repo");
        let sub = repo.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            outer.path().join("go.sum"),
            "github.com/example/lib v9.9.9 h1:parent=\n",
        )
        .unwrap();
        fs::write(
            repo.join("go.sum"),
            "github.com/example/lib v1.2.3 h1:repo=\n",
        )
        .unwrap();

        let mut cache = HashMap::<PathBuf, _>::new();
        let version = find_go_sum_version(&repo, &sub, "github.com/example/lib", &mut cache)
            .unwrap()
            .unwrap();
        assert_eq!(version, "v1.2.3");
    }

    #[test]
    fn find_go_sum_stops_at_repo_root_without_sum() {
        let outer = tempdir().unwrap();
        let repo = outer.path().join("repo");
        let sub = repo.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            outer.path().join("go.sum"),
            "github.com/example/lib v9.9.9 h1:parent=\n",
        )
        .unwrap();

        let mut cache = HashMap::<PathBuf, _>::new();
        assert!(
            find_go_sum_version(&repo, &sub, "github.com/example/lib", &mut cache)
                .unwrap()
                .is_none()
        );
    }
}
