use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_helm::HelmEcosystem;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/helm-floating")
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
fn discovers_chart_yaml_gitops_and_values_files() {
    let repo = fixture_dir();
    let eco = HelmEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    let names: Vec<_> = manifests
        .iter()
        .map(|m| m.path.file_name().and_then(|n| n.to_str()).unwrap_or(""))
        .collect();

    assert!(names.contains(&"Chart.yaml"), "names={names:?}");
    assert!(names.contains(&"helmrelease.yaml"), "names={names:?}");
    assert!(names.contains(&"application.yaml"), "names={names:?}");
    assert!(
        names.contains(&"values.yaml"),
        "values.yaml must be discovered for image pins: {names:?}"
    );
    assert!(
        names.contains(&"values-prod.yaml"),
        "values*.yaml must be discovered: {names:?}"
    );
}

#[test]
fn extracts_floating_images_from_values_yaml() {
    let repo = fixture_dir();
    let eco = HelmEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    let ctx = ctx(&repo);

    let mut findings = Vec::new();
    for m in &manifests {
        let name = m.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("values") {
            findings.extend(eco.extract(m, &ctx).unwrap());
        }
    }

    assert!(
        findings.iter().any(|f| {
            f.is_floating
                && f.name == "ghcr.io/example/app"
                && f.requested == "ghcr.io/example/app:latest"
        }),
        "repository+tag image: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|f| { f.is_floating && f.name == "nginx" && f.requested == "nginx:latest" }),
        "string image under sidecar: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|f| { f.is_floating && f.name == "redis" && f.requested == "redis:latest" }),
        "values-prod.yaml string image: {findings:?}"
    );
}

#[test]
fn discovers_chart_yml_filename() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Chart.yml"),
        "apiVersion: v2\nname: tiny\nversion: 0.1.0\ndependencies: []\n",
    )
    .unwrap();
    let manifests = HelmEcosystem.discover(tmp.path()).unwrap();
    assert_eq!(manifests.len(), 1);
    assert_eq!(
        manifests[0].path.file_name().and_then(|n| n.to_str()),
        Some("Chart.yml")
    );
}

#[test]
fn extracts_floating_chart_deps_and_gitops_versions() {
    let repo = fixture_dir();
    let eco = HelmEcosystem;
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
            .any(|f| f.name == "redis" && f.requested == "*"),
        "findings={findings:?}"
    );
    assert!(
        floating
            .iter()
            .any(|f| f.name == "postgresql" && f.requested == "^12.1.0"),
        "findings={findings:?}"
    );
    assert!(
        floating
            .iter()
            .any(|f| f.name == "nginx" && f.requested == "latest"),
        "findings={findings:?}"
    );
    assert!(
        floating
            .iter()
            .any(|f| f.name == "ingress-nginx" && f.requested.is_empty()),
        "missing version should be floating empty requested: {findings:?}"
    );
    assert!(
        floating
            .iter()
            .any(|f| f.name == "podinfo" && f.requested == ">=6.0.0"),
        "HelmRelease chart version: {findings:?}"
    );
    assert!(
        floating
            .iter()
            .any(|f| f.name == "argo-cd" && f.requested == "~2.4.0"),
        "Application targetRevision: {findings:?}"
    );
    assert!(
        !floating.iter().any(|f| f.name == "cert-manager"),
        "exact chart dep must not be floating"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.name == "cert-manager" && !f.is_floating && f.requested == "1.14.0"),
        "exact dep still extracted: {findings:?}"
    );
}

#[test]
fn resolve_and_rewrite_via_env_map() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_HELM_RESOLVE_MAP",
            "redis@*=18.6.1,postgresql@^12.1.0=12.5.8,nginx@latest=15.5.0,ingress-nginx@=4.10.0,podinfo@>=6.0.0=6.5.4,argo-cd@~2.4.0=2.4.17,ghcr.io/example/app@ghcr.io/example/app:latest=ghcr.io/example/app@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,nginx@nginx:latest=nginx@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,redis@redis:latest=redis@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        );
    }

    let fixture = fixture_dir();
    let tmp = tempfile::tempdir().unwrap();
    for name in [
        "Chart.yaml",
        "helmrelease.yaml",
        "application.yaml",
        "values.yaml",
        "values-prod.yaml",
    ] {
        std::fs::copy(fixture.join(name), tmp.path().join(name)).unwrap();
    }

    let eco = HelmEcosystem;
    let ctx = ctx(tmp.path());
    let manifests = eco.discover(tmp.path()).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .filter(|f| f.is_floating)
        .collect();

    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert!(
        pins.iter()
            .any(|p| p.name == "redis" && p.pinned == "18.6.1")
    );
    assert!(
        pins.iter()
            .any(|p| p.name == "postgresql" && p.pinned == "12.5.8")
    );
    assert!(
        pins.iter()
            .any(|p| p.name == "nginx" && p.pinned == "15.5.0")
    );
    assert!(
        pins.iter()
            .any(|p| p.name == "ingress-nginx" && p.pinned == "4.10.0")
    );
    assert!(
        pins.iter()
            .any(|p| p.name == "podinfo" && p.pinned == "6.5.4")
    );
    assert!(
        pins.iter()
            .any(|p| p.name == "argo-cd" && p.pinned == "2.4.17")
    );
    assert!(
        pins.iter().any(|p| {
            p.name == "ghcr.io/example/app"
                && p.pinned
                    == "ghcr.io/example/app@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }),
        "values image pin missing: {pins:?}"
    );
    let redis = pins
        .iter()
        .find(|p| p.name == "redis" && p.pinned == "18.6.1")
        .expect("redis chart pin");
    assert_eq!(
        redis.metadata.get("chart").and_then(|v| v.as_str()),
        Some("redis")
    );
    assert_eq!(
        redis.metadata.get("repository").and_then(|v| v.as_str()),
        Some("https://charts.bitnami.com/bitnami")
    );
    let argo = pins
        .iter()
        .find(|p| p.name == "argo-cd")
        .expect("argo-cd pin");
    assert_eq!(
        argo.metadata.get("repository").and_then(|v| v.as_str()),
        Some("https://argoproj.github.io/argo-helm")
    );

    let chart = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("Chart.yaml"))
        .expect("Chart.yaml");
    let chart_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("Chart.yaml"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(chart, &chart_pins)
        .unwrap()
        .expect("Chart.yaml rewrite");
    assert!(
        rw.new_contents.contains("18.6.1") && rw.new_contents.contains("12.5.8"),
        "expected pinned chart deps, got:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains("1.14.0"),
        "exact dep preserved:\n{}",
        rw.new_contents
    );
    std::fs::write(&rw.path, &rw.new_contents).unwrap();

    let hr = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("helmrelease.yaml"))
        .expect("helmrelease.yaml");
    let hr_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("helmrelease.yaml"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(hr, &hr_pins)
        .unwrap()
        .expect("HelmRelease rewrite");
    assert!(
        rw.new_contents.contains("6.5.4"),
        "expected HelmRelease version pin, got:\n{}",
        rw.new_contents
    );

    let app = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("application.yaml"))
        .expect("application.yaml");
    let app_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("application.yaml"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(app, &app_pins)
        .unwrap()
        .expect("Application rewrite");
    assert!(
        rw.new_contents.contains("2.4.17"),
        "expected Application targetRevision pin, got:\n{}",
        rw.new_contents
    );

    let values = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("values.yaml"))
        .expect("values.yaml");
    let values_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("values.yaml"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(values, &values_pins)
        .unwrap()
        .expect("values.yaml rewrite");
    assert!(
        rw.new_contents.contains(
            "ghcr.io/example/app@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ) || (rw.new_contents.contains("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            && rw.new_contents.contains("ghcr.io/example/app")),
        "expected values repository+tag pin, got:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains(
            "nginx@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ),
        "expected values string image pin, got:\n{}",
        rw.new_contents
    );

    unsafe {
        std::env::remove_var("PINNER_HELM_RESOLVE_MAP");
    }
}

#[test]
fn rewrite_matches_same_chart_name_by_repository() {
    let _guard = env_lock().lock().unwrap();
    unsafe {
        std::env::set_var(
            "PINNER_HELM_RESOLVE_MAP",
            "redis@*=18.6.1,redis@^1.0.0=9.9.9",
        );
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Chart.yaml"),
        r#"apiVersion: v2
name: umbrella
version: 0.1.0
dependencies:
  - name: redis
    version: "*"
    repository: https://charts.bitnami.com/bitnami
  - name: redis
    version: "^1.0.0"
    repository: https://example.com/other-charts
"#,
    )
    .unwrap();

    let eco = HelmEcosystem;
    let ctx = ctx(tmp.path());
    let manifests = eco.discover(tmp.path()).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .filter(|f| f.is_floating)
        .collect();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert_eq!(pins.len(), 2);
    assert!(pins.iter().any(|p| {
        p.pinned == "18.6.1"
            && p.metadata.get("repository").and_then(|v| v.as_str())
                == Some("https://charts.bitnami.com/bitnami")
    }));
    assert!(pins.iter().any(|p| {
        p.pinned == "9.9.9"
            && p.metadata.get("repository").and_then(|v| v.as_str())
                == Some("https://example.com/other-charts")
    }));

    let chart = &manifests[0];
    let rw = eco
        .rewrite(chart, &pins)
        .unwrap()
        .expect("Chart.yaml rewrite");
    let value: serde_yaml::Value = serde_yaml::from_str(&rw.new_contents).unwrap();
    let deps = value.get("dependencies").unwrap().as_sequence().unwrap();
    assert_eq!(
        deps[0].get("version").and_then(|v| v.as_str()),
        Some("18.6.1")
    );
    assert_eq!(
        deps[1].get("version").and_then(|v| v.as_str()),
        Some("9.9.9")
    );

    unsafe {
        std::env::remove_var("PINNER_HELM_RESOLVE_MAP");
    }
}

#[test]
fn offline_without_map_fails_closed() {
    let _guard = env_lock().lock().unwrap();
    unsafe {
        std::env::remove_var("PINNER_HELM_RESOLVE_MAP");
    }
    let repo = fixture_dir();
    let eco = HelmEcosystem;
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
        msg.contains("offline") || msg.contains("PINNER_HELM_RESOLVE_MAP"),
        "unexpected error: {msg}"
    );
}
