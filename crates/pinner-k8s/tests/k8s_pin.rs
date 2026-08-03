use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_k8s::K8sEcosystem;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/k8s-floating")
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
fn discovers_workload_kinds_skips_configmap_and_helmrelease() {
    let repo = fixture_dir();
    let eco = K8sEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    let names: Vec<_> = manifests
        .iter()
        .map(|m| m.path.file_name().and_then(|n| n.to_str()).unwrap_or(""))
        .collect();

    assert!(names.contains(&"deployment.yaml"), "names={names:?}");
    assert!(names.contains(&"cronjob.yaml"), "names={names:?}");
    assert!(
        !names.contains(&"ignored.yaml"),
        "ConfigMap/HelmRelease-only file must not be discovered: {names:?}"
    );
}

#[test]
fn extracts_containers_and_init_containers_marks_floating() {
    let repo = fixture_dir();
    let eco = K8sEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    let ctx = ctx(&repo);

    let mut findings = Vec::new();
    for m in &manifests {
        findings.extend(eco.extract(m, &ctx).unwrap());
    }

    let floating: Vec<_> = findings.iter().filter(|f| f.is_floating).collect();
    assert!(
        floating
            .iter()
            .any(|f| f.requested == "nginx:latest" && f.name == "nginx"),
        "findings={findings:?}"
    );
    assert!(
        floating
            .iter()
            .any(|f| f.requested == "busybox:1.36" && f.name == "busybox"),
        "initContainer must be extracted: {findings:?}"
    );
    assert!(
        floating
            .iter()
            .any(|f| f.requested == "alpine" && f.name == "alpine"),
        "untagged image must be floating: {findings:?}"
    );
    assert!(
        floating
            .iter()
            .any(|f| f.requested == "python:3.12" && f.name == "python"),
        "CronJob container: {findings:?}"
    );
}

#[test]
fn resolve_and_rewrite_via_env_map_sets_tag_and_kind_metadata() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_K8S_RESOLVE_MAP",
            "nginx:latest=nginx@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,busybox:1.36=busybox@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,alpine=alpine@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc,python:3.12=python@sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        );
    }

    let fixture = fixture_dir();
    let tmp = tempfile::tempdir().unwrap();
    for name in ["deployment.yaml", "cronjob.yaml", "ignored.yaml"] {
        std::fs::copy(fixture.join(name), tmp.path().join(name)).unwrap();
    }

    let eco = K8sEcosystem;
    let ctx = ctx(tmp.path());
    let manifests = eco.discover(tmp.path()).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .filter(|f| f.is_floating)
        .collect();

    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert!(pins.iter().all(|p| p.pinned.contains("@sha256:")));

    let nginx = pins
        .iter()
        .find(|p| p.requested == "nginx:latest")
        .expect("nginx pin");
    assert_eq!(
        nginx.metadata.get("tag").and_then(|v| v.as_str()),
        Some("latest")
    );
    assert_eq!(
        nginx.metadata.get("kind").and_then(|v| v.as_str()),
        Some("Deployment")
    );

    let python = pins
        .iter()
        .find(|p| p.requested == "python:3.12")
        .expect("python pin");
    assert_eq!(
        python.metadata.get("tag").and_then(|v| v.as_str()),
        Some("3.12")
    );
    assert_eq!(
        python.metadata.get("kind").and_then(|v| v.as_str()),
        Some("CronJob")
    );

    let alpine = pins
        .iter()
        .find(|p| p.requested == "alpine")
        .expect("alpine pin");
    assert_eq!(
        alpine.metadata.get("tag").and_then(|v| v.as_str()),
        Some("")
    );

    let dep = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("deployment.yaml"))
        .expect("deployment.yaml");
    let dep_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("deployment.yaml"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(dep, &dep_pins)
        .unwrap()
        .expect("deployment rewrite");
    assert!(
        rw.new_contents
            .contains("nginx@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "got:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents
            .contains("busybox@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        "initContainer pin missing:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents
            .contains("alpine@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
        "untagged pin missing:\n{}",
        rw.new_contents
    );
    assert!(
        !rw.new_contents.contains("nginx:latest"),
        "floating tag should be rewritten:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains("kind: ConfigMap"),
        "non-target docs must remain:\n{}",
        rw.new_contents
    );

    let cj = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("cronjob.yaml"))
        .expect("cronjob.yaml");
    let cj_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("cronjob.yaml"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(cj, &cj_pins)
        .unwrap()
        .expect("cronjob rewrite");
    assert!(
        rw.new_contents
            .contains("python@sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"),
        "got:\n{}",
        rw.new_contents
    );

    unsafe {
        std::env::remove_var("PINNER_K8S_RESOLVE_MAP");
    }
}

#[test]
fn offline_without_map_fails_closed() {
    let _guard = env_lock().lock().unwrap();
    unsafe {
        std::env::remove_var("PINNER_K8S_RESOLVE_MAP");
    }
    let repo = fixture_dir();
    let eco = K8sEcosystem;
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
        msg.contains("offline") || msg.contains("PINNER_K8S_RESOLVE_MAP"),
        "unexpected error: {msg}"
    );
}

#[test]
fn digest_pinned_image_is_not_floating() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pinned.yaml"),
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: pinned
spec:
  selector:
    matchLabels:
      app: pinned
  template:
    metadata:
      labels:
        app: pinned
    spec:
      containers:
        - name: app
          image: nginx@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
"#,
    )
    .unwrap();

    let eco = K8sEcosystem;
    let ctx = ctx(tmp.path());
    let manifests = eco.discover(tmp.path()).unwrap();
    assert_eq!(manifests.len(), 1);
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert_eq!(findings.len(), 1);
    assert!(!findings[0].is_floating);
    assert!(findings[0].requested.contains("@sha256:"));
}
