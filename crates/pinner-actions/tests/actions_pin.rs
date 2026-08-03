use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_actions::ActionsEcosystem;
use std::path::PathBuf;

#[test]
fn pins_action_tag_to_sha_with_comment() {
    // SAFETY: test-only env seam for deterministic resolve.
    unsafe {
        std::env::set_var(
            "PINNER_ACTIONS_RESOLVE_MAP",
            "actions/checkout@v4=11bd71901bbe5b1630ceea73d27597364c9af683",
        );
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/actions-floating");
    let eco = ActionsEcosystem;
    let ctx = EcosystemCtx { lock_pins: &[], offline: false, pin_exact_ranges: true };
    let manifests = eco.discover(&repo).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert!(findings[0].is_floating);
    let pins = eco.resolve(&findings, &ctx).unwrap();
    let rw = eco.rewrite(&manifests[0], &pins).unwrap().unwrap();
    assert!(rw.new_contents.contains("actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683"));
    assert!(rw.new_contents.contains("# v4"));
}
