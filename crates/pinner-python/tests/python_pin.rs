use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
};
use pinner_python::PythonEcosystem;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python-floating")
}

fn upgrade_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python-upgrade")
}

#[test]
fn extracts_unpinned_requirement() {
    let eco = PythonEcosystem;
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let manifests = eco.discover(&fixture()).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .collect();
    assert!(
        findings
            .iter()
            .any(|f| f.name == "requests" && f.is_floating)
    );
}

#[test]
fn resolves_from_uv_lock_offline() {
    let eco = PythonEcosystem;
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let manifests = eco.discover(&fixture()).unwrap();
    let req = manifests
        .iter()
        .find(|m| m.path.ends_with("requirements.txt"))
        .unwrap();
    let findings = eco.extract(req, &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert_eq!(pins[0].pinned, "2.32.3");
}

#[test]
fn resolves_from_poetry_lock_when_uv_lock_absent() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("requirements.txt"), "requests\n").unwrap();
    fs::write(
        dir.path().join("poetry.lock"),
        "[[package]]\nname = \"requests\"\nversion = \"2.32.3\"\n",
    )
    .unwrap();
    let eco = PythonEcosystem;
    let ctx = EcosystemCtx {
        repo: dir.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Python,
        name: "requests".into(),
        requested: "".into(),
        path: PathBuf::from("requirements.txt"),
        is_floating: true,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    assert_eq!(pins[0].pinned, "2.32.3");
    assert_eq!(pins[0].evidence, EvidenceKind::NativeLock);
}

#[test]
fn resolves_from_pdm_lock_when_uv_lock_absent() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("requirements.txt"), "httpx\n").unwrap();
    fs::write(
        dir.path().join("pdm.lock"),
        "[[package]]\nname = \"httpx\"\nversion = \"0.27.0\"\n",
    )
    .unwrap();
    let eco = PythonEcosystem;
    let ctx = EcosystemCtx {
        repo: dir.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Python,
        name: "httpx".into(),
        requested: "".into(),
        path: PathBuf::from("requirements.txt"),
        is_floating: true,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    assert_eq!(pins[0].pinned, "0.27.0");
    assert_eq!(pins[0].evidence, EvidenceKind::NativeLock);
}

#[test]
fn upgrade_prefers_resolve_map_over_native_lock() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_PYTHON_RESOLVE_MAP", "requests=2.32.3:2.33.0");
    }
    let eco = PythonEcosystem;
    let repo = upgrade_fixture();
    let stale_lock = [Pin {
        ecosystem: EcosystemKind::Python,
        name: "requests".into(),
        requested: "2.32.3".into(),
        pinned: "2.32.3".into(),
        path: PathBuf::from("requirements.txt"),
        evidence: EvidenceKind::Lock,
        metadata: Default::default(),
    }];
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &stale_lock,
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Python,
        name: "requests".into(),
        requested: "2.32.3".into(),
        path: PathBuf::from("requirements.txt"),
        is_floating: false,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    unsafe {
        std::env::remove_var("PINNER_PYTHON_RESOLVE_MAP");
    }
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].pinned, "2.33.0");
    assert_eq!(pins[0].metadata["previous"], "2.32.3");
    assert_eq!(pins[0].metadata["upgrade"], true);
    assert_eq!(pins[0].metadata["upgrade_channel"], "map");
    assert_ne!(pins[0].evidence, EvidenceKind::Lock);
    assert_ne!(pins[0].evidence, EvidenceKind::NativeLock);
}

#[test]
fn upgrade_omits_when_map_matches_previous() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_PYTHON_RESOLVE_MAP", "requests=2.32.3:2.32.3");
    }
    let eco = PythonEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Python,
        name: "requests".into(),
        requested: "2.32.3".into(),
        path: PathBuf::from("requirements.txt"),
        is_floating: false,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    unsafe {
        std::env::remove_var("PINNER_PYTHON_RESOLVE_MAP");
    }
    assert!(
        pins.is_empty(),
        "unchanged upgrade must be omitted, got {pins:?}"
    );
}

#[test]
fn upgrade_offline_without_map_ignores_native_lock() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: clear map so resolve cannot succeed via seam.
    unsafe {
        std::env::remove_var("PINNER_PYTHON_RESOLVE_MAP");
    }
    let eco = PythonEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Python,
        name: "requests".into(),
        requested: "2.32.3".into(),
        path: PathBuf::from("requirements.txt"),
        is_floating: false,
    };
    let err = eco.resolve(&[finding], &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("offline") || msg.contains("PINNER_PYTHON_RESOLVE_MAP"),
        "upgrade must not freeze on native lock; got {msg}"
    );
}
