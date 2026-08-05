use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
};
use pinner_node::NodeEcosystem;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/node-floating")
}

fn upgrade_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/node-upgrade")
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

#[test]
fn upgrade_prefers_resolve_map_over_native_lock() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_NODE_RESOLVE_MAP", "ms=2.1.3:2.1.4");
    }
    let eco = NodeEcosystem;
    let repo = upgrade_fixture();
    let stale_lock = [Pin {
        ecosystem: EcosystemKind::Node,
        name: "ms".into(),
        requested: "2.1.3".into(),
        pinned: "2.1.3".into(),
        path: PathBuf::from("package.json"),
        evidence: EvidenceKind::Lock,
        metadata: Default::default(),
    }];
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &stale_lock,
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Node,
        name: "ms".into(),
        requested: "2.1.3".into(),
        path: PathBuf::from("package.json"),
        is_floating: false,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    unsafe {
        std::env::remove_var("PINNER_NODE_RESOLVE_MAP");
    }
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].pinned, "2.1.4");
    assert_eq!(pins[0].metadata["previous"], "2.1.3");
    assert_eq!(pins[0].metadata["upgrade"], true);
    assert_eq!(pins[0].metadata["upgrade_channel"], "map");
    assert_ne!(pins[0].evidence, EvidenceKind::Lock);
    assert_ne!(pins[0].evidence, EvidenceKind::NativeLock);
}

#[test]
fn upgrade_omits_when_map_matches_previous() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_NODE_RESOLVE_MAP", "ms=2.1.3:2.1.3");
    }
    let eco = NodeEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Node,
        name: "ms".into(),
        requested: "2.1.3".into(),
        path: PathBuf::from("package.json"),
        is_floating: false,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    unsafe {
        std::env::remove_var("PINNER_NODE_RESOLVE_MAP");
    }
    assert!(
        pins.is_empty(),
        "unchanged upgrade must be omitted, got {pins:?}"
    );
}

#[test]
fn upgrade_offline_without_map_ignores_native_lock() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: clear map so resolve cannot succeed via seam.
    unsafe {
        std::env::remove_var("PINNER_NODE_RESOLVE_MAP");
    }
    let eco = NodeEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Node,
        name: "ms".into(),
        requested: "2.1.3".into(),
        path: PathBuf::from("package.json"),
        is_floating: false,
    };
    let err = eco.resolve(&[finding], &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("offline") || msg.contains("PINNER_NODE_RESOLVE_MAP"),
        "upgrade must not freeze on native lock; got {msg}"
    );
}
