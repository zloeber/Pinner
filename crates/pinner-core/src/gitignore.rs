use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// Cached gitignore matcher for a repository root (nested `.gitignore` supported).
pub struct RepoIgnore {
    root: PathBuf,
    matcher: Gitignore,
}

impl RepoIgnore {
    pub fn new(repo: &Path) -> Self {
        let root = repo.to_path_buf();
        let mut builder = GitignoreBuilder::new(&root);
        let _ = builder.add_line(None, ".git/");

        // Discover nested .gitignore files without descending into .git
        let walker = WalkBuilder::new(&root)
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .filter_entry(|e| e.file_name() != ".git")
            .build();
        for entry in walker.flatten() {
            if entry.file_type().is_some_and(|t| t.is_file()) && entry.file_name() == ".gitignore" {
                let _ = builder.add(entry.path());
            }
        }

        let matcher = builder.build().unwrap_or_else(|_| Gitignore::empty());
        Self { root, matcher }
    }

    /// Returns true if `path` (absolute or repo-relative) is ignored by gitignore rules.
    pub fn is_ignored(&self, path: &Path) -> bool {
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.root).unwrap_or(path).to_path_buf()
        } else {
            path.to_path_buf()
        };
        let normalized = PathBuf::from(relative.to_string_lossy().replace('\\', "/"));
        self.matcher
            .matched_path_or_any_parents(&normalized, false)
            .is_ignore()
    }
}
