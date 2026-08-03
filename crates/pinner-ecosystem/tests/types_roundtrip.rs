// crates/pinner-ecosystem/tests/types_roundtrip.rs
use pinner_ecosystem::{EcosystemKind, EvidenceKind, Pin};
use std::path::PathBuf;

#[test]
fn pin_serializes_stable_field_names() {
    let pin = Pin {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "lts".into(),
        pinned: "22.11.0".into(),
        path: PathBuf::from(".mise.toml"),
        evidence: EvidenceKind::Lock,
        metadata: Default::default(),
    };
    let v = serde_json::to_value(&pin).unwrap();
    assert_eq!(v["ecosystem"], "mise");
    assert_eq!(v["evidence"], "lock");
    assert_eq!(v["pinned"], "22.11.0");
}

#[test]
fn ecosystem_kind_terraform_serializes_as_lowercase() {
    let v = serde_json::to_value(EcosystemKind::Terraform).unwrap();
    assert_eq!(v, "terraform");
}
