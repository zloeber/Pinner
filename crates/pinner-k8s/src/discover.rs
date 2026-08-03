use std::collections::BTreeSet;
use std::path::Path;

use pinner_ecosystem::{EcosystemError, EcosystemKind, Manifest};
use serde::Deserialize;
use walkdir::WalkDir;

const TARGET_KINDS: &[&str] = &[
    "Deployment",
    "StatefulSet",
    "DaemonSet",
    "Job",
    "CronJob",
];

pub(crate) fn discover(repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
    let mut paths = BTreeSet::new();

    for entry in WalkDir::new(repo)
        .into_iter()
        .filter_entry(|e| !should_skip(e.path()))
    {
        let entry = entry
            .map_err(|e| EcosystemError::Io(std::io::Error::other(format!("walkdir: {e}"))))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if is_workload_manifest(path) {
            paths.insert(path.to_path_buf());
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| Manifest {
            ecosystem: EcosystemKind::K8s,
            path,
        })
        .collect())
}

fn is_yaml_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yaml") | Some("yml")
    )
}

/// YAML/YML with at least one core workload `kind`. Invalid YAML is skipped.
fn is_workload_manifest(path: &Path) -> bool {
    if !is_yaml_file(path) {
        return false;
    }
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    for doc in serde_yaml::Deserializer::from_str(&contents) {
        let Ok(value) = serde_yaml::Value::deserialize(doc) else {
            continue;
        };
        if let Some(kind) = value.get("kind").and_then(|k| k.as_str())
            && is_target_kind(kind)
        {
            return true;
        }
    }
    false
}

pub(crate) fn is_target_kind(kind: &str) -> bool {
    TARGET_KINDS.contains(&kind)
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == ".git" || s == "node_modules" || s == ".venv" || s == "vendor"
    })
}

#[cfg(test)]
mod tests {
    use super::{is_target_kind, should_skip};
    use std::path::Path;

    #[test]
    fn target_kinds_only() {
        assert!(is_target_kind("Deployment"));
        assert!(is_target_kind("CronJob"));
        assert!(!is_target_kind("ConfigMap"));
        assert!(!is_target_kind("HelmRelease"));
    }

    #[test]
    fn skips_vcs_and_vendor_dirs() {
        assert!(should_skip(Path::new(".git")));
        assert!(should_skip(Path::new("repo/node_modules/pkg")));
        assert!(!should_skip(Path::new("manifests/deploy.yaml")));
    }
}
