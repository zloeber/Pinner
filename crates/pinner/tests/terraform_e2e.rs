use assert_cmd::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

const RESOLVE_MAP: &str = "vpc@~> 5.0=5.1.0,hashicorp/aws@~> 5.0=5.100.0,git_mod@git::https://example.com/org/mod.git?ref=main=11bd71901bbe5b1630ceea73d27597364c9af683";

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/terraform-floating")
}

fn copy_fixture(dst: &Path) {
    let src = fixture_dir();
    for name in ["modules.tf", "providers.tf"] {
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
        std::env::set_var("PINNER_TERRAFORM_RESOLVE_MAP", RESOLVE_MAP);
    }

    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["pin", "--ecosystem", "terraform"])
        .assert()
        .success();

    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["check", "--ecosystem", "terraform"])
        .assert()
        .success();

    let lock = dir.path().join("pinner.lock.json");
    let modules = dir.path().join("modules.tf");
    let providers = dir.path().join("providers.tf");
    assert!(lock.is_file(), "pin must write pinner.lock.json");

    let lock_before = file_hash(&lock);
    let modules_before = file_hash(&modules);
    let providers_before = file_hash(&providers);

    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["pin", "--ecosystem", "terraform"])
        .assert()
        .success();

    assert_eq!(
        file_hash(&lock),
        lock_before,
        "second pin must not change lock"
    );
    assert_eq!(
        file_hash(&modules),
        modules_before,
        "second pin must not change modules.tf"
    );
    assert_eq!(
        file_hash(&providers),
        providers_before,
        "second pin must not change providers.tf"
    );

    let modules_body = fs::read_to_string(&modules).unwrap();
    assert!(
        modules_body.contains("version = \"5.1.0\""),
        "vpc module should be pinned"
    );
    assert!(
        modules_body.contains("ref=11bd71901bbe5b1630ceea73d27597364c9af683"),
        "git module ref should be pinned to sha"
    );
    assert!(
        modules_body.contains("module \"local_mod\""),
        "local module must be preserved"
    );

    let providers_body = fs::read_to_string(&providers).unwrap();
    assert!(
        providers_body.contains("version = \"5.100.0\""),
        "aws provider should be pinned"
    );

    unsafe {
        std::env::remove_var("PINNER_TERRAFORM_RESOLVE_MAP");
    }
}
