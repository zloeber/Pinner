use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use hcl_edit::structure::Body;
use pinner_ecosystem::{
    absolute_in_repo, EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin,
};
use pinner_iac_common::{parse_resolve_map, resolve_git_sha};
use pinner_toolchain::{CommandRunner, RealCommandRunner};

use crate::TerraformEcosystem;

impl TerraformEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            pins.push(resolve_one(&runner, finding, ctx, &map)?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Terraform
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Terraform,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    if let Some(pinned) = map.get(&finding.requested) {
        return Ok(registry_pin(finding, pinned.clone(), EvidenceKind::Registry));
    }

    if is_provider_finding(finding)
        && let Some(pinned) = resolve_from_terraform_lock(ctx.repo, finding)
    {
        return Ok(registry_pin(finding, pinned, EvidenceKind::NativeLock));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    if is_git_or_http_requested(&finding.requested) {
        let (repo_url, ref_name) = parse_git_source(&finding.requested).ok_or_else(|| {
            EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint: "could not parse git module source for resolve".into(),
            }
        })?;
        let pinned = resolve_git_sha(runner, &repo_url, &ref_name).map_err(|hint| {
            EcosystemError::Resolve {
                name: finding.name.clone(),
                requested: finding.requested.clone(),
                hint,
            }
        })?;
        return Ok(registry_pin(finding, pinned, EvidenceKind::Tool));
    }

    Err(EcosystemError::Resolve {
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        hint: "set PINNER_TERRAFORM_RESOLVE_MAP (requested=pinned) for offline/tests, or enable network registry resolve".into(),
    })
}

fn registry_pin(finding: &Finding, pinned: String, evidence: EvidenceKind) -> Pin {
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

fn is_git_or_http_requested(requested: &str) -> bool {
    let s = requested.trim();
    s.starts_with("git::")
        || s.starts_with("git@")
        || s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("github.com/")
        || s.starts_with("bitbucket.org/")
        || s.starts_with("gitlab.com/")
}

/// Provider findings use registry-style `namespace/name` (or hostname-qualified) sources.
fn is_provider_finding(finding: &Finding) -> bool {
    !is_git_or_http_requested(&finding.requested) && finding.name.contains('/')
}

fn parse_git_source(source: &str) -> Option<(String, String)> {
    let source = source.trim();
    let without_git = source.strip_prefix("git::").unwrap_or(source);
    let (url_part, query) = without_git.split_once('?')?;
    let mut ref_name = None;
    for part in query.split('&') {
        if let Some(value) = part.strip_prefix("ref=") {
            ref_name = Some(value.to_string());
            break;
        }
    }
    let ref_name = ref_name?;
    let repo_url = if url_part.starts_with("github.com/")
        || url_part.starts_with("gitlab.com/")
        || url_part.starts_with("bitbucket.org/")
    {
        format!("https://{url_part}")
    } else {
        url_part.to_string()
    };
    Some((repo_url, ref_name))
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

#[cfg(test)]
mod tests {
    use super::{parse_git_source, provider_label_matches};

    #[test]
    fn parse_git_source_strips_prefix_and_query() {
        assert_eq!(
            parse_git_source("git::https://example.com/org/mod.git?ref=main"),
            Some((
                "https://example.com/org/mod.git".into(),
                "main".into()
            ))
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
}
