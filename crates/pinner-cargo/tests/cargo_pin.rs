use pinner_cargo::CargoEcosystem;
use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Manifest, ResolveMode,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cargo-floating")
}

#[test]
fn extracts_floating_cargo_deps() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cargo-floating");
    let eco = CargoEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert!(!manifests.is_empty());
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert!(findings.iter().any(|f| f.name == "serde" && f.is_floating));
    assert!(findings.iter().any(|f| f.name == "tokio" && f.is_floating));
}

#[test]
fn resolves_from_cargo_lock_and_rewrites_exact() {
    let eco = CargoEcosystem;
    let repo = fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let manifests = eco.discover(&repo).unwrap();
    let findings: Vec<_> = eco
        .extract(&manifests[0], &ctx)
        .unwrap()
        .into_iter()
        .filter(|f| f.is_floating)
        .collect();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert_eq!(
        pins.iter().find(|p| p.name == "serde").unwrap().pinned,
        "1.0.210"
    );
    assert_eq!(
        pins.iter().find(|p| p.name == "tokio").unwrap().pinned,
        "1.40.0"
    );
    assert!(pins.iter().all(|p| p.evidence == EvidenceKind::NativeLock));

    let dir = tempdir().unwrap();
    let cargo_toml = dir.path().join("Cargo.toml");
    fs::copy(repo.join("Cargo.toml"), &cargo_toml).unwrap();
    let manifest = Manifest {
        ecosystem: EcosystemKind::Cargo,
        path: cargo_toml.clone(),
    };
    let rw = eco.rewrite(&manifest, &pins).unwrap().unwrap();
    assert!(rw.new_contents.contains("serde = \"1.0.210\""));
    assert!(rw.new_contents.contains("version = \"1.40.0\""));
    assert!(!rw.new_contents.contains("serde = \"1\""));
    assert!(!rw.new_contents.contains("version = \"^1\""));
    fs::write(&cargo_toml, &rw.new_contents).unwrap();

    // Second pin is idempotent.
    let findings2: Vec<_> = eco
        .extract(&manifest, &ctx)
        .unwrap()
        .into_iter()
        .filter(|f| f.is_floating)
        .collect();
    assert!(findings2.is_empty());
    let rw2 = eco.rewrite(&manifest, &[]).unwrap();
    assert!(rw2.is_none());
}

#[test]
fn resolves_from_pinner_cargo_resolve_map() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1"
"#,
    )
    .unwrap();

    // SAFETY: test-only env seam; held behind env_lock.
    unsafe {
        std::env::set_var("PINNER_CARGO_RESOLVE_MAP", "serde=1:1.0.210");
    }
    let eco = CargoEcosystem;
    let ctx = EcosystemCtx {
        repo: dir.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let manifests = eco.discover(dir.path()).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    unsafe {
        std::env::remove_var("PINNER_CARGO_RESOLVE_MAP");
    }
    assert_eq!(pins[0].pinned, "1.0.210");
    assert_eq!(pins[0].evidence, EvidenceKind::Registry);
}

#[test]
fn ignores_parent_cargo_lock_outside_repo() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: clear map so a parallel env-map test cannot make resolve succeed.
    unsafe {
        std::env::remove_var("PINNER_CARGO_RESOLVE_MAP");
    }
    let outer = tempdir().unwrap();
    let repo = outer.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    fs::write(
        outer.path().join("Cargo.lock"),
        "version = 3\n\n[[package]]\nname = \"serde\"\nversion = \"9.9.9\"\n",
    )
    .unwrap();
    fs::write(
        repo.join("Cargo.toml"),
        r#"[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1"
"#,
    )
    .unwrap();

    let eco = CargoEcosystem;
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let manifests = eco.discover(&repo).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let err = eco.resolve(&findings, &ctx).unwrap_err();
    assert!(matches!(
        err,
        pinner_ecosystem::EcosystemError::Offline { .. }
            | pinner_ecosystem::EcosystemError::Resolve { .. }
    ));
}
