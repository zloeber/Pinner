mod cli;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use pinner_core::{ExplainReport, Policy, RunOptions, RunReport, audit, check, explain, pin};
use pinner_ecosystem::{Ecosystem, EcosystemKind};
use pinner_toolchain::{ToolStatus, ensure, status};

use crate::cli::{Cli, Commands, Format, ToolchainCmd};

type Prepared = (Policy, RunOptions, Vec<Arc<dyn Ecosystem>>);
type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> CliResult<ExitCode> {
    match &cli.cmd {
        Commands::Audit { fix } => {
            let (policy, opts, ecosystems) = prepare(&cli)?;
            if *fix {
                // Pin only the floating non-allowlisted findings audit would report.
                let report = pin(&ecosystems, &policy, &opts)?;
                emit_report(&report, cli.format)?;
                Ok(ExitCode::SUCCESS)
            } else {
                let report = audit(&ecosystems, &policy, &opts)?;
                emit_audit(&report, cli.format)?;
                if report.findings.is_empty() {
                    Ok(ExitCode::SUCCESS)
                } else {
                    Ok(ExitCode::from(1))
                }
            }
        }
        Commands::Explain { target } => {
            let (policy, opts, ecosystems) = prepare(&cli)?;
            let report = explain(&ecosystems, &policy, &opts, target)?;
            emit_explain(&report, cli.format)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Toolchain(cmd) => run_toolchain(&cli, cmd),
        Commands::Pin => {
            let (policy, opts, ecosystems) = prepare(&cli)?;
            let report = pin(&ecosystems, &policy, &opts)?;
            emit_report(&report, cli.format)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Check => {
            let (policy, opts, ecosystems) = prepare(&cli)?;
            let report = check(&ecosystems, &policy, &opts)?;
            emit_report(&report, cli.format)?;
            if report.drift.is_empty() {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
    }
}

fn prepare(cli: &Cli) -> CliResult<Prepared> {
    let config_path = resolve_config_path(cli.config.as_deref())?;
    let policy = Policy::load(config_path.as_deref())?;
    let ecosystems_filter = parse_ecosystems(cli.ecosystem.as_deref())?;
    let opts = RunOptions {
        repo: std::env::current_dir()?,
        dry_run: cli.dry_run,
        offline: cli.offline || policy.offline_default,
        ecosystems_filter,
    };
    Ok((policy, opts, register_ecosystems()))
}

fn run_toolchain(cli: &Cli, cmd: &ToolchainCmd) -> CliResult<ExitCode> {
    let config_path = resolve_config_path(cli.config.as_deref())?;
    let policy = Policy::load(config_path.as_deref())?;
    let enabled = enabled_kinds(cli, &policy)?;

    match cmd {
        ToolchainCmd::Status => {
            let tools = status(&enabled);
            emit_toolchain(&tools, cli.format)?;
            Ok(ExitCode::SUCCESS)
        }
        ToolchainCmd::Ensure => {
            let tools = ensure(&enabled, policy.toolchain_install, cli.offline)?;
            emit_toolchain(&tools, cli.format)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn enabled_kinds(
    cli: &Cli,
    policy: &Policy,
) -> Result<Vec<EcosystemKind>, Box<dyn std::error::Error>> {
    if let Some(filter) = parse_ecosystems(cli.ecosystem.as_deref())? {
        return Ok(filter
            .into_iter()
            .filter(|kind| policy.is_enabled(*kind))
            .collect());
    }
    Ok(policy.enabled.clone())
}

fn register_ecosystems() -> Vec<Arc<dyn Ecosystem>> {
    vec![
        Arc::new(pinner_mise::MiseEcosystem::default()),
        Arc::new(pinner_node::NodeEcosystem),
        Arc::new(pinner_python::PythonEcosystem),
        Arc::new(pinner_docker::DockerEcosystem),
        Arc::new(pinner_actions::ActionsEcosystem),
        Arc::new(pinner_terraform::TerraformEcosystem),
        Arc::new(pinner_helm::HelmEcosystem),
        Arc::new(pinner_k8s::K8sEcosystem),
    ]
}

fn resolve_config_path(
    explicit: Option<&Path>,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    if let Some(path) = explicit {
        return Ok(Some(path.to_path_buf()));
    }
    let default = std::env::current_dir()?.join("pinner.toml");
    if default.is_file() {
        Ok(Some(default))
    } else {
        Ok(None)
    }
}

fn parse_ecosystems(
    values: Option<&[String]>,
) -> Result<Option<Vec<EcosystemKind>>, Box<dyn std::error::Error>> {
    let Some(values) = values else {
        return Ok(None);
    };
    let mut kinds = Vec::with_capacity(values.len());
    for value in values {
        kinds.push(parse_ecosystem(value)?);
    }
    Ok(Some(kinds))
}

fn parse_ecosystem(value: &str) -> Result<EcosystemKind, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "mise" => Ok(EcosystemKind::Mise),
        "node" => Ok(EcosystemKind::Node),
        "python" => Ok(EcosystemKind::Python),
        "docker" => Ok(EcosystemKind::Docker),
        "actions" => Ok(EcosystemKind::Actions),
        "terraform" => Ok(EcosystemKind::Terraform),
        "helm" => Ok(EcosystemKind::Helm),
        "k8s" => Ok(EcosystemKind::K8s),
        other => Err(format!("unknown ecosystem: {other}").into()),
    }
}

fn emit_report(report: &RunReport, format: Format) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
        Format::Text => {
            println!(
                "pins: {}  rewrites: {}  findings: {}  drift: {}",
                report.pins.len(),
                report.rewrites.len(),
                report.findings.len(),
                report.drift.len()
            );
            for finding in &report.findings {
                println!(
                    "finding {} {} requested={} floating={}",
                    finding.path.display(),
                    finding.name,
                    finding.requested,
                    finding.is_floating
                );
            }
            for item in &report.drift {
                println!(
                    "drift {} {} expected={} actual={}",
                    item.path.display(),
                    item.name,
                    item.expected,
                    item.actual
                );
            }
        }
    }
    Ok(())
}

fn emit_audit(report: &RunReport, format: Format) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        Format::Json => {
            // Compact findings-only payload (matches audit contract / CLI tests).
            let payload = serde_json::json!({ "findings": report.findings });
            println!("{}", serde_json::to_string(&payload)?);
        }
        Format::Text => {
            if report.findings.is_empty() {
                println!("no floating findings");
            }
            for finding in &report.findings {
                println!(
                    "{} {} requested={} path={}",
                    finding.ecosystem.as_str(),
                    finding.name,
                    finding.requested,
                    finding.path.display()
                );
            }
        }
    }
    Ok(())
}

fn emit_explain(report: &ExplainReport, format: Format) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
        Format::Text => {
            println!(
                "{} @ {} requested={} pinned={} evidence={:?} — {}",
                report.name,
                report.path.display(),
                report.requested,
                report.pinned,
                report.evidence,
                report.detail
            );
        }
    }
    Ok(())
}

fn emit_toolchain(tools: &[ToolStatus], format: Format) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(tools)?);
        }
        Format::Text => {
            for tool in tools {
                let state = if tool.present { "present" } else { "missing" };
                let version = tool.version.as_deref().unwrap_or("-");
                println!("{}: {state} ({version})", tool.name);
            }
        }
    }
    Ok(())
}
