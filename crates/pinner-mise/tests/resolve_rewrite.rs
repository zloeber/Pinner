use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Manifest, Pin,
    ResolveMode,
};
use pinner_mise::MiseEcosystem;
use pinner_toolchain::{CommandOutput, CommandRunner, ToolchainError};
use tempfile::tempdir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct RecordingRunner {
    calls: Arc<Mutex<Vec<String>>>,
    latest: String,
}

impl CommandRunner for RecordingRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, ToolchainError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{program} {}", args.join(" ")));
        Ok(CommandOutput {
            status: 0,
            stdout: format!("{}\n", self.latest),
            stderr: String::new(),
        })
    }
}

#[test]
fn prefers_lock_pin_over_tool() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only env seam; serialized via env_lock.
    unsafe {
        std::env::remove_var("PINNER_MISE_RESOLVE_MAP");
    }

    let calls = Arc::new(Mutex::new(Vec::new()));
    let runner = Arc::new(RecordingRunner {
        calls: calls.clone(),
        latest: "99.0.0".into(),
    });
    let eco = MiseEcosystem::with_runner(runner);

    let finding = Finding {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "latest".into(),
        path: PathBuf::from(".mise.toml"),
        is_floating: true,
    };
    let lock = Pin {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "latest".into(),
        pinned: "22.11.0".into(),
        path: PathBuf::from(".mise.toml"),
        evidence: EvidenceKind::Tool,
        metadata: Default::default(),
    };
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[lock],
        offline: false,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };

    let pins = eco.resolve(&[finding], &ctx).unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].pinned, "22.11.0");
    assert_eq!(pins[0].evidence, EvidenceKind::Lock);
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn resolve_map_used_before_mise() {
    let _guard = env_lock().lock().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let runner = Arc::new(RecordingRunner {
        calls: calls.clone(),
        latest: "99.0.0".into(),
    });
    let eco = MiseEcosystem::with_runner(runner);

    let finding = Finding {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "latest".into(),
        path: PathBuf::from(".mise.toml"),
        is_floating: true,
    };
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: false,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };

    // SAFETY: test-only env seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_MISE_RESOLVE_MAP", "node=22.11.0,python=3.12.7");
    }
    let pins = eco.resolve(&[finding], &ctx);
    unsafe {
        std::env::remove_var("PINNER_MISE_RESOLVE_MAP");
    }

    let pins = pins.unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].pinned, "22.11.0");
    assert_eq!(pins[0].evidence, EvidenceKind::Tool);
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn offline_without_lock_or_map_errors() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only env seam; serialized via env_lock.
    unsafe {
        std::env::remove_var("PINNER_MISE_RESOLVE_MAP");
    }

    let eco = MiseEcosystem::with_runner(Arc::new(RecordingRunner {
        calls: Arc::new(Mutex::new(Vec::new())),
        latest: "22.11.0".into(),
    }));
    let finding = Finding {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "latest".into(),
        path: PathBuf::from(".mise.toml"),
        is_floating: true,
    };
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let err = eco.resolve(&[finding], &ctx).unwrap_err();
    assert!(matches!(
        err,
        EcosystemError::Offline {
            name,
            requested
        } if name == "node" && requested == "latest"
    ));
}

#[test]
fn resolves_via_mise_latest() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only env seam; serialized via env_lock.
    unsafe {
        std::env::remove_var("PINNER_MISE_RESOLVE_MAP");
    }

    let calls = Arc::new(Mutex::new(Vec::new()));
    let runner = Arc::new(RecordingRunner {
        calls: calls.clone(),
        latest: "22.11.0".into(),
    });
    let eco = MiseEcosystem::with_runner(runner);
    let finding = Finding {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "latest".into(),
        path: PathBuf::from(".mise.toml"),
        is_floating: true,
    };
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: false,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let pins = eco.resolve(&[finding], &ctx).unwrap();
    assert_eq!(pins[0].pinned, "22.11.0");
    assert_eq!(pins[0].evidence, EvidenceKind::Tool);
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "mise latest node")
    );
}

#[test]
fn rewrite_mise_toml_sets_exact_version() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".mise.toml");
    std::fs::write(&path, "[tools]\nnode = \"latest\"\n").unwrap();
    let manifest = Manifest {
        ecosystem: EcosystemKind::Mise,
        path: path.clone(),
    };
    let pins = vec![Pin {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "latest".into(),
        pinned: "22.11.0".into(),
        path: path.clone(),
        evidence: EvidenceKind::Tool,
        metadata: Default::default(),
    }];
    let rw = MiseEcosystem::default()
        .rewrite(&manifest, &pins)
        .unwrap()
        .unwrap();
    assert!(rw.new_contents.contains("22.11.0"));
    assert!(!rw.new_contents.contains("latest"));
}

#[test]
fn rewrite_tool_versions_replaces_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".tool-versions");
    std::fs::write(&path, "node latest\npython 3.12\n").unwrap();
    let manifest = Manifest {
        ecosystem: EcosystemKind::Mise,
        path: path.clone(),
    };
    let pins = vec![Pin {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "latest".into(),
        pinned: "22.11.0".into(),
        path: path.clone(),
        evidence: EvidenceKind::Tool,
        metadata: Default::default(),
    }];
    let rw = MiseEcosystem::default()
        .rewrite(&manifest, &pins)
        .unwrap()
        .unwrap();
    assert!(rw.new_contents.contains("node 22.11.0"));
    assert!(rw.new_contents.contains("python 3.12"));
    assert!(!rw.new_contents.contains("node latest"));
}
