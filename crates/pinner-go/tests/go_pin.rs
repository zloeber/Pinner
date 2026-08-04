use pinner_ecosystem::{Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Manifest};
use pinner_go::GoEcosystem;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/go-floating")
}

#[test]
fn extracts_floating_go_requires() {
    let repo = fixture();
    let eco = GoEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert!(!manifests.is_empty());
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert!(
        findings.iter().any(|f| f.name == "github.com/example/lib"
            && f.is_floating
            && f.requested == "latest")
    );
    assert!(
        findings
            .iter()
            .any(|f| f.name == "github.com/stretchr/testify" && !f.is_floating)
    );
}

#[test]
fn resolves_from_go_sum_and_rewrites_exact() {
    let eco = GoEcosystem;
    let repo = fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };
    let manifests = eco.discover(&repo).unwrap();
    let findings: Vec<_> = eco
        .extract(&manifests[0], &ctx)
        .unwrap()
        .into_iter()
        .filter(|f| f.is_floating)
        .collect();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    let lib = pins
        .iter()
        .find(|p| p.name == "github.com/example/lib")
        .unwrap();
    assert_eq!(lib.pinned, "v1.2.3");
    assert_eq!(lib.evidence, EvidenceKind::NativeLock);

    let dir = tempdir().unwrap();
    let go_mod = dir.path().join("go.mod");
    fs::copy(repo.join("go.mod"), &go_mod).unwrap();
    let manifest = Manifest {
        ecosystem: EcosystemKind::Go,
        path: go_mod.clone(),
    };
    let rw = eco.rewrite(&manifest, &pins).unwrap().unwrap();
    assert!(rw.new_contents.contains("github.com/example/lib v1.2.3"));
    assert!(!rw.new_contents.contains("latest"));
    fs::write(&go_mod, &rw.new_contents).unwrap();

    let findings2: Vec<_> = eco
        .extract(&manifest, &ctx)
        .unwrap()
        .into_iter()
        .filter(|f| f.is_floating)
        .collect();
    assert!(findings2.is_empty());
    let rw2 = eco.rewrite(&manifest, &[]).unwrap();
    assert!(rw2.is_none());
}

#[test]
fn resolves_from_pinner_go_resolve_map() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("go.mod"),
        "module example.com/demo\n\ngo 1.22\n\nrequire github.com/example/lib latest\n",
    )
    .unwrap();

    // SAFETY: test-only env seam; held behind env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_GO_RESOLVE_MAP",
            "github.com/example/lib=latest:v1.2.3",
        );
    }
    let eco = GoEcosystem;
    let ctx = EcosystemCtx {
        repo: dir.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };
    let manifests = eco.discover(dir.path()).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    unsafe {
        std::env::remove_var("PINNER_GO_RESOLVE_MAP");
    }
    assert_eq!(pins[0].pinned, "v1.2.3");
    assert_eq!(pins[0].evidence, EvidenceKind::Registry);
}

#[test]
fn ignores_parent_go_sum_outside_repo() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: clear map so a parallel env-map test cannot make resolve succeed.
    unsafe {
        std::env::remove_var("PINNER_GO_RESOLVE_MAP");
    }
    let outer = tempdir().unwrap();
    let repo = outer.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    fs::write(
        outer.path().join("go.sum"),
        "github.com/example/lib v9.9.9 h1:parent=\n",
    )
    .unwrap();
    fs::write(
        repo.join("go.mod"),
        "module example.com/demo\n\ngo 1.22\n\nrequire github.com/example/lib latest\n",
    )
    .unwrap();

    let eco = GoEcosystem;
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };
    let manifests = eco.discover(&repo).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let err = eco.resolve(&findings, &ctx).unwrap_err();
    assert!(matches!(
        err,
        pinner_ecosystem::EcosystemError::Offline { .. }
            | pinner_ecosystem::EcosystemError::Resolve { .. }
    ));
}

#[test]
fn discovers_go_work_modules() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("go.work"),
        "go 1.22\n\nuse (\n\t./alpha\n\t./beta\n)\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("alpha")).unwrap();
    fs::create_dir_all(dir.path().join("beta")).unwrap();
    fs::write(
        dir.path().join("alpha/go.mod"),
        "module example.com/alpha\n\ngo 1.22\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("beta/go.mod"),
        "module example.com/beta\n\ngo 1.22\n",
    )
    .unwrap();

    let eco = GoEcosystem;
    let manifests = eco.discover(dir.path()).unwrap();
    assert_eq!(manifests.len(), 2);
    assert!(
        manifests
            .iter()
            .any(|m| m.path.ends_with(Path::new("alpha/go.mod")))
    );
    assert!(
        manifests
            .iter()
            .any(|m| m.path.ends_with(Path::new("beta/go.mod")))
    );
}
