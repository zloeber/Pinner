use std::env;
use std::io;
use std::process::Command;

use pinner_ecosystem::EcosystemKind;
use thiserror::Error;

use crate::detect::{ToolStatus, statuses_with_runner};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, ToolchainError>;
}

pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, ToolchainError> {
        let output = Command::new(program).args(args).output().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ToolchainError::Missing {
                    tools: vec![program.to_string()],
                }
            } else {
                ToolchainError::Io(error)
            }
        })?;

        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[derive(Debug, Error)]
pub enum ToolchainError {
    #[error("missing required tools: {tools:?}")]
    Missing { tools: Vec<String> },
    #[error("failed to run {program}: exit status {status}: {stderr}")]
    CommandFailed {
        program: String,
        status: i32,
        stderr: String,
    },
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub fn ensure(
    enabled: &[EcosystemKind],
    allow_install: bool,
) -> Result<Vec<ToolStatus>, ToolchainError> {
    ensure_with_runner(&RealCommandRunner, enabled, allow_install)
}

pub fn ensure_with_runner(
    runner: &dyn CommandRunner,
    enabled: &[EcosystemKind],
    allow_install: bool,
) -> Result<Vec<ToolStatus>, ToolchainError> {
    let mut statuses = statuses_with_runner(runner, enabled, false);
    let missing = missing_names(&statuses);
    if missing.is_empty() {
        return Ok(statuses);
    }
    if !allow_install {
        return Err(ToolchainError::Missing { tools: missing });
    }

    let mut mise_available = command_succeeds(runner, "mise", &["--version"]);
    let needs_mise = missing
        .iter()
        .any(|tool| matches!(tool.as_str(), "mise" | "node" | "npm" | "uv" | "gh"));

    if needs_mise && !mise_available {
        if offline() {
            return Err(ToolchainError::Missing { tools: missing });
        }
        run_checked(runner, "sh", &["-c", "curl https://mise.run | sh"])?;
        mise_available = true;
        mark_present(&mut statuses, &["mise"]);
    }

    let mut install_args = vec!["install"];
    if missing
        .iter()
        .any(|tool| matches!(tool.as_str(), "node" | "npm"))
    {
        install_args.push("node@lts");
    }
    if missing.iter().any(|tool| tool == "uv") {
        install_args.push("uv");
    }
    if missing.iter().any(|tool| tool == "gh") {
        install_args.push("gh");
    }

    if install_args.len() > 1 && mise_available {
        run_checked(runner, "mise", &install_args)?;
        for target in install_args.iter().skip(1) {
            match *target {
                "node@lts" => mark_present(&mut statuses, &["node", "npm"]),
                "uv" => mark_present(&mut statuses, &["uv"]),
                "gh" => mark_present(&mut statuses, &["gh"]),
                _ => {}
            }
        }
    }

    let still_missing = missing_names(&statuses);
    if still_missing.is_empty() {
        Ok(statuses)
    } else {
        Err(ToolchainError::Missing {
            tools: still_missing,
        })
    }
}

fn run_checked(
    runner: &dyn CommandRunner,
    program: &str,
    args: &[&str],
) -> Result<CommandOutput, ToolchainError> {
    let output = runner.run(program, args)?;
    if output.status == 0 {
        Ok(output)
    } else {
        Err(ToolchainError::CommandFailed {
            program: program.to_string(),
            status: output.status,
            stderr: output.stderr,
        })
    }
}

fn command_succeeds(runner: &dyn CommandRunner, program: &str, args: &[&str]) -> bool {
    runner
        .run(program, args)
        .is_ok_and(|output| output.status == 0)
}

fn missing_names(statuses: &[ToolStatus]) -> Vec<String> {
    statuses
        .iter()
        .filter(|tool| !tool.present)
        .map(|tool| tool.name.clone())
        .collect()
}

fn mark_present(statuses: &mut [ToolStatus], names: &[&str]) {
    for status in statuses {
        if names.contains(&status.name.as_str()) {
            status.present = true;
        }
    }
}

fn offline() -> bool {
    env::var("PINNER_OFFLINE")
        .is_ok_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}
