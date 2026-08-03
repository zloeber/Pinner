use std::path::PathBuf;

use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_mise::MiseEcosystem;

#[test]
fn discovers_mise_toml_and_tool_versions() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mise-floating");
    let eco = MiseEcosystem::default();
    let manifests = eco.discover(&repo).unwrap();
    assert_eq!(manifests.len(), 2);
}

#[test]
fn extracts_latest_as_floating() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mise-floating");
    let eco = MiseEcosystem::default();
    let m = eco.discover(&repo).unwrap();
    let ctx = EcosystemCtx {
        lock_pins: &[],
        offline: false,
        pin_exact_ranges: true,
    };
    let findings: Vec<_> = m.iter().flat_map(|x| eco.extract(x, &ctx).unwrap()).collect();
    assert!(findings
        .iter()
        .any(|f| f.name == "node" && f.requested == "latest" && f.is_floating));
}
