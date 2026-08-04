use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Text,
    Json,
}

#[derive(Debug, Parser)]
#[command(
    name = "pinner",
    version = env!("PINNER_VERSION"),
    about = "Pin floating dependency versions across ecosystems"
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Commands,
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub offline: bool,
    #[arg(long, global = true)]
    pub dry_run: bool,
    #[arg(long, global = true, value_delimiter = ',')]
    pub ecosystem: Option<Vec<String>>,
    #[arg(long, global = true, default_value = "text")]
    pub format: Format,
    /// Interactive compact-list gate before rewrite/lock writes
    #[arg(long, global = true)]
    pub walkthrough: bool,
    /// Force agent mode (JSON output, no TUI prompts)
    #[arg(long, global = true)]
    pub agent: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Resolve floating versions and rewrite manifests / lockfile
    Pin,
    /// Report drift against pinner.lock.json without writing
    Check,
    /// Report floating refs (text/JSON); `--fix` applies pin
    Audit {
        #[arg(long)]
        fix: bool,
    },
    /// Explain why a pin was chosen (name or path substring)
    Explain { target: String },
    /// Detect or install resolver toolchain binaries
    #[command(subcommand)]
    Toolchain(ToolchainCmd),
}

#[derive(Debug, Subcommand)]
pub enum ToolchainCmd {
    /// Show whether required tools are present
    Status,
    /// Install missing tools when policy allows
    Ensure,
}
