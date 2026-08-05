use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Finding, ResolveMode,
};
use pinner_python::PythonEcosystem;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python-floating")
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
