use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Finding, Manifest, Pin, ResolveMode,
};
use pinner_ruby::RubyEcosystem;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ruby-floating")
}

fn upgrade_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ruby-upgrade")
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
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Gemfile"),
        "source \"https://rubygems.org\"\ngem \"rake\"\n",
    )
    .unwrap();

    // SAFETY: test-only env seam; serialized via env_lock.
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

#[test]
fn upgrade_prefers_resolve_map_over_native_lock() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_RUBY_RESOLVE_MAP", "rake=13.2.1:13.3.0");
    }
    let eco = RubyEcosystem;
    let repo = upgrade_fixture();
    let stale_lock = [Pin {
        ecosystem: EcosystemKind::Ruby,
        name: "rake".into(),
        requested: "13.2.1".into(),
        pinned: "13.2.1".into(),
        path: PathBuf::from("Gemfile"),
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
        ecosystem: EcosystemKind::Ruby,
        name: "rake".into(),
        requested: "13.2.1".into(),
        path: PathBuf::from("Gemfile"),
        is_floating: false,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    unsafe {
        std::env::remove_var("PINNER_RUBY_RESOLVE_MAP");
    }
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].pinned, "13.3.0");
    assert_eq!(pins[0].metadata["previous"], "13.2.1");
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
        std::env::set_var("PINNER_RUBY_RESOLVE_MAP", "rake=13.2.1:13.2.1");
    }
    let eco = RubyEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Ruby,
        name: "rake".into(),
        requested: "13.2.1".into(),
        path: PathBuf::from("Gemfile"),
        is_floating: false,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    unsafe {
        std::env::remove_var("PINNER_RUBY_RESOLVE_MAP");
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
        std::env::remove_var("PINNER_RUBY_RESOLVE_MAP");
    }
    let eco = RubyEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Ruby,
        name: "rake".into(),
        requested: "13.2.1".into(),
        path: PathBuf::from("Gemfile"),
        is_floating: false,
    };
    let err = eco.resolve(&[finding], &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("offline") || msg.contains("PINNER_RUBY_RESOLVE_MAP"),
        "upgrade must not freeze on native lock; got {msg}"
    );
}
