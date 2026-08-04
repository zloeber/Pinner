use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_gitlab::GitlabEcosystem;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gitlab-floating")
}

fn ctx(repo: &Path) -> EcosystemCtx<'_> {
    EcosystemCtx {
        repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    }
}

#[test]
fn extracts_floating_image_and_remote_include() {
    let repo = fixture_dir();
    let eco = GitlabEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert!(
        !manifests.is_empty(),
        "expected .gitlab-ci.yml discover: {manifests:?}"
    );

    let ctx = ctx(&repo);
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .collect();

    let floating: Vec<_> = findings.iter().filter(|f| f.is_floating).collect();
    assert!(
        floating
            .iter()
            .any(|f| f.requested == "node:latest" && f.name == "node"),
        "floating image missing: {findings:?}"
    );
    assert!(
        floating.iter().any(|f| {
            f.name == "group/ci-templates" && f.requested == "group/ci-templates@main"
        }),
        "floating remote include missing: {findings:?}"
    );
}

#[test]
fn resolve_and_rewrite_via_resolve_maps() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seams; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_DOCKER_RESOLVE_MAP",
            "node:latest=node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        std::env::set_var(
            "PINNER_GITLAB_RESOLVE_MAP",
            "group/ci-templates@group/ci-templates@main=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
    }

    let fixture = fixture_dir();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::copy(
        fixture.join(".gitlab-ci.yml"),
        tmp.path().join(".gitlab-ci.yml"),
    )
    .unwrap();

    let eco = GitlabEcosystem;
    let ctx = ctx(tmp.path());
    let manifests = eco.discover(tmp.path()).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .filter(|f| f.is_floating)
        .collect();

    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert!(
        pins.iter().any(|p| p.pinned.contains("@sha256:")),
        "image pin missing digest: {pins:?}"
    );
    assert!(
        pins.iter().any(|p| {
            p.name == "group/ci-templates" && p.pinned == "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }),
        "include pin missing: {pins:?}"
    );

    let manifest = &manifests[0];
    let rw = eco
        .rewrite(manifest, &pins)
        .unwrap()
        .expect("expected rewrite");
    assert!(
        rw.new_contents.contains(
            "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        "image not rewritten:\n{}",
        rw.new_contents
    );
    assert!(
        !rw.new_contents.contains("node:latest"),
        "floating image remains:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents
            .contains("ref: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        "include ref not rewritten:\n{}",
        rw.new_contents
    );
    assert!(
        !rw.new_contents.contains("ref: main"),
        "floating ref remains:\n{}",
        rw.new_contents
    );

    // Idempotent: after rewrite, no floating findings.
    std::fs::write(&manifest.path, &rw.new_contents).unwrap();
    let after = eco.extract(manifest, &ctx).unwrap();
    assert!(
        after.iter().all(|f| !f.is_floating),
        "expected no floating after rewrite: {after:?}"
    );

    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
        std::env::remove_var("PINNER_GITLAB_RESOLVE_MAP");
    }
}

#[test]
fn offline_without_map_fails_closed() {
    let _guard = env_lock().lock().unwrap();
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
        std::env::remove_var("PINNER_GITLAB_RESOLVE_MAP");
    }
    let repo = fixture_dir();
    let eco = GitlabEcosystem;
    let ctx = ctx(&repo);
    let manifests = eco.discover(&repo).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .filter(|f| f.is_floating)
        .collect();
    let err = eco.resolve(&findings, &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("offline")
            || msg.contains("PINNER_DOCKER_RESOLVE_MAP")
            || msg.contains("PINNER_GITLAB_RESOLVE_MAP"),
        "unexpected error: {msg}"
    );
}

#[test]
fn discovers_nested_local_includes() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("ci")).unwrap();
    std::fs::write(
        tmp.path().join(".gitlab-ci.yml"),
        r#"
include:
  - local: ci/nested.yml
"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("ci/nested.yml"), "image: alpine:latest\n").unwrap();

    let eco = GitlabEcosystem;
    let manifests = eco.discover(tmp.path()).unwrap();
    let names: Vec<_> = manifests
        .iter()
        .map(|m| m.path.strip_prefix(tmp.path()).unwrap().to_path_buf())
        .collect();
    assert!(
        names.iter().any(|p| p == Path::new(".gitlab-ci.yml")),
        "names={names:?}"
    );
    assert!(
        names.iter().any(|p| p == Path::new("ci/nested.yml")),
        "nested local include not discovered: {names:?}"
    );

    let ctx = ctx(tmp.path());
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .collect();
    assert!(
        findings.iter().any(|f| f.requested == "alpine:latest"),
        "nested image missing: {findings:?}"
    );
}

#[test]
fn digest_pinned_image_and_sha_include_not_floating() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join(".gitlab-ci.yml"),
        r#"
image: node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
include:
  - project: 'group/ci-templates'
    file: '/template.yml'
    ref: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
"#,
    )
    .unwrap();

    let eco = GitlabEcosystem;
    let ctx = ctx(tmp.path());
    let manifests = eco.discover(tmp.path()).unwrap();
    assert_eq!(manifests.len(), 1);
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|f| !f.is_floating), "{findings:?}");
}
