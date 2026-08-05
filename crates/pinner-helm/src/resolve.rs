use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
    absolute_in_repo, upgrade_pin,
};
use pinner_iac_common::{
    http_get, parse_resolve_map, resolve_image_digest, resolve_map_lookup, select_matching_version,
};
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde::Deserialize;
use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use crate::HelmEcosystem;
use crate::discover::is_values_file;

impl HelmEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let map = resolve_map_from_env();
        let mut repos = RepositoryQueue::load(ctx.repo, findings)?;
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            if is_values_file(&finding.path) {
                if let Some(pin) = resolve_image_one(&runner, finding, ctx, &map)? {
                    pins.push(pin);
                }
            } else {
                let repository = repos.take(finding);
                if let Some(pin) = resolve_chart_one(&runner, finding, ctx, &map, repository)? {
                    pins.push(pin);
                }
            }
        }
        Ok(pins)
    }
}

fn resolve_image_one(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    if ctx.resolve_mode == ResolveMode::Upgrade {
        return resolve_image_upgrade(runner, finding, ctx, map);
    }

    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Helm
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Helm,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        }));
    }

    if let Some(pinned) = resolve_map_lookup(map, &finding.name, &finding.requested) {
        return Ok(Some(image_pin(finding, pinned, EvidenceKind::Registry)));
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
    Ok(Some(image_pin(finding, pinned, EvidenceKind::Tool)))
}

fn resolve_image_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
) -> Result<Option<Pin>, EcosystemError> {
    let Some(inspect_ref) = upgrade_image_ref(&finding.requested) else {
        return Ok(None);
    };

    let previous = previous_for_image_upgrade(finding, ctx);

    if let Some(newest) = resolve_map_lookup(map, &finding.name, &finding.requested)
        .or_else(|| map.get(&finding.requested).cloned())
        .or_else(|| map.get(&inspect_ref).cloned())
    {
        return Ok(upgrade_image_pin(finding, &previous, &newest, "map"));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let newest =
        resolve_image_digest(runner, &inspect_ref).map_err(|hint| EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        })?;

    Ok(upgrade_image_pin(finding, &previous, &newest, "docker"))
}

fn resolve_chart_one(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
    repository: String,
) -> Result<Option<Pin>, EcosystemError> {
    if ctx.resolve_mode == ResolveMode::Upgrade {
        return resolve_chart_upgrade(runner, finding, ctx, map, repository);
    }

    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Helm
            && pin.name == finding.name
            && pin.requested == finding.requested
            && repository_matches(pin, &repository)
    }) {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Helm,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        }));
    }

    if let Some(pinned) = resolve_map_lookup(map, &finding.name, &finding.requested) {
        return Ok(Some(helm_pin(
            finding,
            pinned,
            EvidenceKind::Registry,
            repository,
        )));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let pinned = resolve_helm_chart_version(runner, &finding.name, &finding.requested, &repository)
        .map_err(|hint| EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        })?;
    Ok(Some(helm_pin(
        finding,
        pinned,
        EvidenceKind::Registry,
        repository,
    )))
}

fn resolve_chart_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
    repository: String,
) -> Result<Option<Pin>, EcosystemError> {
    let previous = previous_for_chart_upgrade(finding, ctx, &repository);

    if let Some(newest) = resolve_map_lookup(map, &finding.name, &finding.requested) {
        return Ok(upgrade_chart_pin(
            finding,
            &previous,
            &newest,
            &repository,
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
        resolve_helm_chart_version(runner, &finding.name, "*", &repository).map_err(|hint| {
            EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint,
            }
        })?;

    Ok(upgrade_chart_pin(
        finding,
        &previous,
        &newest,
        &repository,
        "registry",
    ))
}

fn upgrade_chart_pin(
    finding: &Finding,
    previous: &str,
    newest: &str,
    repository: &str,
    channel: &str,
) -> Option<Pin> {
    let mut pin = upgrade_pin(finding, previous, newest, EvidenceKind::Registry, channel)?;
    pin.metadata
        .insert("chart".into(), Value::String(finding.name.clone()));
    pin.metadata
        .insert("repository".into(), Value::String(repository.to_string()));
    Some(pin)
}

fn upgrade_image_pin(
    finding: &Finding,
    previous: &str,
    newest: &str,
    channel: &str,
) -> Option<Pin> {
    let evidence = if channel == "docker" {
        EvidenceKind::Tool
    } else {
        EvidenceKind::Registry
    };
    let mut pin = upgrade_pin(finding, previous, newest, evidence, channel)?;
    pin.metadata
        .insert("kind".into(), Value::String("image".into()));
    Some(pin)
}

fn previous_for_chart_upgrade(
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    repository: &str,
) -> String {
    if is_exact_chart_version(&finding.requested) {
        return finding.requested.clone();
    }
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Helm
            && pin.name == finding.name
            && pin.requested == finding.requested
            && repository_matches(pin, repository)
    }) {
        return lock.pinned.clone();
    }
    finding.requested.clone()
}

fn previous_for_image_upgrade(finding: &Finding, ctx: &EcosystemCtx<'_>) -> String {
    if finding.requested.contains("@sha256:") {
        return finding.requested.clone();
    }
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Helm
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return lock.pinned.clone();
    }
    finding.requested.clone()
}

fn is_exact_chart_version(version: &str) -> bool {
    let version = version.trim();
    if version.is_empty() || version == "*" || version.eq_ignore_ascii_case("latest") {
        return false;
    }
    let version = version.strip_prefix('=').map(str::trim).unwrap_or(version);
    let bytes = version.as_bytes();
    let mut i = 0;
    if !consume_digits(bytes, &mut i) {
        return false;
    }
    if i >= bytes.len() || bytes[i] != b'.' {
        return false;
    }
    i += 1;
    if !consume_digits(bytes, &mut i) {
        return false;
    }
    if i >= bytes.len() || bytes[i] != b'.' {
        return false;
    }
    i += 1;
    if !consume_digits(bytes, &mut i) {
        return false;
    }
    i == bytes.len()
}

fn consume_digits(bytes: &[u8], i: &mut usize) -> bool {
    let start = *i;
    while *i < bytes.len() && bytes[*i].is_ascii_digit() {
        *i += 1;
    }
    *i > start
}

/// Tag/name form to re-resolve. Digest-only (`name@sha256:…` without `:tag`) → None.
fn upgrade_image_ref(requested: &str) -> Option<String> {
    let requested = requested.trim();
    if requested.is_empty() {
        return None;
    }
    if let Some((left, _digest)) = requested.split_once("@sha256:") {
        if image_has_tag(left) {
            return Some(left.to_string());
        }
        return None;
    }
    Some(requested.to_string())
}

fn image_has_tag(image: &str) -> bool {
    let after_slash = image.rfind('/').map(|i| i + 1).unwrap_or(0);
    image[after_slash..].contains(':')
}

fn image_pin(finding: &Finding, pinned: String, evidence: EvidenceKind) -> Pin {
    let mut metadata = Map::new();
    metadata.insert("kind".into(), Value::String("image".into()));
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

/// Resolve a chart version from an HTTP repo index or OCI tags list.
///
/// Prefer `PINNER_HELM_RESOLVE_MAP` in tests/offline. Network callers inject via
/// [`resolve_helm_chart_version_with`].
pub fn resolve_helm_chart_version(
    runner: &dyn CommandRunner,
    chart: &str,
    requested: &str,
    repository: &str,
) -> Result<String, String> {
    resolve_helm_chart_version_with(chart, requested, repository, &|url| http_get(runner, url))
}

pub fn resolve_helm_chart_version_with<F>(
    chart: &str,
    requested: &str,
    repository: &str,
    http_get_fn: &F,
) -> Result<String, String>
where
    F: Fn(&str) -> Result<String, String>,
{
    let repository = repository.trim();
    if repository.is_empty() {
        return Err(format!(
            "no chart repository for {chart}; set PINNER_HELM_RESOLVE_MAP (name@requested=pinned)"
        ));
    }

    if let Some(oci) = repository.strip_prefix("oci://") {
        return resolve_oci_chart_tag(chart, requested, oci, http_get_fn);
    }

    if !(repository.starts_with("http://") || repository.starts_with("https://")) {
        return Err(format!(
            "unsupported helm repository {repository}; set PINNER_HELM_RESOLVE_MAP"
        ));
    }

    let index_url = format!("{}/index.yaml", repository.trim_end_matches('/'));
    let body = http_get_fn(&index_url)?;
    let versions = parse_helm_index_versions(&body, chart)?;
    select_matching_version(&versions, requested).ok_or_else(|| {
        format!(
            "no helm chart version for {chart} matching {requested:?} in {index_url}; set PINNER_HELM_RESOLVE_MAP"
        )
    })
}

fn resolve_oci_chart_tag<F>(
    chart: &str,
    requested: &str,
    oci_ref: &str,
    http_get_fn: &F,
) -> Result<String, String>
where
    F: Fn(&str) -> Result<String, String>,
{
    // oci://ghcr.io/org/charts/mychart → https://ghcr.io/v2/org/charts/mychart/tags/list
    let oci_ref = oci_ref.trim().trim_end_matches('/');
    let (host, path) = oci_ref.split_once('/').ok_or_else(|| {
        format!("invalid oci repository oci://{oci_ref}; set PINNER_HELM_RESOLVE_MAP")
    })?;
    let path = if path.ends_with(chart) {
        path.to_string()
    } else {
        format!("{path}/{chart}")
    };
    let tags_url = format!("https://{host}/v2/{path}/tags/list");
    let body = http_get_fn(&tags_url)?;
    let versions = parse_oci_tags(&body)?;
    select_matching_version(&versions, requested).ok_or_else(|| {
        format!(
            "no OCI tag for {chart} matching {requested:?} at {tags_url}; set PINNER_HELM_RESOLVE_MAP"
        )
    })
}

fn parse_helm_index_versions(index_yaml: &str, chart: &str) -> Result<Vec<String>, String> {
    let value: YamlValue =
        serde_yaml::from_str(index_yaml).map_err(|e| format!("helm index.yaml parse: {e}"))?;
    let entries = value
        .get("entries")
        .and_then(|e| e.as_mapping())
        .ok_or_else(|| "helm index.yaml missing entries".to_string())?;
    let chart_entries = entries
        .get(YamlValue::String(chart.to_string()))
        .and_then(|v| v.as_sequence())
        .ok_or_else(|| format!("helm index.yaml has no entry for chart {chart}"))?;
    let mut versions = Vec::new();
    for entry in chart_entries {
        if let Some(v) = entry.get("version").and_then(|v| v.as_str()) {
            versions.push(v.to_string());
        }
    }
    if versions.is_empty() {
        return Err(format!("helm index.yaml entry for {chart} has no versions"));
    }
    Ok(versions)
}

fn parse_oci_tags(body: &str) -> Result<Vec<String>, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|e| format!("OCI tags list parse: {e}"))?;
    let tags = value
        .get("tags")
        .and_then(|t| t.as_array())
        .ok_or_else(|| "OCI tags list missing tags array".to_string())?;
    let mut versions = Vec::new();
    for tag in tags {
        if let Some(s) = tag.as_str() {
            versions.push(s.to_string());
        }
    }
    if versions.is_empty() {
        return Err("OCI tags list empty".into());
    }
    Ok(versions)
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
            if is_values_file(&finding.path) {
                continue;
            }
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

#[cfg(test)]
mod tests {
    use super::{
        parse_helm_index_versions, parse_oci_tags, resolve_helm_chart_version_with,
        upgrade_image_ref,
    };

    #[test]
    fn parses_helm_index_versions() {
        let index = r#"
apiVersion: v1
entries:
  redis:
    - version: 18.6.1
    - version: 17.0.0
"#;
        let versions = parse_helm_index_versions(index, "redis").unwrap();
        assert_eq!(versions, vec!["18.6.1", "17.0.0"]);
    }

    #[test]
    fn resolves_chart_via_injected_http_index() {
        let index = r#"
apiVersion: v1
entries:
  redis:
    - version: 18.6.1
    - version: 17.3.0
"#;
        let pinned = resolve_helm_chart_version_with(
            "redis",
            "*",
            "https://charts.example.com/bitnami",
            &|url| {
                assert!(url.ends_with("/index.yaml"));
                Ok(index.to_string())
            },
        )
        .unwrap();
        assert_eq!(pinned, "18.6.1");
    }

    #[test]
    fn resolves_oci_via_injected_tags_list() {
        let body = r#"{"name":"org/charts/redis","tags":["17.0.0","18.6.1"]}"#;
        let pinned = resolve_helm_chart_version_with(
            "redis",
            "^18.0.0",
            "oci://ghcr.io/org/charts",
            &|_url| Ok(body.to_string()),
        )
        .unwrap();
        assert_eq!(pinned, "18.6.1");
    }

    #[test]
    fn parses_oci_tags() {
        let tags = parse_oci_tags(r#"{"tags":["1.0.0","2.0.0"]}"#).unwrap();
        assert_eq!(tags, vec!["1.0.0", "2.0.0"]);
    }

    #[test]
    fn upgrade_image_ref_skips_digest_only() {
        assert_eq!(
            upgrade_image_ref(
                "nginx@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            None
        );
        assert_eq!(
            upgrade_image_ref("nginx:latest").as_deref(),
            Some("nginx:latest")
        );
    }
}
