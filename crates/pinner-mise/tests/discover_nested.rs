use std::path::PathBuf;

use pinner_ecosystem::Ecosystem;
use pinner_mise::MiseEcosystem;

#[test]
fn discovers_nested_mise_manifests() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mise-nested");
    let eco = MiseEcosystem::default();
    let manifests = eco.discover(&repo).unwrap();

    assert!(
        manifests.len() >= 2,
        "expected root and nested manifests, got: {manifests:?}"
    );

    let paths: Vec<_> = manifests.iter().map(|m| m.path.to_string_lossy()).collect();
    assert!(
        paths.iter().any(|p| p.ends_with("apps/web/.mise.toml")),
        "nested apps/web/.mise.toml not found in: {paths:?}"
    );
}
