use std::collections::BTreeSet;
use std::path::Path;

use pinner_ecosystem::{EcosystemError, EcosystemKind, Manifest};
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
        if is_azure_pipeline_file(entry.path()) {
            paths.insert(entry.path().to_path_buf());
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| Manifest {
            ecosystem: EcosystemKind::Azure,
            path,
        })
        .collect())
}

fn is_azure_pipeline_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let is_yml = name.ends_with(".yml") || name.ends_with(".yaml");
    if !is_yml {
        return false;
    }
    if name.starts_with("azure-pipelines") {
        return true;
    }
    path.components().any(|c| c.as_os_str() == ".azure-pipelines")
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == ".git" || s == "node_modules" || s == ".venv" || s == "vendor"
    })
}

#[cfg(test)]
mod tests {
    use super::{is_azure_pipeline_file, should_skip};
    use std::path::Path;

    #[test]
    fn recognizes_pipeline_filenames() {
        assert!(is_azure_pipeline_file(Path::new("azure-pipelines.yml")));
        assert!(is_azure_pipeline_file(Path::new(
            "azure-pipelines-ci.yaml"
        )));
        assert!(is_azure_pipeline_file(Path::new(
            ".azure-pipelines/build.yml"
        )));
        assert!(!is_azure_pipeline_file(Path::new("ci.yml")));
        assert!(!is_azure_pipeline_file(Path::new("Dockerfile")));
    }

    #[test]
    fn skips_vcs_and_vendor_dirs() {
        assert!(should_skip(Path::new(".git")));
        assert!(should_skip(Path::new("repo/node_modules/pkg")));
        assert!(!should_skip(Path::new("azure-pipelines.yml")));
    }
}
