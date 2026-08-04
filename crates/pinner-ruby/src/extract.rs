use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest};

pub(crate) fn extract(
    manifest: &Manifest,
    ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(&manifest.path)?;
    let mut findings = Vec::new();

    for raw in contents.lines() {
        let line = strip_ruby_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(finding) = finding_from_gem_line(line, &manifest.path, ctx) {
            findings.push(finding);
        }
    }

    Ok(findings)
}

fn finding_from_gem_line(
    line: &str,
    path: &std::path::Path,
    ctx: &EcosystemCtx<'_>,
) -> Option<Finding> {
    let (name, requested) = parse_gem_call(line)?;
    Some(Finding {
        ecosystem: EcosystemKind::Ruby,
        name,
        requested: requested.clone(),
        path: path.to_path_buf(),
        is_floating: is_floating(&requested, ctx.pin_exact_ranges),
    })
}

/// Parse simple `gem "name"` / `gem 'name', 'constraint'` calls.
/// Returns `(name, requested)` where requested is empty when no version arg.
fn parse_gem_call(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let rest = line.strip_prefix("gem")?;
    let rest = rest.trim_start();
    if !rest.starts_with(['"', '\'']) {
        return None;
    }
    let (name, after_name) = parse_quoted(rest)?;
    let after_name = after_name.trim_start();
    if after_name.is_empty() || after_name.starts_with('#') {
        return Some((name, String::new()));
    }
    let after_comma = after_name.strip_prefix(',')?;
    let after_comma = after_comma.trim_start();
    if after_comma.is_empty() || after_comma.starts_with('#') {
        return Some((name, String::new()));
    }
    // Version constraint is the next quoted string; ignore further kwargs.
    if after_comma.starts_with(['"', '\'']) {
        let (requested, _) = parse_quoted(after_comma)?;
        return Some((name, requested));
    }
    // Non-string second arg (e.g. require: false only) — treat as no version.
    Some((name, String::new()))
}

fn parse_quoted(input: &str) -> Option<(String, &str)> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let quote = bytes[0];
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i = i.saturating_add(2);
            continue;
        }
        if bytes[i] == quote {
            let value = input[1..i].to_string();
            return Some((value, &input[i + 1..]));
        }
        i += 1;
    }
    None
}

fn is_floating(requested: &str, pin_exact_ranges: bool) -> bool {
    let requested = requested.trim();
    if requested.is_empty() || requested == "*" || requested.eq_ignore_ascii_case("latest") {
        return true;
    }
    if is_exact_version(requested) {
        return false;
    }
    // Constraint operators (`>=`, `~>`, …) only float when pin_exact_ranges.
    pin_exact_ranges
}

fn is_exact_version(requested: &str) -> bool {
    let s = requested
        .strip_prefix('=')
        .map(str::trim)
        .unwrap_or(requested);
    if s.is_empty() {
        return false;
    }
    if s.chars().any(|c| matches!(c, '>' | '<' | '~' | '!' | '*')) {
        return false;
    }
    s.starts_with(|c: char| c.is_ascii_digit())
}

fn strip_ruby_comment(line: &str) -> &str {
    // Gem lines use `#` comments; ignore `#` inside quotes via a simple scan.
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == b'\\' {
                i = i.saturating_add(2);
                continue;
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            quote = Some(b);
            i += 1;
            continue;
        }
        if b == b'#' {
            return &line[..i];
        }
        i += 1;
    }
    line
}

#[cfg(test)]
mod tests {
    use super::{is_floating, parse_gem_call};

    #[test]
    fn parses_gem_calls() {
        assert_eq!(
            parse_gem_call(r#"gem "rake""#),
            Some(("rake".into(), String::new()))
        );
        assert_eq!(
            parse_gem_call(r#"gem 'rspec', '>= 3.0'"#),
            Some(("rspec".into(), ">= 3.0".into()))
        );
        assert_eq!(
            parse_gem_call(r#"gem "rails", "7.2.1""#),
            Some(("rails".into(), "7.2.1".into()))
        );
        assert_eq!(
            parse_gem_call(r#"gem "foo", require: false"#),
            Some(("foo".into(), String::new()))
        );
    }

    #[test]
    fn floating_signals() {
        assert!(is_floating("", false));
        assert!(is_floating("*", false));
        assert!(is_floating("latest", false));
        assert!(!is_floating(">= 3.0", false));
        assert!(is_floating(">= 3.0", true));
        assert!(is_floating("~> 1.0", true));
        assert!(!is_floating("13.2.1", true));
        assert!(!is_floating("= 13.2.1", true));
    }
}
