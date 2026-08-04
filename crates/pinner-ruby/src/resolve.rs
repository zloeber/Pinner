use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, absolute_in_repo,
};

use crate::RubyEcosystem;

impl RubyEcosystem {
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
        pin.ecosystem == EcosystemKind::Ruby
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Ruby,
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

    if let Some(version) = find_gemfile_lock_version(&dir, &finding.name, lock_cache)? {
        return Ok(Pin {
            ecosystem: EcosystemKind::Ruby,
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
            ecosystem: EcosystemKind::Ruby,
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
        hint: "set PINNER_RUBY_RESOLVE_MAP (name=requested:pinned) or provide Gemfile.lock".into(),
    })
}

fn find_gemfile_lock_version(
    start: &Path,
    name: &str,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Option<String>, EcosystemError> {
    let mut current = start.to_path_buf();
    loop {
        if !lock_cache.contains_key(&current) {
            let map = read_gemfile_lock_versions(&current.join("Gemfile.lock"))?;
            lock_cache.insert(current.clone(), map);
        }
        if let Some(Some(versions)) = lock_cache.get(&current)
            && let Some(version) = versions.get(name)
        {
            return Ok(Some(version.clone()));
        }
        if !current.pop() {
            break;
        }
    }
    Ok(None)
}

/// Parse `Gemfile.lock` SPECS: top-level `name (version)` entries under `specs:`.
fn read_gemfile_lock_versions(
    lock_path: &Path,
) -> Result<Option<HashMap<String, String>>, EcosystemError> {
    if !lock_path.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(lock_path)?;
    let mut map = HashMap::new();
    let mut in_specs = false;

    for raw in contents.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Section headers are unindented (GEM, PLATFORMS, DEPENDENCIES, …).
        if !raw.starts_with([' ', '\t']) && !trimmed.eq_ignore_ascii_case("specs:") {
            in_specs = false;
            continue;
        }

        if trimmed.eq_ignore_ascii_case("specs:") {
            in_specs = true;
            continue;
        }

        if !in_specs {
            continue;
        }

        // Top-level gems are indented once (2 or 4 spaces); nested deps deeper.
        let indent = raw.len() - raw.trim_start().len();
        if indent > 4 {
            continue;
        }

        let Some((name, version)) = parse_spec_entry(trimmed) else {
            continue;
        };
        map.entry(name).or_insert(version);
    }

    Ok(Some(map))
}

fn parse_spec_entry(line: &str) -> Option<(String, String)> {
    let open = line.rfind(" (")?;
    let close = line[open..].find(')')? + open;
    let name = line[..open].trim();
    let version = line[open + 2..close].trim();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name.to_string(), version.to_string()))
}

/// Parse `PINNER_RUBY_RESOLVE_MAP` entries shaped as `name=requested:pinned`
/// (comma- or newline-separated).
fn resolve_map_from_env() -> HashMap<(String, String), String> {
    let Ok(raw) = env::var("PINNER_RUBY_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_ruby_resolve_map(&raw)
}

fn parse_ruby_resolve_map(raw: &str) -> HashMap<(String, String), String> {
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
    use super::{parse_ruby_resolve_map, parse_spec_entry, read_gemfile_lock_versions};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parse_name_requested_pinned() {
        let map = parse_ruby_resolve_map("rake=:13.2.1,rspec=>= 3.0:3.13.0");
        assert_eq!(
            map.get(&("rake".into(), "".into())).map(String::as_str),
            Some("13.2.1")
        );
        assert_eq!(
            map.get(&("rspec".into(), ">= 3.0".into()))
                .map(String::as_str),
            Some("3.13.0")
        );
    }

    #[test]
    fn parses_spec_entry() {
        assert_eq!(
            parse_spec_entry("rake (13.2.1)"),
            Some(("rake".into(), "13.2.1".into()))
        );
        assert_eq!(
            parse_spec_entry("rspec-core (~> 3.13.0)"),
            Some(("rspec-core".into(), "~> 3.13.0".into()))
        );
    }

    #[test]
    fn reads_gemfile_lock_specs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Gemfile.lock");
        fs::write(
            &path,
            "GEM\n  remote: https://rubygems.org/\n  specs:\n    rake (13.2.1)\n      some-dep (~> 1.0)\n    rspec (3.13.0)\n\nPLATFORMS\n  ruby\n",
        )
        .unwrap();
        let map = read_gemfile_lock_versions(&path).unwrap().unwrap();
        assert_eq!(map.get("rake").map(String::as_str), Some("13.2.1"));
        assert_eq!(map.get("rspec").map(String::as_str), Some("3.13.0"));
        assert!(!map.contains_key("some-dep"));
    }
}
