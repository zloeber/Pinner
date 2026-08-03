use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSetBuilder};
use pinner_ecosystem::{EcosystemError, EcosystemKind, Manifest};
use serde_json::Value;
use walkdir::WalkDir;

pub(crate) fn discover(repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
    let mut paths = BTreeSet::new();

    let root_pkg = repo.join("package.json");
    if root_pkg.is_file() {
        paths.insert(root_pkg.clone());
        for workspace_pkg in workspace_package_jsons(repo, &root_pkg)? {
            paths.insert(workspace_pkg);
        }
    }

    // Also pick up package.json files under the tree (skip node_modules).
    for entry in WalkDir::new(repo)
        .into_iter()
        .filter_entry(|e| !is_node_modules(e.path()))
    {
        let entry = entry
            .map_err(|e| EcosystemError::Io(std::io::Error::other(format!("walkdir: {e}"))))?;
        if entry.file_type().is_file() && entry.file_name() == "package.json" {
            paths.insert(entry.path().to_path_buf());
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| Manifest {
            ecosystem: EcosystemKind::Node,
            path,
        })
        .collect())
}

fn is_node_modules(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "node_modules")
}

/// Expand `workspaces` globs one level from the root package.json.
fn workspace_package_jsons(repo: &Path, root_pkg: &Path) -> Result<Vec<PathBuf>, EcosystemError> {
    let contents = std::fs::read_to_string(root_pkg)?;
    let value: Value = serde_json::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: root_pkg.to_path_buf(),
        message: e.to_string(),
    })?;

    let patterns = workspace_patterns(&value);
    if patterns.is_empty() {
        return Ok(Vec::new());
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in &patterns {
        // Match directory or package.json under the workspace glob.
        let dir_glob = if pattern.ends_with("/package.json") {
            pattern.clone()
        } else if pattern.ends_with('/') {
            format!("{pattern}package.json")
        } else {
            format!("{pattern}/package.json")
        };
        let glob = Glob::new(&dir_glob).map_err(|e| EcosystemError::Parse {
            path: root_pkg.to_path_buf(),
            message: format!("invalid workspace glob {pattern:?}: {e}"),
        })?;
        builder.add(glob);
    }
    let set = builder.build().map_err(|e| EcosystemError::Parse {
        path: root_pkg.to_path_buf(),
        message: format!("workspace globset: {e}"),
    })?;

    let mut found = Vec::new();
    for entry in WalkDir::new(repo)
        .max_depth(3)
        .into_iter()
        .filter_entry(|e| !is_node_modules(e.path()))
    {
        let entry = entry
            .map_err(|e| EcosystemError::Io(std::io::Error::other(format!("walkdir: {e}"))))?;
        if !entry.file_type().is_file() || entry.file_name() != "package.json" {
            continue;
        }
        let rel = match entry.path().strip_prefix(repo) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if set.is_match(&rel_str) {
            found.push(entry.path().to_path_buf());
        }
    }
    Ok(found)
}

fn workspace_patterns(value: &Value) -> Vec<String> {
    match value.get("workspaces") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        Some(Value::Object(obj)) => obj
            .get("packages")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}
