use std::collections::{BTreeSet, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use pinner_ecosystem::{EcosystemError, EcosystemKind, Manifest};
use serde_yaml::Value;
use walkdir::WalkDir;

pub(crate) fn discover(repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
    let mut roots = BTreeSet::new();

    for entry in WalkDir::new(repo)
        .into_iter()
        .filter_entry(|e| !should_skip(e.path()))
    {
        let entry = entry
            .map_err(|e| EcosystemError::Io(std::io::Error::other(format!("walkdir: {e}"))))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if name == ".gitlab-ci.yml" {
            roots.insert(entry.path().to_path_buf());
        }
    }

    let mut paths = BTreeSet::new();
    let mut queue: VecDeque<PathBuf> = roots.into_iter().collect();
    let mut seen = HashSet::new();

    while let Some(path) = queue.pop_front() {
        if !seen.insert(path.clone()) {
            continue;
        }
        paths.insert(path.clone());
        for local in local_includes(&path)? {
            let candidate = resolve_local_include(repo, &path, &local);
            if candidate.is_file() {
                queue.push_back(candidate);
            }
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| Manifest {
            ecosystem: EcosystemKind::Gitlab,
            path,
        })
        .collect())
}

fn local_includes(path: &Path) -> Result<Vec<String>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let value: Value = serde_yaml::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    Ok(collect_local_includes(&value))
}

fn collect_local_includes(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let Some(include) = value.get("include") else {
        return out;
    };
    match include {
        Value::String(s) => {
            // Bare string include is treated as local path by GitLab when it looks like a path.
            if looks_like_local_path(s) {
                out.push(s.clone());
            }
        }
        Value::Sequence(items) => {
            for item in items {
                match item {
                    Value::String(s) if looks_like_local_path(s) => out.push(s.clone()),
                    Value::Mapping(map) => {
                        if let Some(local) = map
                            .get(Value::String("local".into()))
                            .and_then(|v| v.as_str())
                        {
                            out.push(local.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        Value::Mapping(map) => {
            if let Some(local) = map
                .get(Value::String("local".into()))
                .and_then(|v| v.as_str())
            {
                out.push(local.to_string());
            }
        }
        _ => {}
    }
    out
}

fn looks_like_local_path(s: &str) -> bool {
    let s = s.trim();
    if s.contains("://") {
        return false;
    }
    s.ends_with(".yml") || s.ends_with(".yaml") || s.starts_with('/') || s.starts_with("./")
}

fn resolve_local_include(repo: &Path, from: &Path, local: &str) -> PathBuf {
    let local = local.trim().trim_start_matches('/');
    // GitLab resolves local includes from the project root.
    let from_repo = repo.join(local);
    if from_repo.exists() {
        return from_repo;
    }
    // Fallback: relative to the including file's directory.
    from.parent()
        .map(|p| p.join(local))
        .unwrap_or_else(|| PathBuf::from(local))
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == ".git" || s == "node_modules" || s == ".venv" || s == "vendor"
    })
}

#[cfg(test)]
mod tests {
    use super::{collect_local_includes, looks_like_local_path, should_skip};
    use serde_yaml::Value;
    use std::path::Path;

    #[test]
    fn collects_local_include_entries() {
        let value: Value = serde_yaml::from_str(
            r#"
include:
  - local: templates/build.yml
  - project: group/ci
    ref: main
  - /other.yml
"#,
        )
        .unwrap();
        let locals = collect_local_includes(&value);
        assert!(locals.iter().any(|l| l == "templates/build.yml"));
        assert!(locals.iter().any(|l| l == "/other.yml"));
        assert!(!locals.iter().any(|l| l.contains("group/ci")));
    }

    #[test]
    fn local_path_heuristic() {
        assert!(looks_like_local_path("ci/foo.yml"));
        assert!(looks_like_local_path("/template.yaml"));
        assert!(!looks_like_local_path("https://example.com/x.yml"));
    }

    #[test]
    fn skips_vcs_and_vendor_dirs() {
        assert!(should_skip(Path::new(".git")));
        assert!(should_skip(Path::new("repo/node_modules/pkg")));
        assert!(!should_skip(Path::new(".gitlab-ci.yml")));
    }
}
