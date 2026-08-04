use std::path::Path;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest, repo_relative,
};
use pinner_iac_common::image_name;
use serde::Deserialize;
use serde_yaml::Value;

use crate::discover::is_values_file;

pub(crate) fn extract(
    manifest: &Manifest,
    ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let path = repo_relative(ctx.repo, &manifest.path);
    let contents = std::fs::read_to_string(&manifest.path)?;
    let file_name = manifest
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if matches!(file_name, "Chart.yaml" | "Chart.yml") {
        return extract_chart_yaml(&contents, &path, &manifest.path);
    }

    if is_values_file(&manifest.path) {
        return extract_values_images(&contents, &path, &manifest.path);
    }

    extract_gitops_docs(&contents, &path, &manifest.path)
}

fn extract_chart_yaml(
    contents: &str,
    rel_path: &Path,
    abs_path: &Path,
) -> Result<Vec<Finding>, EcosystemError> {
    let value: Value = serde_yaml::from_str(contents).map_err(|e| EcosystemError::Parse {
        path: abs_path.to_path_buf(),
        message: e.to_string(),
    })?;

    let Some(deps) = value.get("dependencies").and_then(|d| d.as_sequence()) else {
        return Ok(Vec::new());
    };

    let mut findings = Vec::new();
    for dep in deps {
        let Some(name) = dep.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        let version = dep.get("version").and_then(|v| v.as_str());
        let (requested, floating) = version_requested_and_floating(version);
        findings.push(Finding {
            ecosystem: EcosystemKind::Helm,
            name: name.to_string(),
            requested,
            path: rel_path.to_path_buf(),
            is_floating: floating,
        });
    }
    Ok(findings)
}

fn extract_gitops_docs(
    contents: &str,
    rel_path: &Path,
    abs_path: &Path,
) -> Result<Vec<Finding>, EcosystemError> {
    let mut findings = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(contents) {
        let value = Value::deserialize(doc).map_err(|e| EcosystemError::Parse {
            path: abs_path.to_path_buf(),
            message: e.to_string(),
        })?;
        if let Some(finding) = extract_gitops_doc(&value, rel_path) {
            findings.push(finding);
        }
    }
    Ok(findings)
}

/// Collect floating container image refs from Helm values YAML.
fn extract_values_images(
    contents: &str,
    rel_path: &Path,
    abs_path: &Path,
) -> Result<Vec<Finding>, EcosystemError> {
    let mut findings = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(contents) {
        let value = Value::deserialize(doc).map_err(|e| EcosystemError::Parse {
            path: abs_path.to_path_buf(),
            message: e.to_string(),
        })?;
        walk_values_for_images(&value, rel_path, &mut findings);
    }
    Ok(findings)
}

fn walk_values_for_images(value: &Value, rel_path: &Path, out: &mut Vec<Finding>) {
    match value {
        Value::Mapping(map) => {
            if let Some(image) = image_from_repo_tag_map(map) {
                push_image_finding(rel_path, &image, out);
            }
            for (key, child) in map {
                if let Some(key_str) = key.as_str()
                    && key_str.eq_ignore_ascii_case("image")
                    && let Some(s) = child.as_str()
                {
                    push_image_finding(rel_path, s, out);
                    continue;
                }
                walk_values_for_images(child, rel_path, out);
            }
        }
        Value::Sequence(seq) => {
            for child in seq {
                walk_values_for_images(child, rel_path, out);
            }
        }
        _ => {}
    }
}

fn image_from_repo_tag_map(map: &serde_yaml::Mapping) -> Option<String> {
    let repository = map
        .get(Value::String("repository".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let tag = map
        .get(Value::String("tag".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    let registry = map
        .get(Value::String("registry".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let repo = if let Some(registry) = registry {
        format!("{registry}/{repository}")
    } else {
        repository.to_string()
    };

    if let Some(digest) = map
        .get(Value::String("digest".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let digest = digest.strip_prefix('@').unwrap_or(digest);
        return Some(format!("{repo}@{digest}"));
    }

    if tag.is_empty() {
        Some(repo)
    } else {
        Some(format!("{repo}:{tag}"))
    }
}

fn push_image_finding(rel_path: &Path, image: &str, out: &mut Vec<Finding>) {
    let image = image.trim();
    if image.is_empty() || !looks_like_image_ref(image) {
        return;
    }
    out.push(Finding {
        ecosystem: EcosystemKind::Helm,
        name: image_name(image),
        requested: image.to_string(),
        path: rel_path.to_path_buf(),
        is_floating: is_floating_image(image),
    });
}

fn is_floating_image(image: &str) -> bool {
    let image = image.trim();
    !image.is_empty() && !image.contains("@sha256:")
}

fn looks_like_image_ref(image: &str) -> bool {
    let image = image.trim();
    if image.is_empty() || image.contains(' ') || image.contains('\n') {
        return false;
    }
    // Require a tag, digest, or registry/path shape so plain words are skipped.
    image.contains(':') || image.contains('@') || image.contains('/')
}

fn extract_gitops_doc(value: &Value, rel_path: &Path) -> Option<Finding> {
    let kind = value.get("kind")?.as_str()?;
    match kind {
        "HelmRelease" => extract_helm_release(value, rel_path),
        "Application" => extract_application(value, rel_path),
        _ => None,
    }
}

/// Flux HelmRelease: `spec.chart.spec.chart` + `spec.chart.spec.version`.
fn extract_helm_release(value: &Value, rel_path: &Path) -> Option<Finding> {
    let chart_spec = value.get("spec")?.get("chart")?.get("spec")?;
    let name = chart_spec.get("chart")?.as_str()?;
    let version = chart_spec.get("version").and_then(|v| v.as_str());
    let (requested, floating) = version_requested_and_floating(version);
    Some(Finding {
        ecosystem: EcosystemKind::Helm,
        name: name.to_string(),
        requested,
        path: rel_path.to_path_buf(),
        is_floating: floating,
    })
}

/// Argo CD Application: `spec.source.chart` + `spec.source.targetRevision`.
fn extract_application(value: &Value, rel_path: &Path) -> Option<Finding> {
    let source = value.get("spec")?.get("source")?;
    let name = source.get("chart")?.as_str()?;
    let version = source.get("targetRevision").and_then(|v| v.as_str());
    let (requested, floating) = version_requested_and_floating(version);
    Some(Finding {
        ecosystem: EcosystemKind::Helm,
        name: name.to_string(),
        requested,
        path: rel_path.to_path_buf(),
        is_floating: floating,
    })
}

fn version_requested_and_floating(version: Option<&str>) -> (String, bool) {
    match version {
        None => (String::new(), true),
        Some(v) => {
            let v = v.trim();
            if v.is_empty() || v == "*" || v.eq_ignore_ascii_case("latest") {
                (v.to_string(), true)
            } else {
                (v.to_string(), !is_exact_semver(v))
            }
        }
    }
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
    if bytes[i] == b'.' || bytes[i] == b'-' || bytes[i] == b'+' {
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

#[cfg(test)]
mod tests {
    use super::{is_exact_semver, version_requested_and_floating};

    #[test]
    fn floating_signals() {
        assert!(version_requested_and_floating(None).1);
        assert!(version_requested_and_floating(Some("*")).1);
        assert!(version_requested_and_floating(Some("latest")).1);
        assert!(version_requested_and_floating(Some("^1.0.0")).1);
        assert!(version_requested_and_floating(Some(">=6.0.0")).1);
        assert!(version_requested_and_floating(Some("~2.4.0")).1);
        assert!(!version_requested_and_floating(Some("1.14.0")).1);
    }

    #[test]
    fn exact_semver() {
        assert!(is_exact_semver("1.14.0"));
        assert!(is_exact_semver("1.14.0-rc.1"));
        assert!(!is_exact_semver("*"));
        assert!(!is_exact_semver("^1.0.0"));
        assert!(!is_exact_semver("1.x"));
    }
}
