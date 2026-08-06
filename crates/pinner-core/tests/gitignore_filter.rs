use pinner_core::RepoIgnore;
use std::fs;
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
