use std::path::{Path, PathBuf};

use pinner_ecosystem::{
    repo_relative, EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest,
};
use pinner_iac_common::image_name;
use serde::Deserialize;
use serde_yaml::Value;

use crate::discover::is_target_kind;

/// Image extracted from a workload document, with Kubernetes `kind` for pin metadata.
#[derive(Debug, Clone)]
pub(crate) struct WorkloadImage {
    pub kind: String,
    pub image: String,
}

pub(crate) fn extract(
    manifest: &Manifest,
    ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let rel_path = repo_relative(ctx.repo, &manifest.path);
    let images = extract_workload_images(&manifest.path)?;
    Ok(images
        .into_iter()
        .map(|img| finding(&rel_path, &img.image))
        .collect())
}

/// Collect container/initContainer images from target workload kinds in a YAML file.
pub(crate) fn extract_workload_images(path: &Path) -> Result<Vec<WorkloadImage>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let mut images = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(&contents) {
        let value = Value::deserialize(doc).map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        images.extend(images_from_doc(&value));
    }
    Ok(images)
}

fn images_from_doc(value: &Value) -> Vec<WorkloadImage> {
    let Some(kind) = value.get("kind").and_then(|k| k.as_str()) else {
        return Vec::new();
    };
    if !is_target_kind(kind) {
        return Vec::new();
    }
    let Some(pod_spec) = pod_spec_for_kind(value, kind) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    push_container_images(pod_spec, kind, "initContainers", &mut out);
    push_container_images(pod_spec, kind, "containers", &mut out);
    out
}

fn pod_spec_for_kind<'a>(value: &'a Value, kind: &str) -> Option<&'a Value> {
    let spec = value.get("spec")?;
    if kind == "CronJob" {
        spec.get("jobTemplate")?
            .get("spec")?
            .get("template")?
            .get("spec")
    } else {
        // Deployment, StatefulSet, DaemonSet, Job
        spec.get("template")?.get("spec")
    }
}

fn push_container_images(
    pod_spec: &Value,
    kind: &str,
    field: &str,
    out: &mut Vec<WorkloadImage>,
) {
    let Some(containers) = pod_spec.get(field).and_then(|c| c.as_sequence()) else {
        return;
    };
    for container in containers {
        let Some(image) = container.get("image").and_then(|i| i.as_str()) else {
            continue;
        };
        let image = image.trim();
        if image.is_empty() {
            continue;
        }
        out.push(WorkloadImage {
            kind: kind.to_string(),
            image: image.to_string(),
        });
    }
}

fn finding(path: &Path, image: &str) -> Finding {
    Finding {
        ecosystem: EcosystemKind::K8s,
        name: image_name(image),
        requested: image.to_string(),
        path: path.to_path_buf(),
        is_floating: is_floating_image(image),
    }
}

/// Floating if missing `@sha256:` digest (includes `:latest`, untagged, and other tags).
pub(crate) fn is_floating_image(image: &str) -> bool {
    let image = image.trim();
    if image.is_empty() {
        return false;
    }
    !image.contains("@sha256:")
}

/// Tag portion of an image ref, or empty string when untagged / digest-only.
pub(crate) fn image_tag(image: &str) -> String {
    let image = image.trim();
    if image.contains('@') {
        return String::new();
    }
    let after_slash = image.rfind('/').map(|i| i + 1).unwrap_or(0);
    match image[after_slash..].rfind(':') {
        Some(i) => image[after_slash + i + 1..].to_string(),
        None => String::new(),
    }
}

/// Map `(repo-relative path, requested image) → kind` for pin metadata.
pub(crate) fn kind_lookup(
    repo: &Path,
    findings: &[Finding],
) -> Result<std::collections::HashMap<(PathBuf, String), String>, EcosystemError> {
    use std::collections::HashMap;
    let mut map = HashMap::new();
    let mut seen_paths = std::collections::HashSet::new();
    for finding in findings {
        if !seen_paths.insert(finding.path.clone()) {
            continue;
        }
        let abs = if finding.path.is_absolute() {
            finding.path.clone()
        } else {
            repo.join(&finding.path)
        };
        for img in extract_workload_images(&abs)? {
            map.entry((finding.path.clone(), img.image))
                .or_insert(img.kind);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::{image_tag, is_floating_image};

    #[test]
    fn floating_detection() {
        assert!(is_floating_image("nginx:latest"));
        assert!(is_floating_image("busybox:1.36"));
        assert!(is_floating_image("alpine"));
        assert!(!is_floating_image(
            "nginx@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
    }

    #[test]
    fn image_tag_extraction() {
        assert_eq!(image_tag("nginx:latest"), "latest");
        assert_eq!(image_tag("python:3.12"), "3.12");
        assert_eq!(image_tag("alpine"), "");
        assert_eq!(
            image_tag(
                "nginx@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            ""
        );
        assert_eq!(image_tag("ghcr.io/org/app:1.0"), "1.0");
    }
}
