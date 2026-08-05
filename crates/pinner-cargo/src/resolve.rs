use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
    absolute_in_repo, upgrade_pin,
};
use pinner_iac_common::http_get;
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde_json::Value;

use crate::CargoEcosystem;

impl CargoEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        let mut lock_cache: HashMap<PathBuf, Option<HashMap<String, String>>> = HashMap::new();

        for finding in findings {
            if let Some(pin) = resolve_one(&runner, finding, ctx, &map, &mut lock_cache)? {
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
    map: &HashMap<(String, String), String>,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Option<Pin>, EcosystemError> {
    if ctx.resolve_mode == ResolveMode::Upgrade {
        return resolve_upgrade(runner, finding, ctx, map, lock_cache);
    }

    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Cargo
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Cargo,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        }));
    }

    let abs_path = absolute_in_repo(ctx.repo, &finding.path);
    let dir = abs_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    if let Some(version) = find_cargo_lock_version(ctx.repo, &dir, &finding.name, lock_cache)? {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Cargo,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: version,
            path: finding.path.clone(),
            evidence: EvidenceKind::NativeLock,
            metadata: Default::default(),
        }));
    }

    if let Some(pinned) = map
        .get(&(finding.name.clone(), finding.requested.clone()))
        .cloned()
    {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Cargo,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned,
            path: finding.path.clone(),
            evidence: EvidenceKind::Registry,
            metadata: Default::default(),
        }));
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

fn resolve_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<(String, String), String>,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Option<Pin>, EcosystemError> {
    let previous = previous_for_upgrade(finding, ctx, lock_cache)?;

    if let Some(newest) = map
        .get(&(finding.name.clone(), finding.requested.clone()))
        .cloned()
    {
        return Ok(upgrade_pin(
            finding,
            &previous,
            &newest,
            EvidenceKind::Registry,
            "map",
        ));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let newest =
        resolve_crates_io_max_version(finding, &|url| http_get(runner, url)).map_err(|hint| {
            EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint,
            }
        })?;

    Ok(upgrade_pin(
        finding,
        &previous,
        &newest,
        EvidenceKind::Registry,
        "crates.io",
    ))
}

/// Display-only previous version: exact requested, else native-lock peek, else requested.
fn previous_for_upgrade(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<String, EcosystemError> {
    if is_exact_looking_cargo(&finding.requested) {
        return Ok(finding.requested.clone());
    }

    let abs_path = absolute_in_repo(ctx.repo, &finding.path);
    let dir = abs_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    if let Some(version) = find_cargo_lock_version(ctx.repo, &dir, &finding.name, lock_cache)? {
        return Ok(version);
    }
    Ok(finding.requested.clone())
}

/// crates.io `/api/v1/crates/{name}` → `crate.max_version`.
pub fn resolve_crates_io_max_version<F>(
    finding: &Finding,
    http_get_fn: &F,
) -> Result<String, String>
where
    F: Fn(&str) -> Result<String, String>,
{
    let url = format!("https://crates.io/api/v1/crates/{}", finding.name);
    let body = http_get_fn(&url)?;
    parse_crates_io_max_version(&body).ok_or_else(|| {
        format!(
            "crates.io response missing max_version for {}",
            finding.name
        )
    })
}

fn parse_crates_io_max_version(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    value
        .get("crate")?
        .get("max_version")?
        .as_str()
        .map(str::to_string)
}

fn is_exact_looking_cargo(requested: &str) -> bool {
    let r = requested.trim();
    if r.is_empty() || r == "*" || r.eq_ignore_ascii_case("latest") {
        return false;
    }
    if r.starts_with(['^', '~', '>', '<', '=', '*']) {
        return false;
    }
    // Exact x.y.z (three numeric parts).
    let mut parts = 0u8;
    for part in r.split('.') {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        parts += 1;
    }
    parts == 3
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
    use super::{
        find_cargo_lock_version, is_exact_looking_cargo, parse_cargo_resolve_map,
        parse_crates_io_max_version, read_cargo_lock_versions, resolve_crates_io_max_version,
    };
    use pinner_ecosystem::{EcosystemKind, Finding};
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

    #[test]
    fn exact_looking_cargo_versions() {
        assert!(is_exact_looking_cargo("1.0.200"));
        assert!(!is_exact_looking_cargo("1"));
        assert!(!is_exact_looking_cargo("^1"));
        assert!(!is_exact_looking_cargo("1.0"));
    }

    #[test]
    fn parses_crates_io_max_version() {
        let body = r#"{"crate":{"name":"serde","max_version":"1.0.210"}}"#;
        assert_eq!(
            parse_crates_io_max_version(body).as_deref(),
            Some("1.0.210")
        );
    }

    #[test]
    fn resolve_crates_io_uses_injected_http() {
        let finding = Finding {
            ecosystem: EcosystemKind::Cargo,
            name: "serde".into(),
            requested: "1.0.200".into(),
            path: PathBuf::from("Cargo.toml"),
            is_floating: false,
        };
        let pinned = resolve_crates_io_max_version(&finding, &|url| {
            assert!(url.ends_with("/crates/serde"));
            Ok(r#"{"crate":{"max_version":"1.0.219"}}"#.into())
        })
        .unwrap();
        assert_eq!(pinned, "1.0.219");
    }
}
