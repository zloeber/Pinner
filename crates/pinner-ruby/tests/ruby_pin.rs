use pinner_ecosystem::{Ecosystem, EcosystemCtx, ResolveMode, EcosystemKind, EvidenceKind, Manifest};
use pinner_ruby::RubyEcosystem;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ruby-floating")
}

#[test]
fn extracts_floating_gemfile_deps() {
    let repo = fixture();
    let eco = RubyEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert!(!manifests.is_empty());
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert!(findings.iter().any(|f| f.name == "rake" && f.is_floating));
    assert!(findings.iter().any(|f| f.name == "rspec" && f.is_floating));
}

#[test]
fn resolves_from_gemfile_lock_and_rewrites_exact() {
    let eco = RubyEcosystem;
    let repo = fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let manifests = eco.discover(&repo).unwrap();
    let findings: Vec<_> = eco
        .extract(&manifests[0], &ctx)
        .unwrap()
        .into_iter()
        .filter(|f| f.is_floating)
        .collect();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert_eq!(
        pins.iter().find(|p| p.name == "rake").unwrap().pinned,
        "13.2.1"
    );
    assert_eq!(
        pins.iter().find(|p| p.name == "rspec").unwrap().pinned,
        "3.13.0"
    );
    assert!(pins.iter().all(|p| p.evidence == EvidenceKind::NativeLock));

    let dir = tempdir().unwrap();
    let gemfile = dir.path().join("Gemfile");
    fs::copy(repo.join("Gemfile"), &gemfile).unwrap();
    let manifest = Manifest {
        ecosystem: EcosystemKind::Ruby,
        path: gemfile.clone(),
    };
    let rw = eco.rewrite(&manifest, &pins).unwrap().unwrap();
    assert!(rw.new_contents.contains(r#"gem "rake", "13.2.1""#));
    assert!(rw.new_contents.contains(r#"gem "rspec", "3.13.0""#));
    assert!(!rw.new_contents.contains(r#"">= 3.0""#));
    fs::write(&gemfile, &rw.new_contents).unwrap();

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
fn resolves_from_pinner_ruby_resolve_map() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Gemfile"),
        "source \"https://rubygems.org\"\ngem \"rake\"\n",
    )
    .unwrap();

    // SAFETY: test-only env seam; serial within this process for this var.
    unsafe {
        std::env::set_var("PINNER_RUBY_RESOLVE_MAP", "rake=:13.2.1");
    }
    let eco = RubyEcosystem;
    let ctx = EcosystemCtx {
        repo: dir.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let manifests = eco.discover(dir.path()).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    unsafe {
        std::env::remove_var("PINNER_RUBY_RESOLVE_MAP");
    }
    assert_eq!(pins[0].pinned, "13.2.1");
    assert_eq!(pins[0].evidence, EvidenceKind::Registry);
}
