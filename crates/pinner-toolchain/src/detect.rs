use std::collections::HashSet;
use std::env;
use std::path::PathBuf;

use pinner_ecosystem::EcosystemKind;
use serde::Serialize;

use crate::{CommandRunner, RealCommandRunner};

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub name: String,
    pub required_by: Vec<EcosystemKind>,
    pub present: bool,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
}

pub fn required_tools(enabled: &[EcosystemKind]) -> Vec<&'static str> {
    let mut seen = HashSet::new();
    let mut tools = Vec::new();

    for ecosystem in enabled {
        let ecosystem_tools: &[&str] = match ecosystem {
            EcosystemKind::Mise => &["mise"],
            EcosystemKind::Node => &["node", "npm"],
            EcosystemKind::Python => &["uv"],
            EcosystemKind::Docker => &["docker"],
            EcosystemKind::Actions => &["gh"],
        };
        for &tool in ecosystem_tools {
            if seen.insert(tool) {
                tools.push(tool);
            }
        }
    }

    tools
}

pub fn status(enabled: &[EcosystemKind]) -> Vec<ToolStatus> {
    statuses_with_runner(&RealCommandRunner, enabled, true)
}

pub(crate) fn statuses_with_runner(
    runner: &dyn CommandRunner,
    enabled: &[EcosystemKind],
    include_paths: bool,
) -> Vec<ToolStatus> {
    required_tools(enabled)
        .into_iter()
        .map(|name| {
            let output = runner.run(name, &["--version"]).ok();
            let successful = output.as_ref().filter(|output| output.status == 0);
            ToolStatus {
                name: name.to_string(),
                required_by: required_by(name, enabled),
                present: successful.is_some(),
                version: successful.and_then(version_from_output),
                path: include_paths.then(|| find_on_path(name)).flatten(),
            }
        })
        .collect()
}

fn required_by(name: &str, enabled: &[EcosystemKind]) -> Vec<EcosystemKind> {
    enabled
        .iter()
        .copied()
        .filter(|ecosystem| match ecosystem {
            EcosystemKind::Mise => name == "mise",
            EcosystemKind::Node => matches!(name, "node" | "npm"),
            EcosystemKind::Python => name == "uv",
            EcosystemKind::Docker => name == "docker",
            EcosystemKind::Actions => name == "gh",
        })
        .collect()
}

fn version_from_output(output: &crate::CommandOutput) -> Option<String> {
    let value = if output.stdout.trim().is_empty() {
        output.stderr.trim()
    } else {
        output.stdout.trim()
    };
    (!value.is_empty()).then(|| value.to_string())
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}
