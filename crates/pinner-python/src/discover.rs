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
        let entry = entry.map_err(|e| {
            EcosystemError::Io(std::io::Error::other(format!("walkdir: {e}")))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if name == "pyproject.toml" || is_requirements_file(&name) {
            paths.insert(entry.path().to_path_buf());
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| Manifest {
            ecosystem: EcosystemKind::Python,
            path,
        })
        .collect())
}

fn is_requirements_file(name: &str) -> bool {
    // requirements.txt, requirements-dev.txt, requirements_test.txt, etc.
    name == "requirements.txt"
        || (name.starts_with("requirements") && name.ends_with(".txt"))
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == ".venv"
            || s == "venv"
            || s == ".git"
            || s == "__pycache__"
            || s == ".tox"
            || s == "node_modules"
            || s == ".mypy_cache"
            || s == ".pytest_cache"
    })
}
