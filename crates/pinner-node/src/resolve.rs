use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
    absolute_in_repo, upgrade_pin,
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
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        // Cache lockfile parses by directory.
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
        pin.ecosystem == EcosystemKind::Node
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Node,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        }));
    }

    let abs_path = absolute_in_repo(ctx.repo, &finding.path);
    let parent = abs_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    if let Some(version) = lookup_native_lock_version(&parent, &finding.name, lock_cache)? {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Node,
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
            ecosystem: EcosystemKind::Node,
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

    let pinned =
        resolve_via_npm(runner, &finding.name).map_err(|hint| EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        })?;

    Ok(Some(Pin {
        ecosystem: EcosystemKind::Node,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence: EvidenceKind::Registry,
        metadata: Default::default(),
    }))
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
        resolve_via_npm(runner, &finding.name).map_err(|hint| EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        })?;

    Ok(upgrade_pin(
        finding,
        &previous,
        &newest,
        EvidenceKind::Registry,
        "npm",
    ))
}

/// Display-only previous version: exact requested, else native-lock peek, else requested.
fn previous_for_upgrade(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<String, EcosystemError> {
    if is_exact_node_version(&finding.requested) {
        return Ok(finding.requested.clone());
    }

    let abs_path = absolute_in_repo(ctx.repo, &finding.path);
    let parent = abs_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    if let Some(version) = lookup_native_lock_version(&parent, &finding.name, lock_cache)? {
        return Ok(version);
    }
    Ok(finding.requested.clone())
}

fn is_exact_node_version(requested: &str) -> bool {
    let requested = requested.trim();
    if requested.is_empty()
        || requested == "latest"
        || requested == "*"
        || requested.starts_with('^')
        || requested.starts_with('~')
        || requested.starts_with('>')
        || requested.starts_with('<')
        || requested.starts_with('=')
    {
        return false;
    }
    requested.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Parse `PINNER_NODE_RESOLVE_MAP` entries shaped as `name=requested:pinned`.
fn resolve_map_from_env() -> HashMap<(String, String), String> {
    let Ok(raw) = env::var("PINNER_NODE_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_node_resolve_map(&raw)
}

fn parse_node_resolve_map(raw: &str) -> HashMap<(String, String), String> {
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

fn lookup_native_lock_version(
    dir: &Path,
    name: &str,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Option<String>, EcosystemError> {
    if !lock_cache.contains_key(dir) {
        let map = read_node_lock_versions(dir)?;
        lock_cache.insert(dir.to_path_buf(), map);
    }
    Ok(lock_cache
        .get(dir)
        .and_then(|m| m.as_ref())
        .and_then(|versions| versions.get(name).cloned()))
}

/// Prefer package-lock.json, then pnpm-lock.yaml, then yarn.lock.
fn read_node_lock_versions(dir: &Path) -> Result<Option<HashMap<String, String>>, EcosystemError> {
    if let Some(map) = read_package_lock_versions(&dir.join("package-lock.json"))? {
        return Ok(Some(map));
    }
    if let Some(map) = read_pnpm_lock_versions(&dir.join("pnpm-lock.yaml"))? {
        return Ok(Some(map));
    }
    if let Some(map) = read_yarn_lock_versions(&dir.join("yarn.lock"))? {
        return Ok(Some(map));
    }
    Ok(None)
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
        let name = key.strip_prefix("node_modules/").unwrap_or(key.as_str());
        if !is_top_level_package_name(name) {
            continue;
        }
        map.insert(name.to_string(), version.to_string());
    }
    Ok(Some(map))
}

/// Minimal pnpm-lock.yaml parser: read `packages:` keys shaped like
/// `ms@2.1.3`, `/ms@2.1.3`, or `/ms/2.1.3`.
fn read_pnpm_lock_versions(
    lock_path: &Path,
) -> Result<Option<HashMap<String, String>>, EcosystemError> {
    if !lock_path.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(lock_path)?;
    let value: serde_yaml::Value =
        serde_yaml::from_str(&contents).map_err(|e| EcosystemError::Parse {
            path: lock_path.to_path_buf(),
            message: e.to_string(),
        })?;

    let Some(packages) = value.get("packages").and_then(|p| p.as_mapping()) else {
        return Ok(None);
    };

    let mut map = HashMap::new();
    for (key, _entry) in packages {
        let Some(key) = key.as_str() else {
            continue;
        };
        if let Some((name, version)) = parse_pnpm_package_key(key) {
            map.entry(name).or_insert(version);
        }
    }
    Ok(Some(map))
}

fn parse_pnpm_package_key(key: &str) -> Option<(String, String)> {
    let key = key.trim().trim_start_matches('/');
    if key.is_empty() {
        return None;
    }
    // Scoped: @scope/name@version or @scope/name/version
    if let Some(rest) = key.strip_prefix('@') {
        if let Some((scope_name, version)) = rest.rsplit_once('@')
            && scope_name.contains('/')
        {
            return Some((format!("@{scope_name}"), version.to_string()));
        }
        // @scope/name/1.2.3
        let (scope_name, version) = rest.rsplit_once('/')?;
        if looks_like_version(version) && scope_name.contains('/') {
            return Some((format!("@{scope_name}"), version.to_string()));
        }
        return None;
    }

    if let Some((name, version)) = key.rsplit_once('@')
        && is_top_level_package_name(name)
        && looks_like_version(version)
    {
        return Some((name.to_string(), version.to_string()));
    }
    // name/1.2.3
    if let Some((name, version)) = key.rsplit_once('/')
        && is_top_level_package_name(name)
        && looks_like_version(version)
    {
        return Some((name.to_string(), version.to_string()));
    }
    None
}

fn looks_like_version(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty() && s.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Parse yarn.lock (classic v1 and berry-style `version:` / `version "..."`).
fn read_yarn_lock_versions(
    lock_path: &Path,
) -> Result<Option<HashMap<String, String>>, EcosystemError> {
    if !lock_path.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(lock_path)?;
    let mut map = HashMap::new();
    let mut pending_names: Vec<String> = Vec::new();

    for raw in contents.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Descriptor line(s): `ms@^2.0.0:` or `"ms@npm:^2.0.0", "ms@npm:latest":`
        if !line.starts_with(' ') && !line.starts_with('\t') && line.ends_with(':') {
            pending_names = parse_yarn_descriptor_names(&line[..line.len() - 1]);
            continue;
        }
        let trimmed = line.trim_start();
        if let Some(version) = parse_yarn_version_line(trimmed) {
            for name in pending_names.drain(..) {
                map.entry(name).or_insert(version.clone());
            }
        }
    }
    Ok(Some(map))
}

fn parse_yarn_descriptor_names(descriptor: &str) -> Vec<String> {
    let mut names = Vec::new();
    for part in descriptor.split(',') {
        let part = part.trim().trim_matches('"').trim_matches('\'');
        if part.is_empty() {
            continue;
        }
        // Strip berry `npm:` protocol: ms@npm:^2.0.0 → ms
        let without_proto = part.replace("@npm:", "@");
        if let Some(name) = yarn_package_name(&without_proto) {
            names.push(name);
        }
    }
    names
}

fn yarn_package_name(descriptor: &str) -> Option<String> {
    // @scope/name@range
    if let Some(rest) = descriptor.strip_prefix('@') {
        let (scope_name, _) = rest.rsplit_once('@')?;
        return Some(format!("@{scope_name}"));
    }
    let (name, _) = descriptor.split_once('@')?;
    Some(name.to_string())
}

fn parse_yarn_version_line(trimmed: &str) -> Option<String> {
    if let Some(rest) = trimmed.strip_prefix("version ") {
        // classic: version "2.1.3"
        let v = rest.trim().trim_matches('"').trim_matches('\'');
        return (!v.is_empty()).then(|| v.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("version:") {
        // berry: version: 2.1.3
        let v = rest.trim().trim_matches('"').trim_matches('\'');
        return (!v.is_empty()).then(|| v.to_string());
    }
    None
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

#[cfg(test)]
mod tests {
    use super::{
        parse_node_resolve_map, parse_pnpm_package_key, parse_yarn_descriptor_names,
        parse_yarn_version_line, read_pnpm_lock_versions, read_yarn_lock_versions,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parse_name_requested_pinned() {
        let map = parse_node_resolve_map("ms=2.1.3:2.1.4,left-pad=^1.3.0:1.3.0");
        assert_eq!(
            map.get(&("ms".into(), "2.1.3".into())).map(String::as_str),
            Some("2.1.4")
        );
        assert_eq!(
            map.get(&("left-pad".into(), "^1.3.0".into()))
                .map(String::as_str),
            Some("1.3.0")
        );
    }

    #[test]
    fn parses_pnpm_package_keys() {
        assert_eq!(
            parse_pnpm_package_key("ms@2.1.3"),
            Some(("ms".into(), "2.1.3".into()))
        );
        assert_eq!(
            parse_pnpm_package_key("/ms@2.1.3"),
            Some(("ms".into(), "2.1.3".into()))
        );
        assert_eq!(
            parse_pnpm_package_key("/ms/2.1.3"),
            Some(("ms".into(), "2.1.3".into()))
        );
        assert_eq!(
            parse_pnpm_package_key("@scope/pkg@1.0.0"),
            Some(("@scope/pkg".into(), "1.0.0".into()))
        );
    }

    #[test]
    fn reads_pnpm_lock_fixture() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pnpm-lock.yaml");
        fs::write(
            &path,
            "lockfileVersion: '9.0'\npackages:\n  ms@2.1.3:\n    resolution: {integrity: sha512-abc}\n  left-pad@1.3.0:\n    resolution: {integrity: sha512-def}\n",
        )
        .unwrap();
        let map = read_pnpm_lock_versions(&path).unwrap().unwrap();
        assert_eq!(map.get("ms").map(String::as_str), Some("2.1.3"));
        assert_eq!(map.get("left-pad").map(String::as_str), Some("1.3.0"));
    }

    #[test]
    fn reads_yarn_classic_and_berry() {
        assert_eq!(
            parse_yarn_descriptor_names(r#"ms@^2.0.0, ms@latest"#),
            vec!["ms".to_string(), "ms".to_string()]
        );
        assert_eq!(
            parse_yarn_descriptor_names(r#""ms@npm:^2.0.0""#),
            vec!["ms".to_string()]
        );
        assert_eq!(
            parse_yarn_version_line(r#"version "2.1.3""#).as_deref(),
            Some("2.1.3")
        );
        assert_eq!(
            parse_yarn_version_line("version: 2.1.3").as_deref(),
            Some("2.1.3")
        );

        let dir = tempdir().unwrap();
        let classic = dir.path().join("yarn.lock");
        fs::write(
            &classic,
            "# yarn lockfile v1\n\nms@^2.0.0, ms@latest:\n  version \"2.1.3\"\n  resolved \"https://registry.yarnpkg.com/ms/-/ms-2.1.3.tgz\"\n",
        )
        .unwrap();
        let map = read_yarn_lock_versions(&classic).unwrap().unwrap();
        assert_eq!(map.get("ms").map(String::as_str), Some("2.1.3"));

        let berry = dir.path().join("yarn-berry.lock");
        fs::write(
            &berry,
            "\"ms@npm:^2.0.0\":\n  version: 2.1.3\n  resolution: \"ms@npm:2.1.3\"\n",
        )
        .unwrap();
        // reuse reader on berry-shaped file
        let map = read_yarn_lock_versions(&berry).unwrap().unwrap();
        assert_eq!(map.get("ms").map(String::as_str), Some("2.1.3"));
    }
}
