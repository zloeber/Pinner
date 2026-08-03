use assert_cmd::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mise-floating")
}

fn copy_fixture(dst: &Path) {
    let src = fixture_dir();
    for name in [".mise.toml", ".tool-versions"] {
        fs::copy(src.join(name), dst.join(name)).unwrap_or_else(|e| {
            panic!("copy {} from {}: {e}", name, src.display());
        });
    }
}

fn file_hash(path: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[test]
fn pin_then_check_is_clean_and_idempotent() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    copy_fixture(dir.path());

    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_MISE_RESOLVE_MAP",
            "node=22.11.0,python=3.12.7,ruby=3.3.5",
        );
    }

    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["pin"])
        .assert()
        .success();

    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["check"])
        .assert()
        .success();

    let lock = dir.path().join("pinner.lock.json");
    let mise_toml = dir.path().join(".mise.toml");
    let tool_versions = dir.path().join(".tool-versions");
    assert!(lock.is_file(), "pin must write pinner.lock.json");

    let lock_before = file_hash(&lock);
    let toml_before = file_hash(&mise_toml);
    let tools_before = file_hash(&tool_versions);

    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["pin"])
        .assert()
        .success();

    assert_eq!(
        file_hash(&lock),
        lock_before,
        "second pin must not change lock"
    );
    assert_eq!(
        file_hash(&mise_toml),
        toml_before,
        "second pin must not change .mise.toml"
    );
    assert_eq!(
        file_hash(&tool_versions),
        tools_before,
        "second pin must not change .tool-versions"
    );

    let mise_body = fs::read_to_string(&mise_toml).unwrap();
    assert!(mise_body.contains("22.11.0"), "node should be pinned");
    assert!(mise_body.contains("3.12.7"), "python should be pinned");
    let tools_body = fs::read_to_string(&tool_versions).unwrap();
    assert!(tools_body.contains("3.3.5"), "ruby should be pinned");

    unsafe {
        std::env::remove_var("PINNER_MISE_RESOLVE_MAP");
    }
}
