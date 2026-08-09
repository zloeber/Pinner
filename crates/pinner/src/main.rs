mod cli;

use std::cell::Cell;
use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use clap::Parser;
use pinner_core::{
    ExplainReport, Policy, RepoIgnore, RunOptions, RunReport, WalkthroughFilter,
    WalkthroughOutcome, audit, check, explain, pin, pin_with_filter, upgrade, upgrade_with_filter,
};
use pinner_ecosystem::{Ecosystem, EcosystemKind, repo_relative};
use pinner_toolchain::{ToolStatus, ensure, status};
use serde_json::json;

use crate::cli::{Cli, Commands, Format, ToolchainCmd};

type Prepared = (Policy, RunOptions, Vec<Arc<dyn Ecosystem>>);
type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
struct UpgradeScriptCommand {
    ecosystem: EcosystemKind,
    manager: &'static str,
    manifest: PathBuf,
    working_dir: PathBuf,
    command: String,
}

#[derive(Debug, Clone)]
struct UpgradeScriptPlan {
    script: String,
    commands: Vec<UpgradeScriptCommand>,
}

struct ScanSpinner {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    clear_width: usize,
}

impl ScanSpinner {
    fn start(message: String) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running_bg = Arc::clone(&running);
        let clear_width = message.len() + 3;

        let handle = thread::spawn(move || {
            let frames = ['|', '/', '-', '\\'];
            let mut idx = 0usize;

            while running_bg.load(Ordering::Relaxed) {
                eprint!("\r{} {}", frames[idx % frames.len()], message);
                let _ = io::stderr().flush();
                idx += 1;
                thread::sleep(Duration::from_millis(90));
            }
        });

        Self {
            running,
            handle: Some(handle),
            clear_width,
        }
    }
}

impl Drop for ScanSpinner {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        eprint!("\r{:<width$}\r", "", width = self.clear_width);
        let _ = io::stderr().flush();
    }
}

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
    if cli.walkthrough && (cli.agent || cli.format == Format::Json || !stdout_is_tty()) {
        return Err("walkthrough requires an interactive TTY (not --agent/--format json)".into());
    }

    let format = effective_format(&cli);

    match &cli.cmd {
        Commands::Audit { fix } => {
            let (policy, opts, ecosystems) = prepare(&cli)?;
            if *fix {
                let spinner = start_scan_spinner(&cli, format, &opts);
                let (report, aborted) = run_pin(&ecosystems, &policy, &opts, cli.walkthrough)?;
                drop(spinner);
                if aborted {
                    eprintln!("walkthrough aborted; nothing written");
                    return Ok(ExitCode::SUCCESS);
                }
                emit_report(&report, format)?;
                Ok(ExitCode::SUCCESS)
            } else {
                let use_progress = matches!(format, Format::Text) && !cli.agent && stderr_is_tty();
                let report = if use_progress {
                    let sink = pinner_ui::StderrAuditProgress::new(true);
                    audit(&ecosystems, &policy, &opts, Some(&sink))?
                } else {
                    let spinner = start_scan_spinner(&cli, format, &opts);
                    let report = audit(&ecosystems, &policy, &opts, None)?;
                    drop(spinner);
                    report
                };
                emit_audit(&report, format)?;
                if report.findings.is_empty() {
                    Ok(ExitCode::SUCCESS)
                } else {
                    Ok(ExitCode::from(1))
                }
            }
        }
        Commands::Explain { target } => {
            let (policy, opts, ecosystems) = prepare(&cli)?;
            let spinner = start_scan_spinner(&cli, format, &opts);
            let report = explain(&ecosystems, &policy, &opts, target)?;
            drop(spinner);
            emit_explain(&report, format)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Toolchain(cmd) => run_toolchain(&cli, cmd, format),
        Commands::Pin => {
            let (policy, opts, ecosystems) = prepare(&cli)?;
            let spinner = start_scan_spinner(&cli, format, &opts);
            let (report, aborted) = run_pin(&ecosystems, &policy, &opts, cli.walkthrough)?;
            drop(spinner);
            if aborted {
                eprintln!("walkthrough aborted; nothing written");
                return Ok(ExitCode::SUCCESS);
            }
            emit_report(&report, format)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Upgrade {
            script,
            continue_on_ecosystem_error,
        } => {
            let (policy, mut opts, ecosystems) = prepare(&cli)?;
            opts.continue_on_ecosystem_error = *continue_on_ecosystem_error;
            if *script {
                let spinner = start_scan_spinner(&cli, format, &opts);
                let plan = build_upgrade_script(&ecosystems, &policy, &opts)?;
                drop(spinner);
                emit_upgrade_script(&plan, format)?;
                return Ok(ExitCode::SUCCESS);
            }
            let spinner = start_scan_spinner(&cli, format, &opts);
            let (report, aborted) = run_upgrade(&ecosystems, &policy, &opts, cli.walkthrough)?;
            drop(spinner);
            if aborted {
                eprintln!("walkthrough aborted; nothing written");
                return Ok(ExitCode::SUCCESS);
            }
            emit_ecosystem_warnings(&report);
            emit_report(&report, format)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Check => {
            let (policy, opts, ecosystems) = prepare(&cli)?;
            let spinner = start_scan_spinner(&cli, format, &opts);
            let report = check(&ecosystems, &policy, &opts)?;
            drop(spinner);
            emit_report(&report, format)?;
            if report.drift.is_empty() {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
    }
}

fn run_pin(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    walkthrough: bool,
) -> CliResult<(RunReport, bool)> {
    if !walkthrough {
        return Ok((pin(ecosystems, policy, opts)?, false));
    }

    let aborted = Cell::new(false);
    let mut filter =
        |pins: &[pinner_ecosystem::Pin]| -> Result<WalkthroughOutcome, pinner_core::CoreError> {
            let outcome = pinner_ui::run_compact_walkthrough(pins)?;
            if matches!(outcome, WalkthroughOutcome::Aborted) {
                aborted.set(true);
            }
            Ok(outcome)
        };
    let filter_ref: &mut WalkthroughFilter<'_> = &mut filter;
    let report = pin_with_filter(ecosystems, policy, opts, Some(filter_ref))?;
    Ok((report, aborted.get()))
}

fn run_upgrade(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    walkthrough: bool,
) -> CliResult<(RunReport, bool)> {
    if !walkthrough {
        return Ok((upgrade(ecosystems, policy, opts)?, false));
    }

    let aborted = Cell::new(false);
    let mut filter =
        |pins: &[pinner_ecosystem::Pin]| -> Result<WalkthroughOutcome, pinner_core::CoreError> {
            let outcome = pinner_ui::run_compact_walkthrough(pins)?;
            if matches!(outcome, WalkthroughOutcome::Aborted) {
                aborted.set(true);
            }
            Ok(outcome)
        };
    let filter_ref: &mut WalkthroughFilter<'_> = &mut filter;
    let report = upgrade_with_filter(ecosystems, policy, opts, Some(filter_ref))?;
    Ok((report, aborted.get()))
}

fn effective_format(cli: &Cli) -> Format {
    if cli.agent { Format::Json } else { cli.format }
}

fn stdout_is_tty() -> bool {
    io::stdout().is_terminal()
}

fn stderr_is_tty() -> bool {
    io::stderr().is_terminal()
}

fn prepare(cli: &Cli) -> CliResult<Prepared> {
    let config_path = resolve_config_path(cli.config.as_deref())?;
    let policy = Policy::load(config_path.as_deref())?;
    let ecosystems_filter = parse_ecosystems(cli.ecosystem.as_deref())?;
    let cwd = std::env::current_dir()?;
    let repo = match cli.path.as_deref() {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => cwd.join(path),
        None => cwd,
    };
    if !repo.is_dir() {
        return Err(format!("scan path is not a directory: {}", repo.display()).into());
    }
    let opts = RunOptions {
        repo,
        dry_run: cli.dry_run,
        offline: cli.offline || policy.offline_default,
        continue_on_ecosystem_error: false,
        recursive: cli.recursive,
        ecosystems_filter,
    };
    Ok((policy, opts, register_ecosystems()))
}

fn start_scan_spinner(cli: &Cli, format: Format, opts: &RunOptions) -> Option<ScanSpinner> {
    if cli.agent || !matches!(format, Format::Text) || !stderr_is_tty() {
        return None;
    }
    let mode = if opts.recursive {
        "recursive"
    } else {
        "current directory only"
    };
    Some(ScanSpinner::start(format!(
        "scanning {} ({mode})",
        display_scan_path(&opts.repo)
    )))
}

fn display_scan_path(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(&cwd).ok().map(|rel| rel.to_path_buf()))
        .map(|rel| {
            if rel.as_os_str().is_empty() {
                ".".to_string()
            } else {
                format!("./{}", rel.display())
            }
        })
        .unwrap_or_else(|| path.display().to_string())
}

fn run_toolchain(cli: &Cli, cmd: &ToolchainCmd, format: Format) -> CliResult<ExitCode> {
    let config_path = resolve_config_path(cli.config.as_deref())?;
    let policy = Policy::load(config_path.as_deref())?;
    let enabled = enabled_kinds(cli, &policy)?;

    match cmd {
        ToolchainCmd::Status => {
            let tools = status(&enabled);
            emit_toolchain(&tools, format)?;
            Ok(ExitCode::SUCCESS)
        }
        ToolchainCmd::Ensure => {
            let tools = ensure(&enabled, policy.toolchain_install, cli.offline)?;
            emit_toolchain(&tools, format)?;
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
        Arc::new(pinner_cargo::CargoEcosystem),
        Arc::new(pinner_go::GoEcosystem),
        Arc::new(pinner_ruby::RubyEcosystem),
        Arc::new(pinner_gitlab::GitlabEcosystem),
        Arc::new(pinner_azure::AzureEcosystem),
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
        "cargo" => Ok(EcosystemKind::Cargo),
        "go" => Ok(EcosystemKind::Go),
        "ruby" => Ok(EcosystemKind::Ruby),
        "gitlab" => Ok(EcosystemKind::Gitlab),
        "azure" => Ok(EcosystemKind::Azure),
        other => Err(format!("unknown ecosystem: {other}").into()),
    }
}

fn emit_report(report: &RunReport, format: Format) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
        Format::Text => {
            if stdout_is_tty() {
                let mut out = io::stdout().lock();
                pinner_ui::emit_pretty_report(report, &mut out)?;
                out.flush()?;
            } else {
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
            if stdout_is_tty() {
                let mut out = io::stdout().lock();
                pinner_ui::emit_pretty_audit(report, &mut out, true)?;
                out.flush()?;
            } else if report.findings.is_empty() {
                println!("no floating findings");
            } else {
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
            if tools.is_empty() {
                println!("No toolchain requirements for enabled ecosystems.");
                return Ok(());
            }

            let tool_width = tools
                .iter()
                .map(|tool| tool.name.len())
                .max()
                .unwrap_or(4)
                .max(4);
            let status_width = 7usize;

            println!(
                "{:<tool_width$}  {:<status_width$}  version",
                "tool",
                "status",
                tool_width = tool_width,
                status_width = status_width
            );
            let blank = "";
            println!(
                "{:-<tool_width$}  {:-<status_width$}  {:-<7}",
                blank,
                blank,
                blank,
                tool_width = tool_width,
                status_width = status_width
            );

            for tool in tools {
                let state = if tool.present { "present" } else { "missing" };
                let version = tool.version.as_deref().unwrap_or("-");
                println!(
                    "{:<tool_width$}  {:<status_width$}  {}",
                    tool.name,
                    state,
                    version,
                    tool_width = tool_width,
                    status_width = status_width
                );
                if let Some(path) = &tool.path {
                    println!(
                        "{:<tool_width$}  {:<status_width$}  path={}",
                        blank,
                        blank,
                        path.display(),
                        tool_width = tool_width,
                        status_width = status_width
                    );
                }
            }
        }
    }
    Ok(())
}

fn emit_ecosystem_warnings(report: &RunReport) {
    for warning in &report.ecosystem_warnings {
        eprintln!("warning: {warning}");
    }
}

fn build_upgrade_script(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
) -> Result<UpgradeScriptPlan, Box<dyn std::error::Error>> {
    let mut commands = Vec::new();
    let mut seen = BTreeSet::new();
    let gitignore = RepoIgnore::new(&opts.repo);

    for ecosystem in ecosystems {
        if !policy.is_enabled(ecosystem.kind()) {
            continue;
        }
        if let Some(filter) = &opts.ecosystems_filter
            && !filter.contains(&ecosystem.kind())
        {
            continue;
        }

        for manifest in ecosystem.discover(&opts.repo)? {
            let rel_manifest = repo_relative(&opts.repo, &manifest.path);
            if policy.is_ignored(&rel_manifest) || gitignore.is_ignored(&rel_manifest) {
                continue;
            }
            if let Some((manager, manifest_cmd)) =
                upgrade_command_for_manifest(ecosystem.kind(), &rel_manifest, &opts.repo)
            {
                let dir = rel_manifest
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                let key = format!(
                    "{}|{}|{}",
                    ecosystem.kind().as_str(),
                    dir.display(),
                    manifest_cmd
                );
                if !seen.insert(key) {
                    continue;
                }
                commands.push(UpgradeScriptCommand {
                    ecosystem: ecosystem.kind(),
                    manager,
                    manifest: rel_manifest,
                    working_dir: dir,
                    command: manifest_cmd,
                });
            }
        }
    }

    commands.sort_by(|a, b| {
        a.ecosystem
            .as_str()
            .cmp(b.ecosystem.as_str())
            .then_with(|| a.working_dir.cmp(&b.working_dir))
            .then_with(|| a.command.cmp(&b.command))
    });

    let script = render_upgrade_script(&commands);
    Ok(UpgradeScriptPlan { script, commands })
}

fn upgrade_command_for_manifest(
    kind: EcosystemKind,
    rel_manifest: &Path,
    repo: &Path,
) -> Option<(&'static str, String)> {
    let filename = rel_manifest.file_name()?.to_string_lossy();
    let rel_file = repo_relative(repo, rel_manifest).display().to_string();

    match kind {
        EcosystemKind::Mise => Some(("mise", "mise upgrade".to_string())),
        EcosystemKind::Node => Some(("npm", "npm update".to_string())),
        EcosystemKind::Python => {
            if filename == "pyproject.toml" {
                Some(("uv", "uv lock --upgrade".to_string()))
            } else if filename.starts_with("requirements") && filename.ends_with(".txt") {
                Some((
                    "uv",
                    format!("uv pip compile --upgrade {}", shell_quote(&rel_file)),
                ))
            } else {
                None
            }
        }
        EcosystemKind::Terraform => Some(("terraform", "terraform init -upgrade".to_string())),
        EcosystemKind::Helm => Some(("helm", "helm dependency update".to_string())),
        EcosystemKind::Cargo => Some(("cargo", "cargo update".to_string())),
        EcosystemKind::Go => Some(("go", "go get -u ./... && go mod tidy".to_string())),
        EcosystemKind::Ruby => Some(("bundler", "bundle update".to_string())),
        // These are upgraded via resolvers/API lookups, not package-manager CLIs.
        EcosystemKind::Docker
        | EcosystemKind::Actions
        | EcosystemKind::K8s
        | EcosystemKind::Gitlab
        | EcosystemKind::Azure => None,
    }
}

fn render_upgrade_script(commands: &[UpgradeScriptCommand]) -> String {
    let mut out = String::new();
    out.push_str("#!/usr/bin/env bash\n");
    out.push_str("set -euo pipefail\n\n");
    out.push_str("# Generated by pinner upgrade --script\n");
    out.push_str("# Review commands before running in CI or production.\n\n");

    if commands.is_empty() {
        out.push_str(
            "echo 'No native package-manager upgrade commands found for selected ecosystems.'\n",
        );
        return out;
    }

    let mut current: Option<EcosystemKind> = None;
    for cmd in commands {
        if current != Some(cmd.ecosystem) {
            if current.is_some() {
                out.push('\n');
            }
            out.push_str("# ");
            out.push_str(cmd.ecosystem.as_str());
            out.push_str(" (");
            out.push_str(cmd.manager);
            out.push_str(")\n");
            current = Some(cmd.ecosystem);
        }
        out.push_str("# manifest: ");
        out.push_str(&cmd.manifest.display().to_string());
        out.push('\n');
        if is_current_dir(&cmd.working_dir) {
            out.push_str(&cmd.command);
            out.push('\n');
        } else {
            out.push_str("(cd ");
            out.push_str(&shell_quote_path(&cmd.working_dir));
            out.push_str(" && ");
            out.push_str(&cmd.command);
            out.push_str(")\n");
        }
    }

    out
}

fn emit_upgrade_script(
    plan: &UpgradeScriptPlan,
    format: Format,
) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        Format::Json => {
            let payload = json!({
                "script": plan.script,
                "commands": plan.commands.iter().map(|cmd| {
                    json!({
                        "ecosystem": cmd.ecosystem.as_str(),
                        "manager": cmd.manager,
                        "manifest": cmd.manifest,
                        "working_dir": cmd.working_dir,
                        "command": cmd.command,
                    })
                }).collect::<Vec<_>>()
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        Format::Text => {
            print!("{}", plan.script);
            io::stdout().flush()?;
        }
    }
    Ok(())
}

fn shell_quote_path(path: &Path) -> String {
    shell_quote(&path.display().to_string())
}

fn is_current_dir(path: &Path) -> bool {
    path.as_os_str().is_empty() || path == Path::new(".")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::{UpgradeScriptCommand, is_current_dir, render_upgrade_script};
    use pinner_ecosystem::EcosystemKind;
    use std::path::PathBuf;

    #[test]
    fn script_omits_cd_for_current_dir() {
        let script = render_upgrade_script(&[UpgradeScriptCommand {
            ecosystem: EcosystemKind::Node,
            manager: "npm",
            manifest: PathBuf::from("package.json"),
            working_dir: PathBuf::new(),
            command: "npm update".to_string(),
        }]);

        assert!(script.contains("npm update\n"));
        assert!(!script.contains("cd ''"));
        assert!(!script.contains("(cd "));
    }

    #[test]
    fn detects_current_directory_paths() {
        assert!(is_current_dir(PathBuf::new().as_path()));
        assert!(is_current_dir(PathBuf::from(".").as_path()));
        assert!(!is_current_dir(PathBuf::from("subdir").as_path()));
    }
}
