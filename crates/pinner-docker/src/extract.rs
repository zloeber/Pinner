use std::collections::HashMap;
use std::path::Path;

use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest};
use serde::Deserialize;

pub(crate) fn extract(
    manifest: &Manifest,
    _ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let file_name = manifest
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if is_dockerfile_name(file_name) {
        extract_dockerfile(&manifest.path)
    } else if is_compose_name(file_name) {
        extract_compose(&manifest.path)
    } else {
        Ok(Vec::new())
    }
}

fn is_dockerfile_name(name: &str) -> bool {
    name.starts_with("Dockerfile")
}

fn is_compose_name(name: &str) -> bool {
    matches!(
        name,
        "compose.yaml" | "compose.yml" | "docker-compose.yml" | "docker-compose.yaml"
    )
}

fn extract_dockerfile(path: &Path) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let mut findings = Vec::new();
    for line in contents.lines() {
        let Some(image) = parse_from_image(line) else {
            continue;
        };
        findings.push(finding(path, &image));
    }
    Ok(findings)
}

#[derive(Debug, Deserialize)]
struct ComposeFile {
    #[serde(default)]
    services: HashMap<String, ComposeService>,
}

#[derive(Debug, Deserialize)]
struct ComposeService {
    image: Option<String>,
}

fn extract_compose(path: &Path) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let compose: ComposeFile =
        serde_yaml::from_str(&contents).map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

    let mut findings = Vec::new();
    for (_name, service) in compose.services {
        let Some(image) = service.image else {
            continue;
        };
        let image = image.trim().to_string();
        if image.is_empty() {
            continue;
        }
        findings.push(finding(path, &image));
    }
    Ok(findings)
}

fn finding(path: &Path, image: &str) -> Finding {
    Finding {
        ecosystem: EcosystemKind::Docker,
        name: image_name(image),
        requested: image.to_string(),
        path: path.to_path_buf(),
        is_floating: is_floating_image(image),
    }
}

/// Parse `FROM [--flag=val ...] <image> [AS <stage>]` → image ref.
pub(crate) fn parse_from_image(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let mut parts = trimmed.split_whitespace();
    let from = parts.next()?;
    if !from.eq_ignore_ascii_case("FROM") {
        return None;
    }

    parts
        .find(|token| !token.starts_with("--") && !token.eq_ignore_ascii_case("AS"))
        .map(|token| token.to_string())
}

/// Floating if missing `@sha256:` digest (includes `:latest`, untagged, and other tags).
pub(crate) fn is_floating_image(image: &str) -> bool {
    let image = image.trim();
    if image.is_empty() {
        return false;
    }
    !image.contains("@sha256:")
}

/// Repository/name portion before tag or digest.
pub(crate) fn image_name(image: &str) -> String {
    let image = image.trim();
    if let Some((repo, _)) = image.split_once('@') {
        return repo.to_string();
    }
    // Tag separator is the last ':' that is not part of a registry port (host:port/repo).
    if let Some(idx) = find_tag_colon(image) {
        return image[..idx].to_string();
    }
    image.to_string()
}

fn find_tag_colon(image: &str) -> Option<usize> {
    // Prefer last ':' after the final '/'; if none, last ':' only when no '/'.
    let after_slash = image.rfind('/').map(|i| i + 1).unwrap_or(0);
    image[after_slash..].rfind(':').map(|i| after_slash + i)
}

#[cfg(test)]
mod tests {
    use super::{image_name, is_floating_image, parse_from_image};

    #[test]
    fn parses_from_with_stage_alias() {
        assert_eq!(
            parse_from_image("FROM python:3.12 AS build").as_deref(),
            Some("python:3.12")
        );
        assert_eq!(
            parse_from_image("FROM --platform=linux/amd64 alpine:latest AS runtime").as_deref(),
            Some("alpine:latest")
        );
    }

    #[test]
    fn floating_detection() {
        assert!(is_floating_image("python:3.12"));
        assert!(is_floating_image("alpine:latest"));
        assert!(is_floating_image("ubuntu"));
        assert!(!is_floating_image(
            "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
    }

    #[test]
    fn image_name_strips_tag_and_digest() {
        assert_eq!(image_name("python:3.12"), "python");
        assert_eq!(image_name("ghcr.io/org/app:1.0"), "ghcr.io/org/app");
        assert_eq!(
            image_name(
                "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            "python"
        );
    }
}
