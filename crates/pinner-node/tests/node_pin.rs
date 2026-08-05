use pinner_ecosystem::{Ecosystem, EcosystemCtx, ResolveMode, EcosystemKind, EvidenceKind, Finding};
use pinner_node::NodeEcosystem;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/node-floating")
}

#[test]
fn extracts_latest_and_caret_as_floating() {
    let eco = NodeEcosystem;
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
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
        repo: Path::new("."),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
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
        repo: Path::new("."),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    let rw = eco.rewrite(&manifests[0], &pins).unwrap().unwrap();
    assert!(rw.new_contents.contains("\"ms\": \"2.1.3\""));
    assert!(!rw.new_contents.contains("latest"));
}

#[test]
fn resolves_from_pnpm_lock_when_package_lock_absent() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"ms":"latest"}}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\npackages:\n  ms@2.1.3:\n    resolution: {integrity: sha512-abc}\n",
    )
    .unwrap();
    let eco = NodeEcosystem;
    let ctx = EcosystemCtx {
        repo: dir.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Node,
        name: "ms".into(),
        requested: "latest".into(),
        path: PathBuf::from("package.json"),
        is_floating: true,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    assert_eq!(pins[0].pinned, "2.1.3");
    assert_eq!(pins[0].evidence, EvidenceKind::NativeLock);
}

#[test]
fn resolves_from_yarn_lock_when_package_lock_absent() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"ms":"latest"}}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("yarn.lock"),
        "# yarn lockfile v1\n\nms@^2.0.0, ms@latest:\n  version \"2.1.3\"\n",
    )
    .unwrap();
    let eco = NodeEcosystem;
    let ctx = EcosystemCtx {
        repo: dir.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Node,
        name: "ms".into(),
        requested: "latest".into(),
        path: PathBuf::from("package.json"),
        is_floating: true,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    assert_eq!(pins[0].pinned, "2.1.3");
    assert_eq!(pins[0].evidence, EvidenceKind::NativeLock);
}
