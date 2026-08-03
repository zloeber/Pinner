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
        if is_workflow(path) || is_action_yml(path) {
            paths.insert(path.to_path_buf());
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| Manifest {
            ecosystem: EcosystemKind::Actions,
            path,
        })
        .collect())
}

/// `.github/workflows/*.{yml,yaml}`
fn is_workflow(path: &Path) -> bool {
    let mut comps = path.components().map(|c| c.as_os_str());
    let mut saw_github = false;
    for c in comps.by_ref() {
        if c == ".github" {
            saw_github = true;
            break;
        }
    }
    if !saw_github {
        return false;
    }
    match comps.next() {
        Some(c) if c == "workflows" => {}
        _ => return false,
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.ends_with(".yml") || name.ends_with(".yaml")
}

/// `**/action.yml` (and `action.yaml`)
fn is_action_yml(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("action.yml") | Some("action.yaml")
    )
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == ".git" || s == "node_modules" || s == ".venv" || s == "vendor"
    })
}

#[cfg(test)]
mod tests {
    use super::{is_action_yml, is_workflow};
    use std::path::Path;

    #[test]
    fn detects_workflow_and_action_yml() {
        assert!(is_workflow(Path::new(".github/workflows/ci.yml")));
        assert!(is_workflow(Path::new(
            "repo/.github/workflows/release.yaml"
        )));
        assert!(!is_workflow(Path::new(".github/dependabot.yml")));
        assert!(is_action_yml(Path::new("actions/setup/action.yml")));
        assert!(is_action_yml(Path::new("action.yaml")));
        assert!(!is_action_yml(Path::new("action.toml")));
    }
}
