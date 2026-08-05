use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use hcl_edit::structure::Body;
use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
    absolute_in_repo, upgrade_pin,
};
use pinner_iac_common::{
    http_get, parse_resolve_map, resolve_git_sha, resolve_map_lookup, select_matching_version,
};
use pinner_toolchain::{CommandRunner, RealCommandRunner};
use serde_json::Value;

use crate::TerraformEcosystem;
use crate::git_source::{git_pinned_source, git_ref_from_source, is_git_or_http_requested};

impl TerraformEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let map = resolve_map_from_env();
        let mut sources = ModuleSourceQueue::load(ctx.repo, findings)?;
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            let module_source = sources.take(finding);
            if let Some(pin) = resolve_one(&runner, finding, ctx, &map, module_source)? {
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
    map: &HashMap<String, String>,
    module_source: String,
) -> Result<Option<Pin>, EcosystemError> {
    if ctx.resolve_mode == ResolveMode::Upgrade {
        return resolve_upgrade(runner, finding, ctx, map, module_source);
    }

    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Terraform
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Some(Pin {
            ecosystem: EcosystemKind::Terraform,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        }));
    }

    // Native provider lock before env map so `.terraform.lock.hcl` wins when both exist.
    if is_provider_finding(finding)
        && let Some(pinned) = resolve_from_terraform_lock(ctx.repo, finding)
    {
        return Ok(Some(registry_pin(
            finding,
            pinned,
            EvidenceKind::NativeLock,
        )));
    }

    if let Some(pinned) = resolve_map_lookup(map, &finding.name, &finding.requested) {
        return Ok(Some(registry_pin(finding, pinned, EvidenceKind::Registry)));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    if is_git_or_http_requested(&finding.requested) {
        let (repo_url, ref_name) =
            parse_git_source(&finding.requested).ok_or_else(|| EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint: "could not parse git module source for resolve".into(),
            })?;
        let pinned = resolve_git_sha(runner, &repo_url, &ref_name).map_err(|hint| {
            EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint,
            }
        })?;
        return Ok(Some(registry_pin(finding, pinned, EvidenceKind::Tool)));
    }

    let pinned = if is_provider_finding(finding) {
        resolve_terraform_registry_provider(&finding.name, &finding.requested, &|url| {
            http_get(runner, url)
        })
    } else {
        let source = if module_source.is_empty() {
            finding.name.clone()
        } else {
            module_source
        };
        resolve_terraform_registry_module(&source, &finding.requested, &|url| http_get(runner, url))
    }
    .map_err(|hint| EcosystemError::Resolve {
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        hint,
    })?;

    Ok(Some(registry_pin(finding, pinned, EvidenceKind::Registry)))
}

fn resolve_upgrade(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
    module_source: String,
) -> Result<Option<Pin>, EcosystemError> {
    let previous = previous_for_upgrade(finding, ctx);

    if let Some(mapped) = resolve_map_lookup(map, &finding.name, &finding.requested) {
        let newest = normalize_upgrade_pinned(finding, mapped);
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

    let newest = if is_git_or_http_requested(&finding.requested) {
        let repo_url =
            parse_git_repo_url(&finding.requested).ok_or_else(|| EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint: "could not parse git module source for upgrade".into(),
            })?;
        let sha = resolve_git_sha(runner, &repo_url, "HEAD").map_err(|hint| {
            EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint,
            }
        })?;
        git_pinned_source(&finding.requested, &sha)
    } else if is_provider_finding(finding) {
        resolve_terraform_registry_provider(&finding.name, "*", &|url| http_get(runner, url))
            .map_err(|hint| EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint,
            })?
    } else {
        let source = if module_source.is_empty() {
            finding.name.clone()
        } else {
            module_source
        };
        resolve_terraform_registry_module(&source, "*", &|url| http_get(runner, url)).map_err(
            |hint| EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint,
            },
        )?
    };

    let evidence = if is_git_or_http_requested(&finding.requested) {
        EvidenceKind::Tool
    } else {
        EvidenceKind::Registry
    };
    let channel = if is_git_or_http_requested(&finding.requested) {
        "git"
    } else {
        "registry"
    };

    Ok(upgrade_pin(
        finding, &previous, &newest, evidence, channel,
    ))
}

fn normalize_upgrade_pinned(finding: &Finding, pinned: String) -> String {
    if is_git_or_http_requested(&finding.requested) {
        // Map may store bare SHA or a full source URL with ref=.
        let sha = git_ref_from_source(&pinned).unwrap_or(pinned.as_str());
        return git_pinned_source(&finding.requested, sha);
    }
    pinned
}

fn previous_for_upgrade(finding: &Finding, ctx: &EcosystemCtx<'_>) -> String {
    if is_git_or_http_requested(&finding.requested) {
        if let Some(ref_) = git_ref_from_source(&finding.requested)
            && is_full_git_sha(ref_)
        {
            return git_pinned_source(&finding.requested, ref_);
        }
        if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
            pin.ecosystem == EcosystemKind::Terraform
                && pin.name == finding.name
                && pin.requested == finding.requested
        }) {
            return lock.pinned.clone();
        }
        return finding.requested.clone();
    }

    if is_exact_version_constraint(&finding.requested) {
        return finding.requested.clone();
    }

    // Display-only peeks — never choose native/lock as the upgrade pin.
    if is_provider_finding(finding)
        && let Some(pinned) = resolve_from_terraform_lock(ctx.repo, finding)
    {
        return pinned;
    }
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Terraform
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return lock.pinned.clone();
    }
    finding.requested.clone()
}

fn is_full_git_sha(ref_: &str) -> bool {
    let ref_ = ref_.trim();
    ref_.len() == 40 && ref_.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_exact_version_constraint(version: &str) -> bool {
    let version = version.trim();
    if version.is_empty() {
        return false;
    }
    let version = version.strip_prefix('=').map(str::trim).unwrap_or(version);
    is_exact_semver(version)
}

fn is_exact_semver(version: &str) -> bool {
    let version = version.trim();
    if version.is_empty() {
        return false;
    }
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

fn registry_pin(finding: &Finding, pinned: String, evidence: EvidenceKind) -> Pin {
    let pinned = if is_git_or_http_requested(&finding.requested) {
        git_pinned_source(&finding.requested, &pinned)
    } else {
        pinned
    };
    Pin {
        ecosystem: EcosystemKind::Terraform,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence,
        metadata: Default::default(),
    }
}

fn resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_TERRAFORM_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_resolve_map(&raw)
}

/// Provider findings use registry-style `namespace/name` (or hostname-qualified) sources.
fn is_provider_finding(finding: &Finding) -> bool {
    !is_git_or_http_requested(&finding.requested) && finding.name.contains('/')
}

fn parse_git_source(source: &str) -> Option<(String, String)> {
    let (repo_url, query) = parse_git_url_parts(source)?;
    let mut ref_name = None;
    for part in query.split('&') {
        if let Some(value) = part.strip_prefix("ref=") {
            ref_name = Some(value.to_string());
            break;
        }
    }
    let ref_name = ref_name?;
    Some((repo_url, ref_name))
}

/// Repo URL for upgrade HEAD resolve (ref query optional).
fn parse_git_repo_url(source: &str) -> Option<String> {
    if let Some((url, _)) = parse_git_url_parts(source) {
        return Some(url);
    }
    let source = source.trim();
    let without_git = source.strip_prefix("git::").unwrap_or(source);
    let url_part = without_git
        .split_once('?')
        .map(|(u, _)| u)
        .unwrap_or(without_git);
    if url_part.is_empty() {
        return None;
    }
    Some(normalize_git_url(url_part))
}

fn parse_git_url_parts(source: &str) -> Option<(String, &str)> {
    let source = source.trim();
    let without_git = source.strip_prefix("git::").unwrap_or(source);
    let (url_part, query) = without_git.split_once('?')?;
    Some((normalize_git_url(url_part), query))
}

fn normalize_git_url(url_part: &str) -> String {
    if url_part.starts_with("github.com/")
        || url_part.starts_with("gitlab.com/")
        || url_part.starts_with("bitbucket.org/")
    {
        format!("https://{url_part}")
    } else {
        url_part.to_string()
    }
}

fn resolve_from_terraform_lock(repo: &Path, finding: &Finding) -> Option<String> {
    let lock_path = find_terraform_lock(repo, &finding.path)?;
    let contents = std::fs::read_to_string(&lock_path).ok()?;
    let body = Body::from_str(&contents).ok()?;
    for block in body.get_blocks("provider") {
        let label = block.labels.first()?.as_str();
        if !provider_label_matches(label, &finding.name) {
            continue;
        }
        if let Some(constraints) = attr_str(&block.body, "constraints")
            && !constraints.is_empty()
            && constraints != finding.requested
        {
            continue;
        }
        if let Some(version) = attr_str(&block.body, "version") {
            return Some(version);
        }
    }
    None
}

fn find_terraform_lock(repo: &Path, finding_rel: &Path) -> Option<PathBuf> {
    let abs = absolute_in_repo(repo, finding_rel);
    let mut dir = abs.parent().unwrap_or(repo).to_path_buf();
    loop {
        let candidate = dir.join(".terraform.lock.hcl");
        if candidate.is_file() {
            return Some(candidate);
        }
        if dir == repo {
            break;
        }
        if !dir.pop() {
            break;
        }
    }
    let root = repo.join(".terraform.lock.hcl");
    root.is_file().then_some(root)
}

fn provider_label_matches(label: &str, finding_name: &str) -> bool {
    label == finding_name
        || label.ends_with(&format!("/{finding_name}"))
        || label.ends_with(finding_name)
}

fn attr_str(body: &Body, key: &str) -> Option<String> {
    body.get_attribute(key)
        .and_then(|attr| attr.value.as_str().map(str::to_string))
}

/// Resolve a Terraform module version from the public registry API.
///
/// `source` is `namespace/name/provider` (e.g. `terraform-aws-modules/vpc/aws`).
/// Prefer `PINNER_TERRAFORM_RESOLVE_MAP` offline; inject `http_get_fn` in unit tests.
pub fn resolve_terraform_registry_module<F>(
    source: &str,
    requested: &str,
    http_get_fn: &F,
) -> Result<String, String>
where
    F: Fn(&str) -> Result<String, String>,
{
    let path = normalize_module_source(source)?;
    let url = format!("https://registry.terraform.io/v1/modules/{path}/versions");
    let body = http_get_fn(&url)?;
    let versions = parse_module_versions(&body)?;
    select_matching_version(&versions, requested).ok_or_else(|| {
        format!(
            "no terraform module version for {source} matching {requested:?}; set PINNER_TERRAFORM_RESOLVE_MAP"
        )
    })
}

/// Resolve a Terraform provider version from the public registry API.
///
/// `name` is `namespace/type` (e.g. `hashicorp/aws`).
pub fn resolve_terraform_registry_provider<F>(
    name: &str,
    requested: &str,
    http_get_fn: &F,
) -> Result<String, String>
where
    F: Fn(&str) -> Result<String, String>,
{
    let path = normalize_provider_name(name)?;
    let url = format!("https://registry.terraform.io/v1/providers/{path}/versions");
    let body = http_get_fn(&url)?;
    let versions = parse_provider_versions(&body)?;
    select_matching_version(&versions, requested).ok_or_else(|| {
        format!(
            "no terraform provider version for {name} matching {requested:?}; set PINNER_TERRAFORM_RESOLVE_MAP"
        )
    })
}

fn normalize_module_source(source: &str) -> Result<String, String> {
    let source = source.trim().trim_start_matches("registry.terraform.io/");
    let parts: Vec<_> = source.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() != 3 {
        return Err(format!(
            "expected module source namespace/name/provider, got {source:?}; set PINNER_TERRAFORM_RESOLVE_MAP"
        ));
    }
    Ok(parts.join("/"))
}

fn normalize_provider_name(name: &str) -> Result<String, String> {
    let name = name.trim().trim_start_matches("registry.terraform.io/");
    let parts: Vec<_> = name.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() != 2 {
        return Err(format!(
            "expected provider namespace/type, got {name:?}; set PINNER_TERRAFORM_RESOLVE_MAP"
        ));
    }
    Ok(parts.join("/"))
}

fn parse_module_versions(body: &str) -> Result<Vec<String>, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|e| format!("terraform module versions JSON: {e}"))?;
    let modules = value
        .get("modules")
        .and_then(|m| m.as_array())
        .ok_or_else(|| "terraform module versions missing modules[]".to_string())?;
    let mut versions = Vec::new();
    for module in modules {
        let Some(vs) = module.get("versions").and_then(|v| v.as_array()) else {
            continue;
        };
        for v in vs {
            if let Some(s) = v.get("version").and_then(|x| x.as_str()) {
                versions.push(s.to_string());
            }
        }
    }
    if versions.is_empty() {
        return Err("terraform module versions list empty".into());
    }
    Ok(versions)
}

fn parse_provider_versions(body: &str) -> Result<Vec<String>, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|e| format!("terraform provider versions JSON: {e}"))?;
    let vs = value
        .get("versions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "terraform provider versions missing versions[]".to_string())?;
    let mut versions = Vec::new();
    for v in vs {
        if let Some(s) = v.get("version").and_then(|x| x.as_str()) {
            versions.push(s.to_string());
        }
    }
    if versions.is_empty() {
        return Err("terraform provider versions list empty".into());
    }
    Ok(versions)
}

/// Module block label → registry `source` for HTTP resolve.
struct ModuleSourceQueue {
    by_path: HashMap<PathBuf, Vec<(String, String, String)>>,
}

impl ModuleSourceQueue {
    fn load(repo: &Path, findings: &[Finding]) -> Result<Self, EcosystemError> {
        let mut by_path = HashMap::new();
        let mut seen = std::collections::HashSet::new();
        for finding in findings {
            if is_provider_finding(finding) || is_git_or_http_requested(&finding.requested) {
                continue;
            }
            if !seen.insert(finding.path.clone()) {
                continue;
            }
            let abs = absolute_in_repo(repo, &finding.path);
            let rows = load_module_sources(&abs)?;
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

fn load_module_sources(path: &Path) -> Result<Vec<(String, String, String)>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let body = Body::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let mut rows = Vec::new();
    for block in body.get_blocks("module") {
        let Some(label) = block.labels.first().map(|l| l.as_str().to_string()) else {
            continue;
        };
        let Some(source) = attr_str(&block.body, "source") else {
            continue;
        };
        if source.starts_with('.') || source.starts_with('/') || is_git_or_http_requested(&source) {
            continue;
        }
        let requested = attr_str(&block.body, "version").unwrap_or_default();
        rows.push((label, requested, source));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_module_source, parse_git_source, parse_module_versions, provider_label_matches,
        resolve_terraform_registry_module,
    };

    #[test]
    fn parse_git_source_strips_prefix_and_query() {
        assert_eq!(
            parse_git_source("git::https://example.com/org/mod.git?ref=main"),
            Some(("https://example.com/org/mod.git".into(), "main".into()))
        );
    }

    #[test]
    fn provider_label_matches_registry_host() {
        assert!(provider_label_matches(
            "registry.terraform.io/hashicorp/aws",
            "hashicorp/aws"
        ));
        assert!(provider_label_matches("hashicorp/aws", "hashicorp/aws"));
        assert!(!provider_label_matches(
            "registry.terraform.io/hashicorp/azurerm",
            "hashicorp/aws"
        ));
    }

    #[test]
    fn normalize_module_source_strips_host() {
        assert_eq!(
            normalize_module_source("registry.terraform.io/terraform-aws-modules/vpc/aws").unwrap(),
            "terraform-aws-modules/vpc/aws"
        );
    }

    #[test]
    fn parse_module_versions_json() {
        let body = r#"{"modules":[{"versions":[{"version":"1.0.0"},{"version":"1.1.0"}]}]}"#;
        assert_eq!(
            parse_module_versions(body).unwrap(),
            vec!["1.0.0".to_string(), "1.1.0".to_string()]
        );
    }

    #[test]
    fn resolve_module_with_fixture_http() {
        let body = r#"{"modules":[{"versions":[{"version":"5.0.0"},{"version":"5.2.0"}]}]}"#;
        let v =
            resolve_terraform_registry_module("ns/name/prov", "~> 5.0", &|_| Ok(body.to_string()))
                .unwrap();
        assert_eq!(v, "5.2.0");
    }

    #[test]
    fn resolve_module_latest_unconstrained() {
        let body = r#"{"modules":[{"versions":[{"version":"5.2.0"},{"version":"6.0.0"}]}]}"#;
        let v = resolve_terraform_registry_module("ns/name/prov", "*", &|_| Ok(body.to_string()))
            .unwrap();
        assert_eq!(v, "6.0.0");
    }
}
