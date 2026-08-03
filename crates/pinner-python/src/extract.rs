use std::path::Path;

use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest};

pub(crate) fn extract(
    manifest: &Manifest,
    ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let file_name = manifest
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if file_name == "pyproject.toml" {
        extract_pyproject(&manifest.path, ctx)
    } else if is_requirements_file(file_name) {
        extract_requirements(&manifest.path, ctx)
    } else {
        Ok(Vec::new())
    }
}

fn is_requirements_file(name: &str) -> bool {
    name == "requirements.txt" || (name.starts_with("requirements") && name.ends_with(".txt"))
}

fn extract_pyproject(path: &Path, ctx: &EcosystemCtx<'_>) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let value: toml::Value = toml::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let mut findings = Vec::new();

    if let Some(deps) = value
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for dep in deps {
            if let Some(req) = dep.as_str() {
                push_requirement(&mut findings, path, req, ctx);
            }
        }
    }

    // Optional dependency groups: [project.optional-dependencies]
    if let Some(optional) = value
        .get("project")
        .and_then(|p| p.get("optional-dependencies"))
        .and_then(|t| t.as_table())
    {
        for (_group, deps) in optional {
            let Some(arr) = deps.as_array() else {
                continue;
            };
            for dep in arr {
                if let Some(req) = dep.as_str() {
                    push_requirement(&mut findings, path, req, ctx);
                }
            }
        }
    }

    // Poetry-style: [tool.poetry.dependencies] (skip python key)
    if let Some(poetry_deps) = value
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (name, ver) in poetry_deps {
            if name == "python" {
                continue;
            }
            let requested = match ver {
                toml::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            findings.push(Finding {
                ecosystem: EcosystemKind::Python,
                name: name.clone(),
                requested: requested.clone(),
                path: path.to_path_buf(),
                is_floating: is_floating_spec(&requested, ctx.pin_exact_ranges),
            });
        }
    }

    Ok(findings)
}

fn extract_requirements(
    path: &Path,
    ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let mut findings = Vec::new();
    for line in contents.lines() {
        let line = strip_requirement_comment(line).trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        push_requirement(&mut findings, path, line, ctx);
    }
    Ok(findings)
}

fn strip_requirement_comment(line: &str) -> &str {
    // Keep URLs with #egg=; strip trailing " # comment"
    if let Some(idx) = line.find(" #") {
        &line[..idx]
    } else {
        line
    }
}

fn push_requirement(findings: &mut Vec<Finding>, path: &Path, req: &str, ctx: &EcosystemCtx<'_>) {
    let Some((name, requested)) = parse_pep508(req) else {
        return;
    };
    findings.push(Finding {
        ecosystem: EcosystemKind::Python,
        name,
        requested: requested.clone(),
        path: path.to_path_buf(),
        is_floating: is_floating_spec(&requested, ctx.pin_exact_ranges),
    });
}

/// Parse a PEP 508 / requirements line into `(name, version_spec)`.
/// Exact `==X.Y.Z` becomes requested `"X.Y.Z"` so check can match `pin.pinned`.
fn parse_pep508(req: &str) -> Option<(String, String)> {
    let req = req.trim();
    if req.is_empty() {
        return None;
    }
    // Drop environment markers.
    let req = req.split(';').next()?.trim();
    if req.is_empty() {
        return None;
    }
    // Skip direct URL / path / VCS refs for v1.
    if req.contains("://") || req.starts_with('.') || req.starts_with('/') {
        return None;
    }

    let bytes = req.as_bytes();
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
    let name = req[..i].to_string();
    let mut rest = req[i..].trim_start();

    // Drop extras: [security,socks]
    if rest.starts_with('[') {
        let end = rest.find(']')?;
        rest = rest[end + 1..].trim_start();
    }

    let requested = if let Some(ver) = rest.strip_prefix("==") {
        let ver = ver.trim();
        // Keep wildcards like ==2.* as the raw spec for floating detection.
        if ver.contains('*') {
            format!("=={ver}")
        } else {
            ver.to_string()
        }
    } else if rest.is_empty() {
        String::new()
    } else {
        rest.to_string()
    };

    Some((name, requested))
}

fn is_floating_spec(requested: &str, pin_exact_ranges: bool) -> bool {
    let requested = requested.trim();
    if requested.is_empty() || requested == "*" {
        return true;
    }
    if requested.contains('*') {
        return true;
    }
    // Exact version (already stripped of ==).
    if is_exact_version(requested) {
        return false;
    }
    if pin_exact_ranges {
        // Any remaining operator / range is floating.
        return true;
    }
    // Without pin_exact_ranges: treat >= / > / unpinned-like as floating; leave others.
    requested.starts_with(">=")
        || requested.starts_with('>')
        || requested.starts_with("<=")
        || requested.starts_with('<')
        || requested.starts_with("~=")
        || requested.starts_with("!=")
}

fn is_exact_version(version: &str) -> bool {
    let version = version.trim();
    if version.is_empty() {
        return false;
    }
    // Reject if it still has comparison operators.
    if version.starts_with('=')
        || version.starts_with('>')
        || version.starts_with('<')
        || version.starts_with('~')
        || version.starts_with('!')
        || version.starts_with('^')
    {
        return false;
    }
    let bytes = version.as_bytes();
    let mut i = 0;
    if !consume_digits(bytes, &mut i) {
        return false;
    }
    let mut parts = 1;
    while i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        if !consume_digits(bytes, &mut i) {
            // allow prerelease after first complete numeric part via '-' below
            return false;
        }
        parts += 1;
    }
    if parts < 2 {
        return false;
    }
    if i == bytes.len() {
        return true;
    }
    if bytes[i] == b'-' || bytes[i] == b'+' {
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

#[cfg(test)]
mod tests {
    use super::{is_floating_spec, parse_pep508};

    #[test]
    fn parses_pep508_variants() {
        assert_eq!(
            parse_pep508("requests>=2.0"),
            Some(("requests".into(), ">=2.0".into()))
        );
        assert_eq!(
            parse_pep508("requests==2.32.3"),
            Some(("requests".into(), "2.32.3".into()))
        );
        assert_eq!(
            parse_pep508("requests"),
            Some(("requests".into(), String::new()))
        );
        assert_eq!(
            parse_pep508("requests[security]>=2.0 ; python_version >= \"3.8\""),
            Some(("requests".into(), ">=2.0".into()))
        );
    }

    #[test]
    fn floating_signals() {
        assert!(is_floating_spec("", true));
        assert!(is_floating_spec("*", true));
        assert!(is_floating_spec(">=2.0", true));
        assert!(is_floating_spec(">=2.0", false));
        assert!(!is_floating_spec("2.32.3", true));
    }
}
