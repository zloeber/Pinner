use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{EcosystemError, EcosystemKind, Manifest};
use walkdir::WalkDir;

pub(crate) fn discover(repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
    let mut paths = BTreeSet::new();

    let go_work = repo.join("go.work");
    if go_work.is_file() {
        for module_dir in go_work_module_dirs(repo, &go_work)? {
            let go_mod = module_dir.join("go.mod");
            if go_mod.is_file() {
                paths.insert(go_mod);
            }
        }
    }

    for entry in WalkDir::new(repo)
        .into_iter()
        .filter_entry(|e| !should_skip(e.path()))
    {
        let entry = entry
            .map_err(|e| EcosystemError::Io(std::io::Error::other(format!("walkdir: {e}"))))?;
        if entry.file_type().is_file() && entry.file_name() == "go.mod" {
            paths.insert(entry.path().to_path_buf());
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| Manifest {
            ecosystem: EcosystemKind::Go,
            path,
        })
        .collect())
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == ".git" || s == "vendor" || s == "node_modules"
    })
}

/// Parse `use` directives from `go.work` into absolute module directories.
fn go_work_module_dirs(repo: &Path, go_work: &Path) -> Result<Vec<PathBuf>, EcosystemError> {
    let contents = std::fs::read_to_string(go_work)?;
    let mut dirs = Vec::new();
    let mut in_use_block = false;

    for raw in contents.lines() {
        let line = strip_go_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("use") {
            let rest = rest.trim();
            if rest == "(" {
                in_use_block = true;
                continue;
            }
            if rest == "()" {
                continue;
            }
            if !rest.is_empty() {
                dirs.push(resolve_use_path(repo, rest));
            }
            continue;
        }

        if in_use_block {
            if line == ")" {
                in_use_block = false;
                continue;
            }
            dirs.push(resolve_use_path(repo, line));
        }
    }

    Ok(dirs)
}

fn resolve_use_path(repo: &Path, entry: &str) -> PathBuf {
    let entry = entry.trim().trim_matches('"').trim_matches('\'');
    let path = Path::new(entry);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}

fn strip_go_comment(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::{go_work_module_dirs, resolve_use_path};
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    #[test]
    fn parses_go_work_use_block() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("go.work");
        fs::write(&path, "go 1.22\n\nuse (\n\t./alpha\n\t./beta // note\n)\n").unwrap();
        let dirs = go_work_module_dirs(dir.path(), &path).unwrap();
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0], dir.path().join("alpha"));
        assert_eq!(dirs[1], dir.path().join("beta"));
    }

    #[test]
    fn resolves_relative_use_path() {
        let repo = Path::new("/tmp/repo");
        assert_eq!(
            resolve_use_path(repo, "./mod"),
            PathBuf::from("/tmp/repo/mod")
        );
    }
}
