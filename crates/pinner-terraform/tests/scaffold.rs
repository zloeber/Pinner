use pinner_ecosystem::{Ecosystem, EcosystemKind};
use pinner_terraform::TerraformEcosystem;
use std::path::Path;

#[test]
fn terraform_kind_and_empty_discover() {
    let eco = TerraformEcosystem;
    assert_eq!(eco.kind(), EcosystemKind::Terraform);
    let manifests = eco.discover(Path::new(".")).unwrap();
    assert!(
        manifests.is_empty()
            || manifests
                .iter()
                .all(|m| m.ecosystem == EcosystemKind::Terraform)
    );
}
