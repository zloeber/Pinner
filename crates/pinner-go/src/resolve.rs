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

use crate::GoEcosystem;

impl GoEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        let mut sum_cache: HashMap<PathBuf, Option<HashMap<String, String>>> = HashMap::new();

        for finding in findings {
            if let Some(pin) = resolve_one(&runner, finding, ctx, &map, &mut sum_cache)? {
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
    sum_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Option<Pin>, EcosystemError> {
    if ctx.resolve_mode == ResolveMode::Upgrade {
        return resolve_upgrade(runner, finding, ctx, map, sum_cache);
    }

    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Go
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Go,
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

    if let Some(version) = find_go_sum_version(ctx.repo, &dir, &finding.name, sum_cache)? {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Go,
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
            ecosystem: EcosystemKind::Go,
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
        hint: "set PINNER_GO_RESOLVE_MAP (name=requested:pinned) or provide go.sum".into(),
    })
}

fn resolve_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<(String, String), String>,
    sum_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Option<Pin>, EcosystemError> {
    let previous = previous_for_upgrade(finding, ctx, sum_cache)?;

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

    let (newest, evidence, channel) = match resolve_via_go_list(runner, &finding.name) {
        Ok(version) => (version, EvidenceKind::Tool, "go-list"),
        Err(go_err) => {
            let version = resolve_proxy_golang_latest(&finding.name, &|url| http_get(runner, url))
                .map_err(|proxy_err| EcosystemError::Resolve {
                    name: finding.name.clone(),
                    requested: finding.requested.clone(),
                    hint: format!(
                        "go list failed ({go_err}); proxy.golang.org failed ({proxy_err})"
                    ),
                })?;
            (version, EvidenceKind::Registry, "proxy.golang.org")
        }
    };

    Ok(upgrade_pin(finding, &previous, &newest, evidence, channel))
}

/// Display-only previous version: exact requested, else go.sum peek, else requested.
fn previous_for_upgrade(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    sum_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<String, EcosystemError> {
    if is_exact_looking_go(&finding.requested) {
        return Ok(finding.requested.clone());
    }

    let abs_path = absolute_in_repo(ctx.repo, &finding.path);
    let dir = abs_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    if let Some(version) = find_go_sum_version(ctx.repo, &dir, &finding.name, sum_cache)? {
        return Ok(version);
    }
    Ok(finding.requested.clone())
}

fn resolve_via_go_list(runner: &dyn CommandRunner, module: &str) -> Result<String, String> {
    let probe = runner
        .run("go", &["version"])
        .map_err(|err| format!("go not available: {err}"))?;
    if probe.status != 0 {
        return Err(format!(
            "go not available (status {}): {}",
            probe.status,
            probe.stderr.trim()
        ));
    }

    let query = format!("{module}@latest");
    let output = runner
        .run("go", &["list", "-m", "-u", "-json", &query])
        .map_err(|err| format!("go list: {err}"))?;
    if output.status != 0 {
        return Err(format!(
            "go list failed (status {}): {}",
            output.status,
            output.stderr.trim()
        ));
    }
    parse_go_list_version(&output.stdout)
        .ok_or_else(|| format!("go list JSON missing Version for {module}"))
}

fn parse_go_list_version(stdout: &str) -> Option<String> {
    let value: Value = serde_json::from_str(stdout.trim()).ok()?;
    if let Some(update) = value.get("Update")
        && let Some(version) = update.get("Version").and_then(|v| v.as_str())
    {
        return Some(version.to_string());
    }
    value
        .get("Version")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// proxy.golang.org `/{module}/@latest` → `Version`.
pub fn resolve_proxy_golang_latest<F>(module: &str, http_get_fn: &F) -> Result<String, String>
where
    F: Fn(&str) -> Result<String, String>,
{
    let escaped = escape_module_path(module);
    let url = format!("https://proxy.golang.org/{escaped}/@latest");
    let body = http_get_fn(&url)?;
    parse_proxy_golang_version(&body)
        .ok_or_else(|| format!("proxy.golang.org response missing Version for {module}"))
}

fn parse_proxy_golang_version(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    value
        .get("Version")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Case-encode module path for the module proxy (uppercase → `!` + lowercase).
fn escape_module_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_ascii_uppercase() {
            out.push('!');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn is_exact_looking_go(requested: &str) -> bool {
    let r = requested.trim();
    if r.is_empty() || r.eq_ignore_ascii_case("latest") {
        return false;
    }
    // Pseudo-versions and semver tags start with `v`.
    r.starts_with('v')
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
    use super::{
        escape_module_path, find_go_sum_version, parse_go_list_version, parse_go_resolve_map,
        parse_proxy_golang_version, read_go_sum_versions, resolve_proxy_golang_latest,
    };
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

    #[test]
    fn escape_encodes_uppercase() {
        assert_eq!(
            escape_module_path("github.com/Azure/go-autorest"),
            "github.com/!azure/go-autorest"
        );
    }

    #[test]
    fn parses_go_list_prefers_update() {
        let body = r#"{"Path":"m","Version":"v1.0.0","Update":{"Path":"m","Version":"v1.1.0"}}"#;
        assert_eq!(parse_go_list_version(body).as_deref(), Some("v1.1.0"));
        let latest = r#"{"Path":"m","Version":"v2.0.0"}"#;
        assert_eq!(parse_go_list_version(latest).as_deref(), Some("v2.0.0"));
    }

    #[test]
    fn resolve_proxy_uses_injected_http() {
        let pinned = resolve_proxy_golang_latest("github.com/example/lib", &|url| {
            assert!(url.contains("github.com/example/lib/@latest"));
            Ok(r#"{"Version":"v1.9.9"}"#.into())
        })
        .unwrap();
        assert_eq!(pinned, "v1.9.9");
        assert_eq!(
            parse_proxy_golang_version(r#"{"Version":"v1.2.3"}"#).as_deref(),
            Some("v1.2.3")
        );
    }
}
