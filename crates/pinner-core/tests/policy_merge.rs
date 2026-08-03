use pinner_core::policy::Policy;
use pinner_ecosystem::EcosystemKind;
use std::fs;
use tempfile::tempdir;

#[test]
fn defaults_enable_all_ecosystems() {
    let p = Policy::default_policy();
    assert!(p.is_enabled(EcosystemKind::Mise));
    assert!(p.is_enabled(EcosystemKind::Actions));
}

#[test]
fn toml_can_disable_node() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pinner.toml");
    fs::write(&path, "[ecosystems]\nnode = false\n").unwrap();
    let p = Policy::load(Some(&path)).unwrap();
    assert!(!p.is_enabled(EcosystemKind::Node));
    assert!(p.is_enabled(EcosystemKind::Mise));
}

#[test]
fn ignore_globs_skip_node_modules() {
    let p = Policy::default_policy();
    assert!(p.is_ignored(std::path::Path::new("app/node_modules/pkg/package.json")));
}

#[test]
fn defaults_enable_terraform_but_not_helm_or_k8s() {
    let p = Policy::default_policy();
    assert!(p.is_enabled(EcosystemKind::Terraform));
    assert!(!p.is_enabled(EcosystemKind::Helm));
    assert!(!p.is_enabled(EcosystemKind::K8s));
}

#[test]
fn toml_can_enable_helm_and_k8s() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pinner.toml");
    fs::write(
        &path,
        "[ecosystems]\nhelm = true\nk8s = true\n",
    )
    .unwrap();
    let p = Policy::load(Some(&path)).unwrap();
    assert!(p.is_enabled(EcosystemKind::Helm));
    assert!(p.is_enabled(EcosystemKind::K8s));
    assert!(p.is_enabled(EcosystemKind::Terraform));
}
