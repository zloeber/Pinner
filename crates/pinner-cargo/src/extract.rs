use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest};
use toml::Value;

const DEP_SECTIONS: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];

pub(crate) fn extract(
    manifest: &Manifest,
    ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(&manifest.path)?;
    let value: Value = toml::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: manifest.path.clone(),
        message: e.to_string(),
    })?;

    let mut findings = Vec::new();
    extract_dep_tables(&value, &manifest.path, ctx, &mut findings);

    if let Some(targets) = value.get("target").and_then(|t| t.as_table()) {
        for (_cfg, target_table) in targets {
            extract_dep_tables(target_table, &manifest.path, ctx, &mut findings);
        }
    }

    Ok(findings)
}

fn extract_dep_tables(
    table: &Value,
    path: &std::path::Path,
    ctx: &EcosystemCtx<'_>,
    findings: &mut Vec<Finding>,
) {
    for section in DEP_SECTIONS {
        let Some(deps) = table.get(*section).and_then(|v| v.as_table()) else {
            continue;
        };
        for (name, req) in deps {
            if let Some(finding) = finding_from_dep(name, req, path, ctx) {
                findings.push(finding);
            }
        }
    }
}

fn finding_from_dep(
    name: &str,
    req: &Value,
    path: &std::path::Path,
    ctx: &EcosystemCtx<'_>,
) -> Option<Finding> {
    match req {
        Value::String(requested) => Some(Finding {
            ecosystem: EcosystemKind::Cargo,
            name: name.to_string(),
            requested: requested.clone(),
            path: path.to_path_buf(),
            is_floating: is_floating(requested, ctx.pin_exact_ranges),
        }),
        Value::Table(table) => {
            if table.contains_key("path") || table.contains_key("git") {
                return None;
            }
            let requested = table.get("version").and_then(|v| v.as_str())?;
            Some(Finding {
                ecosystem: EcosystemKind::Cargo,
                name: name.to_string(),
                requested: requested.to_string(),
                path: path.to_path_buf(),
                is_floating: is_floating(requested, ctx.pin_exact_ranges),
            })
        }
        _ => None,
    }
}

fn is_floating(requested: &str, pin_exact_ranges: bool) -> bool {
    let requested = requested.trim();
    if requested.is_empty() || requested == "*" || requested == "latest" {
        return true;
    }
    if is_bare_major(requested) {
        return true;
    }
    // Cargo treats "1.0" as a caret range; only x.y.z is exact.
    if is_partial_numeric_version(requested) {
        return true;
    }
    if pin_exact_ranges
        && (requested.starts_with('^') || requested.starts_with('~') || requested.starts_with(">="))
    {
        return true;
    }
    false
}

fn is_partial_numeric_version(requested: &str) -> bool {
    let bytes = requested.as_bytes();
    let mut i = 0;
    if !consume_digits(bytes, &mut i) {
        return false;
    }
    let mut parts = 1;
    while i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        if !consume_digits(bytes, &mut i) {
            return false;
        }
        parts += 1;
    }
    i == bytes.len() && parts < 3
}

/// Cargo bare major like `"1"` (not `"1.0"` / `"1.0.0"`).
fn is_bare_major(requested: &str) -> bool {
    !requested.is_empty() && requested.chars().all(|c| c.is_ascii_digit())
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
    use super::{is_bare_major, is_floating, is_partial_numeric_version};

    #[test]
    fn floating_signals() {
        assert!(is_floating("*", false));
        assert!(is_floating("latest", false));
        assert!(is_floating("1", false));
        assert!(is_bare_major("1"));
        assert!(!is_bare_major("1.0"));
        assert!(is_partial_numeric_version("1.0"));
        assert!(!is_floating("^1", false));
        assert!(is_floating("^1", true));
        assert!(is_floating("~1.0", true));
        assert!(is_floating(">=1", true));
        assert!(!is_floating("1.0.210", true));
    }
}
