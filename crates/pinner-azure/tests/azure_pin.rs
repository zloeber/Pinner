use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use pinner_azure::AzureEcosystem;
use pinner_ecosystem::{Ecosystem, EcosystemCtx};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/azure-floating")
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
fn extracts_floating_image_and_major_task() {
    let repo = fixture_dir();
    let eco = AzureEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert!(
        !manifests.is_empty(),
        "expected azure-pipelines.yml discover: {manifests:?}"
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
        floating
            .iter()
            .any(|f| f.name == "UseNode" && f.requested == "UseNode@1"),
        "floating major-only task missing: {findings:?}"
    );
    assert!(
        !findings.iter().any(|f| f.requested == "build" || f.name == "build"),
        "container alias must not be an image finding: {findings:?}"
    );
    assert!(
        !findings.iter().any(|f| f.requested.contains("ubuntu-latest")),
        "vmImage must not be extracted: {findings:?}"
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
            "PINNER_AZURE_RESOLVE_MAP",
            "UseNode@UseNode@1=UseNode@1.2.3",
        );
    }

    let fixture = fixture_dir();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::copy(
        fixture.join("azure-pipelines.yml"),
        tmp.path().join("azure-pipelines.yml"),
    )
    .unwrap();

    let eco = AzureEcosystem;
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
        pins.iter()
            .any(|p| p.name == "UseNode" && p.pinned == "UseNode@1.2.3"),
        "task pin missing: {pins:?}"
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
        rw.new_contents.contains("task: UseNode@1.2.3"),
        "task not rewritten:\n{}",
        rw.new_contents
    );
    assert!(
        !rw.new_contents.contains("UseNode@1\n") && !rw.new_contents.ends_with("UseNode@1"),
        "floating task remains:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains("container: build"),
        "container alias rewritten unexpectedly:\n{}",
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
        std::env::remove_var("PINNER_AZURE_RESOLVE_MAP");
    }
}

#[test]
fn offline_without_map_fails_closed() {
    let _guard = env_lock().lock().unwrap();
    unsafe {
        std::env::remove_var("PINNER_DOCKER_RESOLVE_MAP");
        std::env::remove_var("PINNER_AZURE_RESOLVE_MAP");
    }
    let repo = fixture_dir();
    let eco = AzureEcosystem;
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
            || msg.contains("PINNER_AZURE_RESOLVE_MAP"),
        "unexpected error: {msg}"
    );
}

#[test]
fn discovers_dot_azure_pipelines_dir() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".azure-pipelines")).unwrap();
    std::fs::write(
        tmp.path().join(".azure-pipelines/build.yml"),
        "steps:\n  - task: Npm@1\n",
    )
    .unwrap();

    let eco = AzureEcosystem;
    let manifests = eco.discover(tmp.path()).unwrap();
    let names: Vec<_> = manifests
        .iter()
        .map(|m| m.path.strip_prefix(tmp.path()).unwrap().to_path_buf())
        .collect();
    assert!(
        names
            .iter()
            .any(|p| p == Path::new(".azure-pipelines/build.yml")),
        "dot-dir pipeline not discovered: {names:?}"
    );

    let ctx = ctx(tmp.path());
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .collect();
    assert!(
        findings
            .iter()
            .any(|f| f.requested == "Npm@1" && f.is_floating),
        "nested task missing: {findings:?}"
    );
}

#[test]
fn digest_image_and_exact_task_not_floating() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("azure-pipelines.yml"),
        r#"
resources:
  containers:
    - container: build
      image: node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
steps:
  - task: UseNode@1.2.3
"#,
    )
    .unwrap();

    let eco = AzureEcosystem;
    let ctx = ctx(tmp.path());
    let manifests = eco.discover(tmp.path()).unwrap();
    assert_eq!(manifests.len(), 1);
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(findings.iter().all(|f| !f.is_floating), "{findings:?}");
}

#[test]
fn job_level_container_string_image_extracted() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("azure-pipelines.yml"),
        "jobs:\n- job: build\n  container: alpine:latest\n  steps:\n  - script: echo hi\n",
    )
    .unwrap();

    let eco = AzureEcosystem;
    let ctx = ctx(tmp.path());
    let manifests = eco.discover(tmp.path()).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f.requested == "alpine:latest" && f.is_floating),
        "job container image missing: {findings:?}"
    );
}
