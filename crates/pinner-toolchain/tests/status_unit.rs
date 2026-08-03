use pinner_ecosystem::EcosystemKind;
use pinner_toolchain::{
    CommandOutput, CommandRunner, ToolchainError, ensure_with_runner, required_tools, status,
};
use std::collections::HashSet;
use std::sync::Mutex;

struct FakeRunner {
    present: Mutex<HashSet<String>>,
    installs: Mutex<Vec<String>>,
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, ToolchainError> {
        if program == "mise" && args.first() == Some(&"install") {
            self.installs.lock().unwrap().push(args.join(" "));
            for tool in args.iter().skip(1) {
                let name = tool.split('@').next().unwrap().to_string();
                self.present.lock().unwrap().insert(name);
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
    };
    let err = ensure_with_runner(&fake, &[EcosystemKind::Mise], false).unwrap_err();
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
    };

    let statuses = ensure_with_runner(
        &fake,
        &[EcosystemKind::Python, EcosystemKind::Actions],
        true,
    )
    .unwrap();

    assert!(statuses.iter().all(|tool| tool.present));
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
    };

    let err = ensure_with_runner(&fake, &[EcosystemKind::Docker], true).unwrap_err();

    assert!(matches!(
        err,
        ToolchainError::Missing { tools } if tools == ["docker"]
    ));
    assert!(fake.installs.lock().unwrap().is_empty());
}
