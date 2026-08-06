use std::path::Path;

use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest};

pub(crate) fn extract(
    manifest: &Manifest,
    _ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let file_name = manifest
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    match file_name {
        ".mise.toml" => extract_mise_toml(&manifest.path),
        ".tool-versions" => extract_tool_versions(&manifest.path),
        _ => Ok(Vec::new()),
    }
}

fn extract_mise_toml(path: &Path) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let value: toml::Value = toml::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let Some(tools) = value.get("tools").and_then(|t| t.as_table()) else {
        return Ok(Vec::new());
    };

    let mut findings = Vec::new();
    for (name, version_value) in tools {
        let requested = version_to_string(version_value);
        findings.push(Finding {
            ecosystem: EcosystemKind::Mise,
            name: name.clone(),
            requested: requested.clone(),
            path: path.to_path_buf(),
            is_floating: !is_exact_semver(&requested),
        });
    }
    Ok(findings)
}

fn extract_tool_versions(path: &Path) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let mut findings = Vec::new();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        let requested = parts.next().unwrap_or("").to_string();
        findings.push(Finding {
            ecosystem: EcosystemKind::Mise,
            name: name.to_string(),
            requested: requested.clone(),
            path: path.to_path_buf(),
            is_floating: !is_exact_semver(&requested),
        });
    }
    Ok(findings)
}

fn version_to_string(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        // Inline / full tables: `awscli = { version = "latest", ... }` and
        // `[tools."http:gkg"]` / `version = "0.24.0"`.
        toml::Value::Table(table) => table
            .get("version")
            .map(version_to_string)
            .unwrap_or_else(|| value.to_string()),
        other => other.to_string(),
    }
}

/// Exact semver: `MAJOR.MINOR.PATCH` with optional prerelease/build suffix.
pub(crate) fn is_exact_semver(version: &str) -> bool {
    let version = version.trim();
    if version.is_empty() {
        return false;
    }
    let bytes = version.as_bytes();
    let mut i = 0;

    // MAJOR
    if !consume_digits(bytes, &mut i) {
        return false;
    }
    if !consume_dot(bytes, &mut i) {
        return false;
    }
    // MINOR
    if !consume_digits(bytes, &mut i) {
        return false;
    }
    if !consume_dot(bytes, &mut i) {
        return false;
    }
    // PATCH
    if !consume_digits(bytes, &mut i) {
        return false;
    }

    // Optional ([.-].+)
    if i == bytes.len() {
        return true;
    }
    if bytes[i] == b'.' || bytes[i] == b'-' {
        i += 1;
        return i < bytes.len();
    }
    false
}

fn consume_digits(bytes: &[u8], i: &mut usize) -> bool {
    let start = *i;
    while *i < bytes.len() && bytes[*i].is_ascii_digit() {
        *i += 1;
    }
    *i > start
}

fn consume_dot(bytes: &[u8], i: &mut usize) -> bool {
    if *i < bytes.len() && bytes[*i] == b'.' {
        *i += 1;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::is_exact_semver;

    #[test]
    fn exact_semver_matches() {
        assert!(is_exact_semver("1.2.3"));
        assert!(is_exact_semver("1.2.3-beta"));
        assert!(is_exact_semver("1.2.3.4"));
        assert!(!is_exact_semver("3.12"));
        assert!(!is_exact_semver("latest"));
        assert!(!is_exact_semver("lts"));
        assert!(!is_exact_semver(""));
        assert!(!is_exact_semver("^1.2.3"));
        assert!(!is_exact_semver("~1.2.3"));
        assert!(!is_exact_semver(">=1.2.3"));
    }
}
