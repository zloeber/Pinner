use pinner_ecosystem::EcosystemKind;
use pinner_toolchain::{
    CommandOutput, CommandRunner, ToolchainError, ensure_with_runner, required_tools, status,
};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct FakeRunner {
    present: Mutex<HashSet<String>>,
    installs: Mutex<Vec<String>>,
    commands: Mutex<Vec<String>>,
    installs_are_effective: bool,
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, ToolchainError> {
        self.commands
            .lock()
            .unwrap()
            .push(format!("{program} {}", args.join(" ")));
        if program == "mise" && args.first() == Some(&"install") {
            self.installs.lock().unwrap().push(args.join(" "));
            if self.installs_are_effective {
                for tool in args.iter().skip(1) {
                    let name = tool.split('@').next().unwrap().to_string();
                    self.present.lock().unwrap().insert(name);
                }
            }
            return Ok(CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            });
        }
        if matches!(program, "mise" | "node" | "npm" | "uv" | "docker" | "gh")
            && args == ["--version"]
        {
            if self.present.lock().unwrap().contains(program) {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "1.0.0\n".into(),
                    stderr: String::new(),
                });
            }
            return Err(ToolchainError::Missing {
                tools: vec![program.into()],
            });
        }
        Ok(CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[test]
fn status_reports_mise_entry() {
    let s = status(&[EcosystemKind::Mise]);
    assert!(s.iter().any(|t| t.name == "mise"));
}

#[test]
fn ensure_errors_when_install_disallowed_and_missing() {
    let fake = FakeRunner {
        present: Mutex::new(HashSet::new()),
        installs: Mutex::new(vec![]),
        commands: Mutex::new(vec![]),
        installs_are_effective: true,
    };
    let err = ensure_with_runner(&fake, &[EcosystemKind::Mise], false, false).unwrap_err();
    assert!(matches!(err, ToolchainError::Missing { .. }));
}

#[test]
fn required_tools_maps_ecosystems_in_order_without_duplicates() {
    assert_eq!(
        required_tools(&[
            EcosystemKind::Node,
            EcosystemKind::Python,
            EcosystemKind::Node,
            EcosystemKind::Docker,
            EcosystemKind::Actions,
        ]),
        vec!["node", "npm", "uv", "docker", "gh"]
    );
}

#[test]
fn ensure_uses_mise_for_supported_missing_tools() {
    let fake = FakeRunner {
        present: Mutex::new(HashSet::from(["mise".into()])),
        installs: Mutex::new(vec![]),
        commands: Mutex::new(vec![]),
        installs_are_effective: true,
    };

    let statuses = ensure_with_runner(
        &fake,
        &[EcosystemKind::Python, EcosystemKind::Actions],
        true,
        false,
    )
    .unwrap();

    assert!(statuses.iter().all(|tool| tool.present));
    assert!(
        statuses
            .iter()
            .all(|tool| tool.version.as_deref() == Some("1.0.0"))
    );
    assert_eq!(
        *fake.installs.lock().unwrap(),
        vec!["install uv gh".to_string()]
    );
}

#[test]
fn ensure_never_attempts_to_install_docker() {
    let fake = FakeRunner {
        present: Mutex::new(HashSet::from(["mise".into()])),
        installs: Mutex::new(vec![]),
        commands: Mutex::new(vec![]),
        installs_are_effective: true,
    };

    let err = ensure_with_runner(&fake, &[EcosystemKind::Docker], true, false).unwrap_err();

    assert!(matches!(
        err,
        ToolchainError::Missing { tools } if tools == ["docker"]
    ));
    assert!(fake.installs.lock().unwrap().is_empty());
}

#[test]
fn ensure_does_not_claim_tools_present_when_install_does_not_make_them_runnable() {
    let fake = FakeRunner {
        present: Mutex::new(HashSet::from(["mise".into()])),
        installs: Mutex::new(vec![]),
        commands: Mutex::new(vec![]),
        installs_are_effective: false,
    };

    let err = ensure_with_runner(&fake, &[EcosystemKind::Python], true, false).unwrap_err();

    assert!(matches!(
        err,
        ToolchainError::Missing { tools } if tools == ["uv"]
    ));
}

#[test]
fn ensure_refuses_curl_bootstrap_without_env_gate() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only env seam; serialized via env_lock.
    unsafe {
        std::env::remove_var("PINNER_BOOTSTRAP_MISE");
    }
    let fake = FakeRunner {
        present: Mutex::new(HashSet::new()),
        installs: Mutex::new(vec![]),
        commands: Mutex::new(vec![]),
        installs_are_effective: true,
    };

    let err = ensure_with_runner(&fake, &[EcosystemKind::Mise], true, false).unwrap_err();

    assert!(matches!(
        err,
        ToolchainError::Missing { tools } if tools == ["mise"]
    ));
    let commands = fake.commands.lock().unwrap();
    assert!(
        !commands
            .iter()
            .any(|command| command.starts_with("sh -c curl")),
        "curl|sh must not run without PINNER_BOOTSTRAP_MISE=1: {commands:?}"
    );
}

#[test]
fn ensure_verifies_mise_after_bootstrap_when_gated() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only env seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_BOOTSTRAP_MISE", "1");
    }
    let fake = FakeRunner {
        present: Mutex::new(HashSet::new()),
        installs: Mutex::new(vec![]),
        commands: Mutex::new(vec![]),
        installs_are_effective: true,
    };

    let err = ensure_with_runner(&fake, &[EcosystemKind::Mise], true, false).unwrap_err();
    unsafe {
        std::env::remove_var("PINNER_BOOTSTRAP_MISE");
    }

    assert!(matches!(
        err,
        ToolchainError::Missing { tools } if tools == ["mise"]
    ));
    let commands = fake.commands.lock().unwrap();
    assert!(
        commands
            .iter()
            .any(|command| command.starts_with("sh -c curl")),
        "expected curl bootstrap when gated: {commands:?}"
    );
}

#[test]
fn ensure_offline_never_runs_install_commands() {
    let fake = FakeRunner {
        present: Mutex::new(HashSet::from(["mise".into()])),
        installs: Mutex::new(vec![]),
        commands: Mutex::new(vec![]),
        installs_are_effective: true,
    };

    let err = ensure_with_runner(&fake, &[EcosystemKind::Python], true, true).unwrap_err();

    assert!(matches!(
        err,
        ToolchainError::Missing { tools } if tools == ["uv"]
    ));
    let commands = fake.commands.lock().unwrap();
    assert!(!commands.iter().any(|command| {
        command.starts_with("mise install") || command.starts_with("sh -c curl")
    }));
}
