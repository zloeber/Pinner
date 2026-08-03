use std::path::{Path, PathBuf};

use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_terraform::TerraformEcosystem;

const GIT_SHA: &str = "11bd71901bbe5b1630ceea73d27597364c9af683";
const GIT_SOURCE: &str = "git::https://example.com/org/mod.git?ref=main";

#[test]
fn resolve_and_rewrite_via_env_map() {
    // SAFETY: test-only env seam for deterministic resolve.
    unsafe {
        std::env::set_var(
            "PINNER_TERRAFORM_RESOLVE_MAP",
            format!("~> 5.0=5.1.0,{GIT_SOURCE}={GIT_SHA}"),
        );
    }

    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/terraform-floating");
    let tmp = tempfile::tempdir().unwrap();
    for name in ["modules.tf", "providers.tf"] {
        std::fs::copy(fixture.join(name), tmp.path().join(name)).unwrap();
    }

    let eco = TerraformEcosystem;
    let ctx = EcosystemCtx {
        repo: tmp.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };

    let manifests = eco.discover(tmp.path()).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .filter(|f| f.is_floating)
        .collect();

    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert!(
        pins.iter()
            .any(|p| p.name == "vpc" && p.pinned == "5.1.0")
    );
    assert!(
        pins.iter()
            .any(|p| p.name == "git_mod" && p.pinned == GIT_SHA)
    );
    assert!(
        pins.iter()
            .any(|p| p.name == "hashicorp/aws" && p.pinned == "5.1.0")
    );

    let modules = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("modules.tf"))
        .expect("modules.tf");
    let module_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("modules.tf"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(modules, &module_pins)
        .unwrap()
        .expect("modules rewrite");
    assert!(
        rw.new_contents.contains("version = \"5.1.0\""),
        "expected exact vpc version, got:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains(&format!("ref={GIT_SHA}")),
        "expected git ref sha, got:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains("module \"local_mod\""),
        "local module must be preserved"
    );

    let providers = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("providers.tf"))
        .expect("providers.tf");
    let provider_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("providers.tf"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(providers, &provider_pins)
        .unwrap()
        .expect("providers rewrite");
    assert!(
        rw.new_contents.contains("version = \"5.1.0\""),
        "expected exact provider version, got:\n{}",
        rw.new_contents
    );
}
