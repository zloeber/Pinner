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
        let path = entry.path();
        if is_terraform_file(path) {
            paths.insert(path.to_path_buf());
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| Manifest {
            ecosystem: EcosystemKind::Terraform,
            path,
        })
        .collect())
}

fn is_terraform_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("tf") | Some("tofu")
    )
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == ".git" || s == ".terraform"
    })
}

#[cfg(test)]
mod tests {
    use super::{is_terraform_file, should_skip};
    use std::path::Path;

    #[test]
    fn detects_tf_and_tofu() {
        assert!(is_terraform_file(Path::new("main.tf")));
        assert!(is_terraform_file(Path::new("main.tofu")));
        assert!(!is_terraform_file(Path::new("main.tf.json")));
        assert!(!is_terraform_file(Path::new("README.md")));
    }

    #[test]
    fn skips_git_and_terraform_dirs() {
        assert!(should_skip(Path::new(".git")));
        assert!(should_skip(Path::new("repo/.terraform/modules")));
        assert!(!should_skip(Path::new("modules/vpc")));
    }
}
