use std::path::Path;

use pinner_ecosystem::{Ecosystem, EcosystemCtx, ResolveMode};
use pinner_terraform::TerraformEcosystem;

#[test]
fn extracts_remote_modules_and_providers_skips_local() {
    let repo =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/terraform-floating");
    let eco = TerraformEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert!(manifests.len() >= 2);
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let mut findings = Vec::new();
    for m in &manifests {
        findings.extend(eco.extract(m, &ctx).unwrap());
    }
    assert!(
        findings
            .iter()
            .any(|f| f.name.contains("vpc") && f.is_floating)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.name.contains("git_mod") && f.is_floating)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.name == "hashicorp/aws" || f.name.contains("aws"))
    );
    assert!(!findings.iter().any(|f| f.name.contains("local_mod")));
}
