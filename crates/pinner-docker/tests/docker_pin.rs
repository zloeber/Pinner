use pinner_docker::DockerEcosystem;
use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use std::path::{Path, PathBuf};

#[test]
fn extracts_floating_from_and_compose_image() {
    // SAFETY: test-only env seam for deterministic resolve.
    unsafe {
        std::env::set_var(
            "PINNER_DOCKER_RESOLVE_MAP",
            "python:3.12=python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,alpine:latest=alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
    }
    let repo =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/docker-floating");
    let eco = DockerEcosystem;
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: false,
        pin_exact_ranges: true,
    };
    let manifests = eco.discover(&repo).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .collect();
    assert!(findings.iter().any(|f| f.requested.contains("python:3.12")));
    assert!(
        findings
            .iter()
            .any(|f| f.requested.contains("alpine:latest"))
    );
    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert!(pins.iter().all(|p| p.pinned.contains("@sha256:")));
}
