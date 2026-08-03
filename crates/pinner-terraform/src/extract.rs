use std::path::Path;
use std::str::FromStr;

use hcl_edit::expr::{Expression, Object, ObjectKey};
use hcl_edit::structure::{Attribute, Block, Body};
use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest, repo_relative,
};

pub(crate) fn extract(
    manifest: &Manifest,
    ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(&manifest.path)?;
    let body = Body::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: manifest.path.clone(),
        message: e.to_string(),
    })?;

    let path = repo_relative(ctx.repo, &manifest.path);
    let mut findings = Vec::new();

    for block in body.get_blocks("module") {
        if let Some(finding) = extract_module(block, &path) {
            findings.push(finding);
        }
    }

    for tf in body.get_blocks("terraform") {
        for providers in tf.body.get_blocks("required_providers") {
            findings.extend(extract_required_providers(providers, &path));
        }
    }

    Ok(findings)
}

fn extract_module(block: &Block, path: &Path) -> Option<Finding> {
    let name = block.labels.first()?.as_str().to_string();
    let source = attr_str(&block.body, "source")?;
    if is_local_module_source(&source) {
        return None;
    }

    let version = attr_str(&block.body, "version");
    let (requested, floating) = module_requested_and_floating(&source, version.as_deref());

    Some(Finding {
        ecosystem: EcosystemKind::Terraform,
        name,
        requested,
        path: path.to_path_buf(),
        is_floating: floating,
    })
}

fn extract_required_providers(block: &Block, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for attr in block.body.attributes() {
        if let Some(finding) = extract_provider_attr(attr, path) {
            findings.push(finding);
        }
    }
    findings
}

fn extract_provider_attr(attr: &Attribute, path: &Path) -> Option<Finding> {
    let obj = attr.value.as_object()?;
    let source = object_str(obj, "source").unwrap_or_else(|| attr.key.as_str().to_string());
    let version = object_str(obj, "version").unwrap_or_default();
    let floating = !is_exact_version_constraint(&version);

    Some(Finding {
        ecosystem: EcosystemKind::Terraform,
        name: source,
        requested: version,
        path: path.to_path_buf(),
        is_floating: floating,
    })
}

fn module_requested_and_floating(source: &str, version: Option<&str>) -> (String, bool) {
    if is_git_or_http_source(source) {
        let floating = !git_ref_is_full_sha(source);
        return (source.to_string(), floating);
    }
    match version {
        None => (String::new(), true),
        Some(v) if v.trim().is_empty() || v.eq_ignore_ascii_case("latest") => (v.to_string(), true),
        Some(v) => (v.to_string(), !is_exact_version_constraint(v)),
    }
}

fn is_local_module_source(source: &str) -> bool {
    let source = source.trim();
    source.starts_with('.') || source.starts_with('/')
}

fn is_git_or_http_source(source: &str) -> bool {
    let s = source.trim();
    s.starts_with("git::")
        || s.starts_with("git@")
        || s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("github.com/")
        || s.starts_with("bitbucket.org/")
        || s.starts_with("gitlab.com/")
}

fn git_ref_is_full_sha(source: &str) -> bool {
    let Some(ref_) = extract_git_ref(source) else {
        return false;
    };
    is_full_git_sha(ref_)
}

fn extract_git_ref(source: &str) -> Option<&str> {
    let after_q = source.split_once('?')?.1;
    for part in after_q.split('&') {
        if let Some(value) = part.strip_prefix("ref=") {
            return Some(value);
        }
    }
    None
}

fn is_full_git_sha(ref_: &str) -> bool {
    let ref_ = ref_.trim();
    ref_.len() == 40 && ref_.chars().all(|c| c.is_ascii_hexdigit())
}

/// Exact version constraint: `"x.y.z"` or `= "x.y.z"` / `= x.y.z`.
fn is_exact_version_constraint(version: &str) -> bool {
    let version = version.trim();
    if version.is_empty() {
        return false;
    }
    let version = version.strip_prefix('=').map(str::trim).unwrap_or(version);
    is_exact_semver(version)
}

/// Exact semver: `MAJOR.MINOR.PATCH` with optional prerelease/build suffix.
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
    if !consume_dot(bytes, &mut i) {
        return false;
    }
    if !consume_digits(bytes, &mut i) {
        return false;
    }
    if !consume_dot(bytes, &mut i) {
        return false;
    }
    if !consume_digits(bytes, &mut i) {
        return false;
    }

    if i == bytes.len() {
        return true;
    }
    if bytes[i] == b'.' || bytes[i] == b'-' {
        i += 1;
        return i < bytes.len();
    }
    false
}

fn consume_digits(bytes: &[u8], i: &mut usize) -> bool {
    let start = *i;
    while *i < bytes.len() && bytes[*i].is_ascii_digit() {
        *i += 1;
    }
    *i > start
}

fn consume_dot(bytes: &[u8], i: &mut usize) -> bool {
    if *i < bytes.len() && bytes[*i] == b'.' {
        *i += 1;
        true
    } else {
        false
    }
}

fn attr_str(body: &Body, key: &str) -> Option<String> {
    body.get_attribute(key)
        .and_then(|attr| expr_str(&attr.value))
}

fn object_str(obj: &Object, key: &str) -> Option<String> {
    obj.iter()
        .find(|(k, _)| object_key_eq(k, key))
        .and_then(|(_, v)| expr_str(v.expr()))
}

fn object_key_eq(key: &ObjectKey, expected: &str) -> bool {
    match key {
        ObjectKey::Ident(ident) => ident.as_str() == expected,
        ObjectKey::Expression(Expression::String(s)) => s.as_str() == expected,
        _ => false,
    }
}

fn expr_str(expr: &Expression) -> Option<String> {
    expr.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{
        is_exact_version_constraint, is_full_git_sha, is_local_module_source,
        module_requested_and_floating,
    };

    #[test]
    fn local_sources() {
        assert!(is_local_module_source("./modules/local"));
        assert!(is_local_module_source("../shared"));
        assert!(is_local_module_source("/abs/path"));
        assert!(!is_local_module_source("terraform-aws-modules/vpc/aws"));
        assert!(!is_local_module_source(
            "git::https://example.com/org/mod.git?ref=main"
        ));
    }

    #[test]
    fn exact_constraints() {
        assert!(is_exact_version_constraint("5.0.0"));
        assert!(is_exact_version_constraint("= 5.0.0"));
        assert!(is_exact_version_constraint("=5.0.0"));
        assert!(!is_exact_version_constraint("~> 5.0"));
        assert!(!is_exact_version_constraint(">= 5.0.0"));
        assert!(!is_exact_version_constraint(""));
        assert!(!is_exact_version_constraint("latest"));
    }

    #[test]
    fn git_sha_detection() {
        assert!(is_full_git_sha("11bd71901bbe5b1630ceea73d27597364c9af683"));
        assert!(!is_full_git_sha("main"));
        assert!(!is_full_git_sha("11bd719"));
    }

    #[test]
    fn module_floating_signals() {
        let (req, floating) =
            module_requested_and_floating("terraform-aws-modules/vpc/aws", Some("~> 5.0"));
        assert_eq!(req, "~> 5.0");
        assert!(floating);

        let (req, floating) =
            module_requested_and_floating("terraform-aws-modules/vpc/aws", Some("5.0.0"));
        assert_eq!(req, "5.0.0");
        assert!(!floating);

        let source = "git::https://example.com/org/mod.git?ref=main";
        let (req, floating) = module_requested_and_floating(source, None);
        assert_eq!(req, source);
        assert!(floating);

        let pinned =
            "git::https://example.com/org/mod.git?ref=11bd71901bbe5b1630ceea73d27597364c9af683";
        let (req, floating) = module_requested_and_floating(pinned, None);
        assert_eq!(req, pinned);
        assert!(!floating);
    }
}
