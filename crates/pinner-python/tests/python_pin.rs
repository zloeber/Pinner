use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_python::PythonEcosystem;
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python-floating")
}

#[test]
fn extracts_unpinned_requirement() {
    let eco = PythonEcosystem;
    let ctx = EcosystemCtx {
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };
    let manifests = eco.discover(&fixture()).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .collect();
    assert!(findings.iter().any(|f| f.name == "requests" && f.is_floating));
}

#[test]
fn resolves_from_uv_lock_offline() {
    let eco = PythonEcosystem;
    let ctx = EcosystemCtx {
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
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
