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
    collect_remote_includes(&value, &rel_path, &mut findings);
    Ok(findings)
}

fn collect_images(value: &Value, path: &Path, out: &mut Vec<Finding>) {
    match value {
        Value::Mapping(map) => {
            for (k, v) in map {
                if k.as_str() == Some("image") {
                    push_image_finding(v, path, out);
                } else {
                    collect_images(v, path, out);
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

fn push_image_finding(value: &Value, path: &Path, out: &mut Vec<Finding>) {
    let image = match value {
        Value::String(s) => Some(s.as_str()),
        Value::Mapping(map) => map
            .get(Value::String("name".into()))
            .and_then(|v| v.as_str()),
        _ => None,
    };
    let Some(image) = image.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    out.push(Finding {
        ecosystem: EcosystemKind::Gitlab,
        name: image_name(image),
        requested: image.to_string(),
        path: path.to_path_buf(),
        is_floating: is_floating_image(image),
    });
}

fn collect_remote_includes(value: &Value, path: &Path, out: &mut Vec<Finding>) {
    let Some(include) = value.get("include") else {
        return;
    };
    match include {
        Value::Sequence(items) => {
            for item in items {
                push_remote_include(item, path, out);
            }
        }
        Value::Mapping(_) => push_remote_include(include, path, out),
        _ => {}
    }
}

fn push_remote_include(value: &Value, path: &Path, out: &mut Vec<Finding>) {
    let Some(map) = value.as_mapping() else {
        return;
    };
    let Some(project) = map
        .get(Value::String("project".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let ref_ = map
        .get(Value::String("ref".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("main");
    if ref_.is_empty() {
        return;
    }
    out.push(Finding {
        ecosystem: EcosystemKind::Gitlab,
        name: project.to_string(),
        requested: format!("{project}@{ref_}"),
        path: path.to_path_buf(),
        is_floating: is_floating_ref(ref_),
    });
}

/// Floating if missing `@sha256:` digest.
pub(crate) fn is_floating_image(image: &str) -> bool {
    let image = image.trim();
    if image.is_empty() {
        return false;
    }
    !image.contains("@sha256:")
}

/// Non-full-hex SHA (not 40 or 64 hex) is floating.
pub(crate) fn is_floating_ref(ref_: &str) -> bool {
    !is_full_git_sha(ref_)
}

pub(crate) fn is_full_git_sha(ref_: &str) -> bool {
    let ref_ = ref_.trim();
    let len = ref_.len();
    (len == 40 || len == 64) && ref_.chars().all(|c| c.is_ascii_hexdigit())
}

/// Include findings use `project@ref` requested form (and are not digest image refs).
pub(crate) fn is_include_finding(finding: &Finding) -> bool {
    finding.requested.starts_with(&format!("{}@", finding.name))
        && !finding.requested.contains("@sha256:")
}

pub(crate) fn include_ref(requested: &str) -> Option<&str> {
    requested.rsplit_once('@').map(|(_, r)| r)
}

#[cfg(test)]
mod tests {
    use super::{is_floating_image, is_floating_ref, is_full_git_sha, is_include_finding};
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
    fn floating_ref_detection() {
        assert!(is_floating_ref("main"));
        assert!(is_floating_ref("v1.2.3"));
        assert!(!is_floating_ref(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        assert!(is_full_git_sha(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
    }

    #[test]
    fn include_finding_shape() {
        let include = Finding {
            ecosystem: EcosystemKind::Gitlab,
            name: "group/ci-templates".into(),
            requested: "group/ci-templates@main".into(),
            path: PathBuf::from(".gitlab-ci.yml"),
            is_floating: true,
        };
        assert!(is_include_finding(&include));

        let image = Finding {
            ecosystem: EcosystemKind::Gitlab,
            name: "node".into(),
            requested: "node:latest".into(),
            path: PathBuf::from(".gitlab-ci.yml"),
            is_floating: true,
        };
        assert!(!is_include_finding(&image));

        let digest = Finding {
            ecosystem: EcosystemKind::Gitlab,
            name: "node".into(),
            requested:
                "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .into(),
            path: PathBuf::from(".gitlab-ci.yml"),
            is_floating: false,
        };
        assert!(!is_include_finding(&digest));
    }
}
