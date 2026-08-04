use std::path::Path;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest, repo_relative,
};
use pinner_iac_common::image_name;
use serde_yaml::Value;

pub(crate) fn extract(
    manifest: &Manifest,
    ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let rel_path = repo_relative(ctx.repo, &manifest.path);
    let contents = std::fs::read_to_string(&manifest.path)?;
    let value: Value = serde_yaml::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: manifest.path.clone(),
        message: e.to_string(),
    })?;

    let mut findings = Vec::new();
    collect_images(&value, &rel_path, &mut findings);
    collect_tasks(&value, &rel_path, &mut findings);
    Ok(findings)
}

fn collect_images(value: &Value, path: &Path, out: &mut Vec<Finding>) {
    match value {
        Value::Mapping(map) => {
            for (k, v) in map {
                match k.as_str() {
                    Some("image") => push_image_finding(v, path, out),
                    Some("container") => push_container_finding(v, path, out),
                    _ => collect_images(v, path, out),
                }
            }
        }
        Value::Sequence(items) => {
            for item in items {
                collect_images(item, path, out);
            }
        }
        _ => {}
    }
}

fn push_container_finding(value: &Value, path: &Path, out: &mut Vec<Finding>) {
    match value {
        // Job-level `container: node:latest` — skip aliases like `container: build`.
        Value::String(s) if looks_like_image_ref(s) => push_image_ref(s, path, out),
        Value::String(_) => {}
        Value::Mapping(map) => {
            if let Some(image) = map
                .get(Value::String("image".into()))
                .and_then(|v| v.as_str())
            {
                push_image_ref(image, path, out);
            } else {
                // Nested keys under container mapping (e.g. endpoint, options).
                for (k, v) in map {
                    if k.as_str() == Some("image") {
                        push_image_finding(v, path, out);
                    } else {
                        collect_images(v, path, out);
                    }
                }
            }
        }
        other => collect_images(other, path, out),
    }
}

fn looks_like_image_ref(value: &str) -> bool {
    let value = value.trim();
    value.contains(':') || value.contains('/') || value.contains('@')
}

fn push_image_finding(value: &Value, path: &Path, out: &mut Vec<Finding>) {
    let Some(image) = value.as_str() else {
        return;
    };
    push_image_ref(image, path, out);
}

fn push_image_ref(image: &str, path: &Path, out: &mut Vec<Finding>) {
    let image = image.trim();
    if image.is_empty() {
        return;
    }
    out.push(Finding {
        ecosystem: EcosystemKind::Azure,
        name: image_name(image),
        requested: image.to_string(),
        path: path.to_path_buf(),
        is_floating: is_floating_image(image),
    });
}

fn collect_tasks(value: &Value, path: &Path, out: &mut Vec<Finding>) {
    match value {
        Value::Mapping(map) => {
            for (k, v) in map {
                if k.as_str() == Some("task") {
                    push_task_finding(v, path, out);
                } else {
                    collect_tasks(v, path, out);
                }
            }
        }
        Value::Sequence(items) => {
            for item in items {
                collect_tasks(item, path, out);
            }
        }
        _ => {}
    }
}

fn push_task_finding(value: &Value, path: &Path, out: &mut Vec<Finding>) {
    let Some(raw) = value.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    let Some((name, version)) = parse_task_ref(raw) else {
        return;
    };
    out.push(Finding {
        ecosystem: EcosystemKind::Azure,
        name: name.to_string(),
        requested: format!("{name}@{version}"),
        path: path.to_path_buf(),
        is_floating: is_floating_task_version(version),
    });
}

/// Parse `TaskName@1` / `TaskName@1.2.3`.
pub(crate) fn parse_task_ref(raw: &str) -> Option<(&str, &str)> {
    let raw = raw.trim();
    let (name, version) = raw.rsplit_once('@')?;
    let name = name.trim();
    let version = version.trim();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name, version))
}

/// Floating if missing `@sha256:` digest.
pub(crate) fn is_floating_image(image: &str) -> bool {
    let image = image.trim();
    if image.is_empty() {
        return false;
    }
    !image.contains("@sha256:")
}

/// Major-only (`1`) or incomplete (`1.2`) task versions are floating; exact `x.y.z` is pinned.
pub(crate) fn is_floating_task_version(version: &str) -> bool {
    let version = version.trim();
    if version.is_empty() {
        return false;
    }
    !is_exact_task_version(version)
}

pub(crate) fn is_exact_task_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 3 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Task findings use `Name@version` without digest.
pub(crate) fn is_task_finding(finding: &Finding) -> bool {
    finding.requested.starts_with(&format!("{}@", finding.name))
        && !finding.requested.contains("@sha256:")
        && parse_task_ref(&finding.requested).is_some()
}

#[cfg(test)]
mod tests {
    use super::{
        is_exact_task_version, is_floating_image, is_floating_task_version, is_task_finding,
        parse_task_ref,
    };
    use pinner_ecosystem::{EcosystemKind, Finding};
    use std::path::PathBuf;

    #[test]
    fn floating_image_detection() {
        assert!(is_floating_image("node:latest"));
        assert!(is_floating_image("alpine"));
        assert!(!is_floating_image(
            "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
    }

    #[test]
    fn floating_task_detection() {
        assert!(is_floating_task_version("1"));
        assert!(is_floating_task_version("1.2"));
        assert!(!is_floating_task_version("1.2.3"));
        assert!(is_exact_task_version("1.2.3"));
        assert!(!is_exact_task_version("1"));
    }

    #[test]
    fn parse_task_ref_splits_name_version() {
        assert_eq!(parse_task_ref("UseNode@1"), Some(("UseNode", "1")));
        assert_eq!(parse_task_ref("UseNode@1.2.3"), Some(("UseNode", "1.2.3")));
        assert_eq!(parse_task_ref("nope"), None);
    }

    #[test]
    fn task_finding_shape() {
        let task = Finding {
            ecosystem: EcosystemKind::Azure,
            name: "UseNode".into(),
            requested: "UseNode@1".into(),
            path: PathBuf::from("azure-pipelines.yml"),
            is_floating: true,
        };
        assert!(is_task_finding(&task));

        let image = Finding {
            ecosystem: EcosystemKind::Azure,
            name: "node".into(),
            requested: "node:latest".into(),
            path: PathBuf::from("azure-pipelines.yml"),
            is_floating: true,
        };
        assert!(!is_task_finding(&image));
    }
}
