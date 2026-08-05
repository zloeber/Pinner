use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
};
use pinner_gitlab::GitlabEcosystem;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gitlab-floating")
}

fn upgrade_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gitlab-upgrade")
}

fn ctx(repo: &Path) -> EcosystemCtx<'_> {
    EcosystemCtx {
        repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
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
            p.name == "group/ci-templates"
                && p.pinned == "group/ci-templates@bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
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

#[test]
fn upgrade_prefers_resolve_map_over_lock() {
    let _guard = env_lock().lock().unwrap();
    let old_digest =
        "node:20@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let new_digest = "node@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let old_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let new_sha = "dddddddddddddddddddddddddddddddddddddddd";
    // SAFETY: test-only resolve seams; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_DOCKER_RESOLVE_MAP",
            format!("{old_digest}={new_digest},node:20={new_digest}"),
        );
        std::env::set_var(
            "PINNER_GITLAB_RESOLVE_MAP",
            format!("group/ci-templates@group/ci-templates@{old_sha}=group/ci-templates@{new_sha}"),
        );
    }

    let eco = GitlabEcosystem;
    let repo = upgrade_fixture();
    let stale_lock = [
        Pin {
            ecosystem: EcosystemKind::Gitlab,
            name: "node".into(),
            requested: old_digest.into(),
            pinned: old_digest.into(),
            path: PathBuf::from(".gitlab-ci.yml"),
            evidence: EvidenceKind::Lock,
            metadata: Default::default(),
        },
        Pin {
            ecosystem: EcosystemKind::Gitlab,
            name: "group/ci-templates".into(),
            requested: format!("group/ci-templates@{old_sha}"),
            pinned: format!("group/ci-templates@{old_sha}"),
            path: PathBuf::from(".gitlab-ci.yml"),
            evidence: EvidenceKind::Lock,
            metadata: Default::default(),
        },
    ];
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &stale_lock,
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let findings = [
        Finding {
            ecosystem: EcosystemKind::Gitlab,
            name: "node".into(),
            requested: old_digest.into(),
            path: PathBuf::from(".gitlab-ci.yml"),
            is_floating: false,
        },
        Finding {
            ecosystem: EcosystemKind::Gitlab,
            name: "group/ci-templates".into(),
            requested: format!("group/ci-templates@{old_sha}"),
            path: PathBuf::from(".gitlab-ci.yml"),
            is_floating: false,
        },
    ];
    let pins = eco.resolve(&findings, &ctx);
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
        std::env::remove_var("PINNER_GITLAB_RESOLVE_MAP");
    }
    let pins = pins.unwrap();
    assert_eq!(pins.len(), 2, "{pins:?}");
    let image = pins.iter().find(|p| p.name == "node").unwrap();
    assert_eq!(image.pinned, new_digest);
    assert_eq!(image.metadata["previous"], old_digest);
    assert_eq!(image.metadata["upgrade"], true);
    assert_eq!(image.metadata["upgrade_channel"], "map");
    assert_eq!(image.metadata["kind"], "image");
    assert_ne!(image.evidence, EvidenceKind::Lock);

    let include = pins
        .iter()
        .find(|p| p.name == "group/ci-templates")
        .unwrap();
    assert_eq!(include.pinned, format!("group/ci-templates@{new_sha}"));
    assert_eq!(
        include.metadata["previous"],
        format!("group/ci-templates@{old_sha}")
    );
    assert_eq!(include.metadata["kind"], "include");
    assert_ne!(include.evidence, EvidenceKind::Lock);
}

#[test]
fn upgrade_omits_when_map_matches_previous() {
    let _guard = env_lock().lock().unwrap();
    let digest = "node:20@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    // SAFETY: test-only resolve seams; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_DOCKER_RESOLVE_MAP",
            format!("{digest}={digest},node:20={digest}"),
        );
        std::env::set_var(
            "PINNER_GITLAB_RESOLVE_MAP",
            format!("group/ci-templates@group/ci-templates@{sha}=group/ci-templates@{sha}"),
        );
    }
    let eco = GitlabEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let findings = [
        Finding {
            ecosystem: EcosystemKind::Gitlab,
            name: "node".into(),
            requested: digest.into(),
            path: PathBuf::from(".gitlab-ci.yml"),
            is_floating: false,
        },
        Finding {
            ecosystem: EcosystemKind::Gitlab,
            name: "group/ci-templates".into(),
            requested: format!("group/ci-templates@{sha}"),
            path: PathBuf::from(".gitlab-ci.yml"),
            is_floating: false,
        },
    ];
    let pins = eco.resolve(&findings, &ctx);
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
        std::env::remove_var("PINNER_GITLAB_RESOLVE_MAP");
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
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
        std::env::remove_var("PINNER_GITLAB_RESOLVE_MAP");
    }
    let eco = GitlabEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Gitlab,
        name: "node".into(),
        requested: "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .into(),
        path: PathBuf::from(".gitlab-ci.yml"),
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
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
        std::env::remove_var("PINNER_GITLAB_RESOLVE_MAP");
    }
    let eco = GitlabEcosystem;
    let repo = upgrade_fixture();
    let old_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let stale_lock = [Pin {
        ecosystem: EcosystemKind::Gitlab,
        name: "group/ci-templates".into(),
        requested: format!("group/ci-templates@{old_sha}"),
        pinned: format!("group/ci-templates@{old_sha}"),
        path: PathBuf::from(".gitlab-ci.yml"),
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
        ecosystem: EcosystemKind::Gitlab,
        name: "group/ci-templates".into(),
        requested: format!("group/ci-templates@{old_sha}"),
        path: PathBuf::from(".gitlab-ci.yml"),
        is_floating: false,
    };
    let err = eco.resolve(&[finding], &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("offline") || msg.contains("PINNER_GITLAB_RESOLVE_MAP"),
        "upgrade must not freeze on lock; got {msg}"
    );
}
