use std::collections::BTreeSet;
use std::path::Path;

use pinner_ecosystem::{EcosystemError, EcosystemKind, Manifest};
use serde::Deserialize;
use walkdir::WalkDir;

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
        if is_chart_file(path) || is_gitops_manifest(path) {
            paths.insert(path.to_path_buf());
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| Manifest {
            ecosystem: EcosystemKind::Helm,
            path,
        })
        .collect())
}

fn is_chart_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("Chart.yaml") | Some("Chart.yml")
    )
}

fn is_yaml_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yaml") | Some("yml")
    )
}

/// Flux `HelmRelease` / Argo CD `Application` matched by document `kind`.
/// Unreadable or invalid YAML is skipped (not a hard discover error).
fn is_gitops_manifest(path: &Path) -> bool {
    if is_chart_file(path) || !is_yaml_file(path) {
        return false;
    }
    // values.yaml is not a chart/GitOps CRD surface for this ecosystem.
    if matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("values.yaml") | Some("values.yml")
    ) {
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
            && (kind == "HelmRelease" || kind == "Application")
        {
            return true;
        }
    }
    false
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == ".git" || s == "node_modules" || s == ".venv" || s == "vendor"
    })
}

#[cfg(test)]
mod tests {
    use super::{is_chart_file, should_skip};
    use std::path::Path;

    #[test]
    fn detects_chart_filenames() {
        assert!(is_chart_file(Path::new("Chart.yaml")));
        assert!(is_chart_file(Path::new("charts/app/Chart.yml")));
        assert!(!is_chart_file(Path::new("values.yaml")));
        assert!(!is_chart_file(Path::new("chart.yaml")));
    }

    #[test]
    fn skips_vcs_and_vendor_dirs() {
        assert!(should_skip(Path::new(".git")));
        assert!(should_skip(Path::new("repo/node_modules/pkg")));
        assert!(!should_skip(Path::new("charts/redis")));
    }
}
