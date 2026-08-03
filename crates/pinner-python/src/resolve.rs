use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin, absolute_in_repo,
};
use pinner_toolchain::{CommandRunner, RealCommandRunner};

use crate::PythonEcosystem;

impl PythonEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let mut pins = Vec::with_capacity(findings.len());
        let mut lock_cache: HashMap<PathBuf, Option<HashMap<String, String>>> = HashMap::new();

        for finding in findings {
            pins.push(resolve_one(&runner, finding, ctx, &mut lock_cache)?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Python
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Python,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    let abs_path = absolute_in_repo(ctx.repo, &finding.path);
    let dir = abs_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    if let Some(version) = find_python_lock_version(&dir, &finding.name, lock_cache)? {
        return Ok(Pin {
            ecosystem: EcosystemKind::Python,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: version,
            path: finding.path.clone(),
            evidence: EvidenceKind::NativeLock,
            metadata: Default::default(),
        });
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let pinned = resolve_via_uv_pip_compile(runner, finding).map_err(|hint| {
        EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        }
    })?;

    Ok(Pin {
        ecosystem: EcosystemKind::Python,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence: EvidenceKind::Registry,
        metadata: Default::default(),
    })
}

fn find_python_lock_version(
    start: &Path,
    name: &str,
    lock_cache: &mut HashMap<PathBuf, Option<HashMap<String, String>>>,
) -> Result<Option<String>, EcosystemError> {
    let mut current = start.to_path_buf();
    loop {
        if !lock_cache.contains_key(&current) {
            let map = read_python_lock_versions(&current)?;
            lock_cache.insert(current.clone(), map);
        }
        if let Some(Some(versions)) = lock_cache.get(&current)
            && let Some(version) = versions.get(name)
        {
            return Ok(Some(version.clone()));
        }
        if !current.pop() {
            break;
        }
    }
    Ok(None)
}

/// Prefer uv.lock, then poetry.lock, then pdm.lock.
fn read_python_lock_versions(
    dir: &Path,
) -> Result<Option<HashMap<String, String>>, EcosystemError> {
    for name in ["uv.lock", "poetry.lock", "pdm.lock"] {
        if let Some(map) = read_toml_package_lock(&dir.join(name))? {
            return Ok(Some(map));
        }
    }
    Ok(None)
}

fn read_toml_package_lock(
    lock_path: &Path,
) -> Result<Option<HashMap<String, String>>, EcosystemError> {
    if !lock_path.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(lock_path)?;
    let value: toml::Value = toml::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: lock_path.to_path_buf(),
        message: e.to_string(),
    })?;

    let Some(packages) = value.get("package").and_then(|p| p.as_array()) else {
        // uv/poetry/pdm use [[package]] → toml key "package" array
        return Ok(None);
    };

    let mut map = HashMap::new();
    for entry in packages {
        let Some(table) = entry.as_table() else {
            continue;
        };
        let Some(name) = table.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(version) = table.get("version").and_then(|v| v.as_str()) else {
            continue;
        };
        map.insert(name.to_string(), version.to_string());
    }
    Ok(Some(map))
}

/// Resolve via `uv pip compile` when `uv` is available.
fn resolve_via_uv_pip_compile(
    runner: &dyn CommandRunner,
    finding: &Finding,
) -> Result<String, String> {
    // Probe uv presence first — brief: "only when available".
    let probe = runner
        .run("uv", &["--version"])
        .map_err(|err| format!("uv not available: {err}"))?;
    if probe.status != 0 {
        return Err(format!(
            "uv not available (status {}): {}",
            probe.status,
            probe.stderr.trim()
        ));
    }

    let req_line = if finding.requested.is_empty() {
        finding.name.clone()
    } else if is_exact_looking(&finding.requested) {
        format!("{}=={}", finding.name, finding.requested)
    } else {
        format!("{}{}", finding.name, finding.requested)
    };

    let tmp = std::env::temp_dir().join(format!(
        "pinner-python-{}-{}.txt",
        std::process::id(),
        finding.name.replace('/', "_")
    ));
    std::fs::write(&tmp, format!("{req_line}\n"))
        .map_err(|err| format!("write temp requirements: {err}"))?;

    let tmp_str = tmp.to_string_lossy();
    let output = runner
        .run(
            "uv",
            &[
                "pip",
                "compile",
                tmp_str.as_ref(),
                "--no-header",
                "--no-annotate",
                "-q",
                "-o",
                "-",
            ],
        )
        .map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            format!("uv pip compile: {err}")
        })?;
    let _ = std::fs::remove_file(&tmp);

    if output.status != 0 {
        return Err(format!(
            "uv pip compile failed (status {}): {}",
            output.status,
            output.stderr.trim()
        ));
    }

    parse_compiled_version(&output.stdout, &finding.name)
        .ok_or_else(|| format!("uv pip compile did not pin {}", finding.name))
}

fn is_exact_looking(requested: &str) -> bool {
    let r = requested.trim();
    !r.is_empty()
        && !r.starts_with('=')
        && !r.starts_with('>')
        && !r.starts_with('<')
        && !r.starts_with('~')
        && !r.starts_with('!')
        && !r.starts_with('^')
        && !r.starts_with('*')
}

fn parse_compiled_version(stdout: &str, name: &str) -> Option<String> {
    let name_lower = name.to_ascii_lowercase();
    for line in stdout.lines() {
        let line = line.split('#').next()?.trim();
        if line.is_empty() {
            continue;
        }
        let (pkg, rest) = split_name_spec(line)?;
        if pkg.to_ascii_lowercase() != name_lower {
            continue;
        }
        if let Some(ver) = rest.strip_prefix("==") {
            return Some(ver.trim().to_string());
        }
    }
    None
}

fn split_name_spec(line: &str) -> Option<(String, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b'.' {
            i += 1;
        } else {
            break;
        }
    }
    if i == 0 {
        return None;
    }
    Some((line[..i].to_string(), line[i..].trim_start()))
}

#[cfg(test)]
mod tests {
    use super::read_toml_package_lock;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reads_poetry_lock_packages() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("poetry.lock");
        fs::write(
            &path,
            "[[package]]\nname = \"requests\"\nversion = \"2.32.3\"\ndescription = \"HTTP\"\n",
        )
        .unwrap();
        let map = read_toml_package_lock(&path).unwrap().unwrap();
        assert_eq!(map.get("requests").map(String::as_str), Some("2.32.3"));
    }

    #[test]
    fn reads_pdm_lock_packages() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pdm.lock");
        fs::write(
            &path,
            "[[package]]\nname = \"httpx\"\nversion = \"0.27.0\"\nrequires_python = \">=3.8\"\n",
        )
        .unwrap();
        let map = read_toml_package_lock(&path).unwrap().unwrap();
        assert_eq!(map.get("httpx").map(String::as_str), Some("0.27.0"));
    }
}
