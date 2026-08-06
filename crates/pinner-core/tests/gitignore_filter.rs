use pinner_core::{Policy, RepoIgnore, RunOptions, audit};
use pinner_ecosystem::Ecosystem;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn nested_gitignore_skips_path() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join(".gitignore"), "skip-me/\n").unwrap();
    fs::write(dir.path().join("nested/.gitignore"), "secret.toml\n").unwrap();
    fs::create_dir_all(dir.path().join("skip-me")).unwrap();
    fs::write(
        dir.path().join("skip-me/Cargo.toml"),
        "[package]\nname=\"x\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("nested/secret.toml"), "x=1\n").unwrap();
    fs::write(dir.path().join("nested/keep.toml"), "x=1\n").unwrap();

    let gi = RepoIgnore::new(dir.path());
    assert!(gi.is_ignored(std::path::Path::new("skip-me/Cargo.toml")));
    assert!(gi.is_ignored(std::path::Path::new("nested/secret.toml")));
    assert!(!gi.is_ignored(std::path::Path::new("nested/keep.toml")));
}

#[test]
fn gitignore_negation_reincludes() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("pkg")).unwrap();
    fs::write(dir.path().join(".gitignore"), "pkg/*\n!pkg/keep.toml\n").unwrap();
    fs::write(dir.path().join("pkg/drop.toml"), "x=1\n").unwrap();
    fs::write(dir.path().join("pkg/keep.toml"), "x=1\n").unwrap();

    let gi = RepoIgnore::new(dir.path());
    assert!(gi.is_ignored(std::path::Path::new("pkg/drop.toml")));
    assert!(!gi.is_ignored(std::path::Path::new("pkg/keep.toml")));
}

#[test]
fn missing_gitignore_ignores_nothing() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
    let gi = RepoIgnore::new(dir.path());
    assert!(!gi.is_ignored(std::path::Path::new("Cargo.toml")));
}

/// Minimal stub: discovers a single planted mise-like toml under an ignored dir.
struct StubEco;

impl Ecosystem for StubEco {
    fn kind(&self) -> pinner_ecosystem::EcosystemKind {
        pinner_ecosystem::EcosystemKind::Mise
    }
    fn discover(
        &self,
        repo: &std::path::Path,
    ) -> Result<Vec<pinner_ecosystem::Manifest>, pinner_ecosystem::EcosystemError> {
        let ignored = repo.join("ignored/.mise.toml");
        let kept = repo.join(".mise.toml");
        Ok(vec![
            pinner_ecosystem::Manifest {
                ecosystem: self.kind(),
                path: ignored,
            },
            pinner_ecosystem::Manifest {
                ecosystem: self.kind(),
                path: kept,
            },
        ])
    }
    fn extract(
        &self,
        manifest: &pinner_ecosystem::Manifest,
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<pinner_ecosystem::Finding>, pinner_ecosystem::EcosystemError> {
        Ok(vec![pinner_ecosystem::Finding {
            ecosystem: self.kind(),
            name: "node".into(),
            requested: "latest".into(),
            path: manifest.path.clone(),
            is_floating: true,
        }])
    }
    fn resolve(
        &self,
        _findings: &[pinner_ecosystem::Finding],
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<pinner_ecosystem::Pin>, pinner_ecosystem::EcosystemError> {
        Ok(vec![])
    }
    fn rewrite(
        &self,
        _manifest: &pinner_ecosystem::Manifest,
        _pins: &[pinner_ecosystem::Pin],
    ) -> Result<Option<pinner_ecosystem::Rewrite>, pinner_ecosystem::EcosystemError> {
        Ok(None)
    }
}

#[test]
fn audit_skips_gitignored_manifests() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("ignored")).unwrap();
    fs::write(dir.path().join(".gitignore"), "ignored/\n").unwrap();
    fs::write(dir.path().join("ignored/.mise.toml"), "[tools]\nnode=\"latest\"\n").unwrap();
    fs::write(dir.path().join(".mise.toml"), "[tools]\nnode=\"latest\"\n").unwrap();

    let ecosystems: Vec<Arc<dyn Ecosystem>> = vec![Arc::new(StubEco)];
    let policy = Policy::default_policy();
    let opts = RunOptions {
        repo: dir.path().to_path_buf(),
        dry_run: true,
        offline: true,
        ecosystems_filter: None,
    };
    // After Task 3 signature includes progress: pass None.
    // For this task, if audit still has the old signature, call without progress.
    let report = audit(&ecosystems, &policy, &opts).unwrap();
    assert_eq!(report.findings.len(), 1);
    assert!(
        report.findings[0]
            .path
            .to_string_lossy()
            .contains(".mise.toml")
            && !report
                .findings[0]
                .path
                .to_string_lossy()
                .contains("ignored")
    );
}
