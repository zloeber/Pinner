use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

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
            EcosystemKind::Terraform | EcosystemKind::Helm | EcosystemKind::K8s => &[],
            EcosystemKind::Cargo | EcosystemKind::Go | EcosystemKind::Ruby
            | EcosystemKind::Gitlab | EcosystemKind::Azure => &[],
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
            EcosystemKind::Terraform | EcosystemKind::Helm | EcosystemKind::K8s => false,
            EcosystemKind::Cargo | EcosystemKind::Go | EcosystemKind::Ruby
            | EcosystemKind::Gitlab | EcosystemKind::Azure => false,
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

pub(crate) fn path_with_mise_dirs() -> Option<std::ffi::OsString> {
    let mut paths = env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .collect::<Vec<_>>();

    for candidate in known_mise_paths().filter(|candidate| is_executable_file(candidate)) {
        if let Some(directory) = candidate.parent()
            && !paths.iter().any(|path| path == directory)
        {
            paths.push(directory.to_path_buf());
        }
    }

    env::join_paths(paths).ok()
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    path_with_mise_dirs()
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable_file(candidate))
}

fn known_mise_paths() -> impl Iterator<Item = PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);
    home.into_iter()
        .flat_map(|home| [home.join(".local/bin/mise"), home.join(".mise/bin/mise")])
}

fn is_executable_file(candidate: &Path) -> bool {
    if !candidate.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        candidate
            .metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs::{self, File};
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::is_executable_file;

    #[test]
    fn path_candidates_must_be_executable() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("pinner-toolchain-{}-{unique}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let candidate = directory.join("tool");
        File::create(&candidate).unwrap();

        fs::set_permissions(&candidate, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_executable_file(&candidate));

        fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable_file(&candidate));

        fs::remove_dir_all(directory).unwrap();
    }
}
