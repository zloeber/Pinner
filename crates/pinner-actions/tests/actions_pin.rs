use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use pinner_actions::ActionsEcosystem;
use pinner_ecosystem::{Ecosystem, EcosystemCtx};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/actions-floating")
}

fn ctx<'a>(repo: &'a Path) -> EcosystemCtx<'a> {
    EcosystemCtx {
        repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    }
}

#[test]
fn extracts_container_service_and_reusable_workflow() {
    let repo = fixture_dir();
    let eco = ActionsEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert_eq!(manifests.len(), 1);

    let findings = eco.extract(&manifests[0], &ctx(&repo)).unwrap();
    let floating: Vec<_> = findings.iter().filter(|f| f.is_floating).collect();

    assert!(
        floating.iter().any(|f| {
            f.name == "container:build" && f.requested == "node:20" && f.is_floating
        }),
        "container finding missing: {findings:?}"
    );
    assert!(
        floating.iter().any(|f| {
            f.name == "service:build/redis" && f.requested == "redis:latest" && f.is_floating
        }),
        "service image finding missing: {findings:?}"
    );
    assert!(
        floating.iter().any(|f| {
            f.name == "org/repo/.github/workflows/reuse.yml"
                && f.requested == "org/repo/.github/workflows/reuse.yml@v1"
                && f.is_floating
        }),
        "reusable workflow uses finding missing: {findings:?}"
    );
    assert!(
        floating
            .iter()
            .any(|f| f.name == "actions/checkout" && f.requested == "actions/checkout@v4"),
        "checkout uses finding missing: {findings:?}"
    );
}

#[test]
fn pins_action_tag_to_sha_with_comment() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only env seam for deterministic resolve.
    unsafe {
        std::env::set_var(
            "PINNER_ACTIONS_RESOLVE_MAP",
            "actions/checkout@v4=11bd71901bbe5b1630ceea73d27597364c9af683",
        );
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
    }

    let repo = fixture_dir();
    let eco = ActionsEcosystem;
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: false,
        pin_exact_ranges: true,
    };
    let manifests = eco.discover(&repo).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let checkout: Vec<_> = findings
        .into_iter()
        .filter(|f| f.name == "actions/checkout")
        .collect();
    assert!(checkout[0].is_floating);
    let pins = eco.resolve(&checkout, &ctx).unwrap();
    let rw = eco.rewrite(&manifests[0], &pins).unwrap().unwrap();
    assert!(
        rw.new_contents
            .contains("actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683")
    );
    assert!(rw.new_contents.contains("# v4"));
}

#[test]
fn resolve_and_rewrite_images_and_reusable_workflow() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seams; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_DOCKER_RESOLVE_MAP",
            "node:20=node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,redis:latest=redis@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        std::env::set_var(
            "PINNER_ACTIONS_RESOLVE_MAP",
            "actions/checkout@v4=11bd71901bbe5b1630ceea73d27597364c9af683,org/repo/.github/workflows/reuse.yml@v1=cccccccccccccccccccccccccccccccccccccccc",
        );
    }

    let fixture = fixture_dir();
    let tmp = tempfile::tempdir().unwrap();
    let workflow_dir = tmp.path().join(".github/workflows");
    std::fs::create_dir_all(&workflow_dir).unwrap();
    std::fs::copy(
        fixture.join(".github/workflows/ci.yml"),
        workflow_dir.join("ci.yml"),
    )
    .unwrap();

    let eco = ActionsEcosystem;
    let ctx = ctx(tmp.path());
    let manifests = eco.discover(tmp.path()).unwrap();
    let findings: Vec<_> = eco
        .extract(&manifests[0], &ctx)
        .unwrap()
        .into_iter()
        .filter(|f| f.is_floating)
        .collect();

    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert!(
        pins.iter()
            .any(|p| p.name == "container:build" && p.pinned.contains("@sha256:")),
        "container pin missing: {pins:?}"
    );
    assert!(
        pins.iter()
            .any(|p| p.name == "service:build/redis" && p.pinned.contains("@sha256:")),
        "service pin missing: {pins:?}"
    );
    assert!(
        pins.iter().any(|p| {
            p.name == "org/repo/.github/workflows/reuse.yml"
                && p.pinned == "cccccccccccccccccccccccccccccccccccccccc"
        }),
        "reusable workflow pin missing: {pins:?}"
    );

    let rw = eco
        .rewrite(&manifests[0], &pins)
        .unwrap()
        .expect("expected rewrite");
    assert!(
        rw.new_contents.contains(
            "container: node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        "container not rewritten:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains(
            "image: redis@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ),
        "service image not rewritten:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains(
            "org/repo/.github/workflows/reuse.yml@cccccccccccccccccccccccccccccccccccccccc # v1"
        ),
        "reusable workflow not rewritten:\n{}",
        rw.new_contents
    );
    assert!(!rw.new_contents.contains("node:20"));
    assert!(!rw.new_contents.contains("redis:latest"));
    assert!(!rw.new_contents.contains("reuse.yml@v1\n") && !rw.new_contents.ends_with("reuse.yml@v1"));
}
