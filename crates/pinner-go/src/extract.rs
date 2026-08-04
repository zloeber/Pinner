use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest};

pub(crate) fn extract(
    manifest: &Manifest,
    _ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(&manifest.path)?;
    let mut findings = Vec::new();
    let mut in_require_block = false;

    for raw in contents.lines() {
        let line = strip_go_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("require") {
            let rest = rest.trim();
            if rest == "(" {
                in_require_block = true;
                continue;
            }
            if rest == "()" {
                continue;
            }
            if !rest.is_empty()
                && let Some(finding) = finding_from_require(rest, &manifest.path)
            {
                findings.push(finding);
            }
            continue;
        }

        if in_require_block {
            if line == ")" {
                in_require_block = false;
                continue;
            }
            if let Some(finding) = finding_from_require(line, &manifest.path) {
                findings.push(finding);
            }
        }
    }

    Ok(findings)
}

fn finding_from_require(line: &str, path: &std::path::Path) -> Option<Finding> {
    let (name, requested) = parse_require_tokens(line)?;
    Some(Finding {
        ecosystem: EcosystemKind::Go,
        name,
        requested: requested.clone(),
        path: path.to_path_buf(),
        is_floating: is_floating(&requested),
    })
}

/// Split `module/path [version]` into name + version (version may be empty).
fn parse_require_tokens(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.split_whitespace();
    let name = parts.next()?.to_string();
    // Ignore exclude/replace-looking junk; require paths contain at least one slash
    // or a known-looking module path. Allow any non-empty module path token.
    if name.is_empty() {
        return None;
    }
    let requested = parts.next().unwrap_or("").to_string();
    Some((name, requested))
}

fn is_floating(requested: &str) -> bool {
    let requested = requested.trim();
    requested.is_empty() || requested == "latest"
}

fn strip_go_comment(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_floating, parse_require_tokens};

    #[test]
    fn parses_require_lines() {
        assert_eq!(
            parse_require_tokens("github.com/example/lib latest"),
            Some(("github.com/example/lib".into(), "latest".into()))
        );
        assert_eq!(
            parse_require_tokens("github.com/stretchr/testify v1.9.0"),
            Some(("github.com/stretchr/testify".into(), "v1.9.0".into()))
        );
        assert_eq!(
            parse_require_tokens("github.com/example/lib"),
            Some(("github.com/example/lib".into(), String::new()))
        );
    }

    #[test]
    fn floating_signals() {
        assert!(is_floating("latest"));
        assert!(is_floating(""));
        assert!(!is_floating("v1.9.0"));
        assert!(!is_floating("v0.0.0-20181221193216-37e7f081c4d4"));
    }
}
