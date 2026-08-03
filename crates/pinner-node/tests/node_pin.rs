use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_node::NodeEcosystem;
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/node-floating")
}

#[test]
fn extracts_latest_and_caret_as_floating() {
    let eco = NodeEcosystem;
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
    assert!(findings.iter().any(|f| f.name == "ms" && f.is_floating));
    assert!(
        findings
            .iter()
            .any(|f| f.name == "left-pad" && f.requested.starts_with('^'))
    );
}

#[test]
fn resolves_from_package_lock_when_offline() {
    let eco = NodeEcosystem;
    let ctx = EcosystemCtx {
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };
    let manifests = eco.discover(&fixture()).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert_eq!(
        pins.iter().find(|p| p.name == "ms").unwrap().pinned,
        "2.1.3"
    );
}

#[test]
fn rewrite_sets_exact_versions() {
    let eco = NodeEcosystem;
    let manifests = eco.discover(&fixture()).unwrap();
    let ctx = EcosystemCtx {
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    let rw = eco.rewrite(&manifests[0], &pins).unwrap().unwrap();
    assert!(rw.new_contents.contains("\"ms\": \"2.1.3\""));
    assert!(!rw.new_contents.contains("latest"));
}
