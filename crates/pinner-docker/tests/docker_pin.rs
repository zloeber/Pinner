use pinner_docker::DockerEcosystem;
use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn floating_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/docker-floating")
}

fn upgrade_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/docker-upgrade")
}

#[test]
fn extracts_floating_from_and_compose_image() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only env seam for deterministic resolve.
    unsafe {
        std::env::set_var(
            "PINNER_DOCKER_RESOLVE_MAP",
            "python:3.12=python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,alpine:latest=alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
    }
    let repo = floating_fixture();
    let eco = DockerEcosystem;
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: false,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
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
    let pins = eco.resolve(&findings, &ctx);
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
    }
    let pins = pins.unwrap();
    assert!(pins.iter().all(|p| p.pinned.contains("@sha256:")));
}

#[test]
fn upgrade_prefers_resolve_map_over_lock() {
    let _guard = env_lock().lock().unwrap();
    let new_digest =
        "python@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_DOCKER_RESOLVE_MAP",
            format!("python:3.12={new_digest}"),
        );
    }
    let eco = DockerEcosystem;
    let repo = upgrade_fixture();
    let stale_lock = [Pin {
        ecosystem: EcosystemKind::Docker,
        name: "python".into(),
        requested: "python:3.12".into(),
        pinned: "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .into(),
        path: PathBuf::from("Dockerfile"),
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
        ecosystem: EcosystemKind::Docker,
        name: "python".into(),
        requested: "python:3.12".into(),
        path: PathBuf::from("Dockerfile"),
        is_floating: true,
    };
    let pins = eco.resolve(&[finding], &ctx);
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
    }
    let pins = pins.unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].pinned, new_digest);
    assert_eq!(pins[0].metadata["previous"], "python:3.12");
    assert_eq!(pins[0].metadata["upgrade"], true);
    assert_eq!(pins[0].metadata["upgrade_channel"], "map");
    assert_ne!(pins[0].evidence, EvidenceKind::Lock);
}

#[test]
fn upgrade_omits_when_map_matches_previous() {
    let _guard = env_lock().lock().unwrap();
    let pinned =
        "python:3.12@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_DOCKER_RESOLVE_MAP",
            format!("{pinned}={pinned},python:3.12={pinned}"),
        );
    }
    let eco = DockerEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Docker,
        name: "python:3.12".into(),
        requested: pinned.into(),
        path: PathBuf::from("Dockerfile"),
        is_floating: false,
    };
    let pins = eco.resolve(&[finding], &ctx);
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
    }
    let pins = pins.unwrap();
    assert!(
        pins.is_empty(),
        "unchanged upgrade must be omitted, got {pins:?}"
    );
}

#[test]
fn upgrade_digest_only_without_tag_skips() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: clear map — skip must not depend on map for digest-only.
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
    }
    let eco = DockerEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Docker,
        name: "python".into(),
        requested: "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .into(),
        path: PathBuf::from("Dockerfile"),
        is_floating: false,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    assert!(
        pins.is_empty(),
        "digest-only without tag must skip, got {pins:?}"
    );
}

#[test]
fn upgrade_offline_without_map_ignores_lock() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: clear map so resolve cannot succeed via seam.
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
    }
    let eco = DockerEcosystem;
    let repo = upgrade_fixture();
    let stale_lock = [Pin {
        ecosystem: EcosystemKind::Docker,
        name: "python".into(),
        requested: "python:3.12".into(),
        pinned: "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .into(),
        path: PathBuf::from("Dockerfile"),
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
        ecosystem: EcosystemKind::Docker,
        name: "python".into(),
        requested: "python:3.12".into(),
        path: PathBuf::from("Dockerfile"),
        is_floating: true,
    };
    let err = eco.resolve(&[finding], &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("offline") || msg.contains("PINNER_DOCKER_RESOLVE_MAP"),
        "upgrade must not freeze on lock; got {msg}"
    );
}
