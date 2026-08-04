use std::collections::HashMap;

use pinner_ecosystem::{EcosystemError, Manifest, Pin, Rewrite};

pub(crate) fn rewrite(
    manifest: &Manifest,
    pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    if pins.is_empty() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&manifest.path)?;
    let pin_by_name: HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.name.as_str(), p.pinned.as_str()))
        .collect();

    let mut out = String::new();
    let mut changed = false;

    for raw in contents.lines() {
        let (rewritten, did) = rewrite_gem_line(raw, &pin_by_name);
        changed |= did;
        out.push_str(&rewritten);
        out.push('\n');
    }

    if !changed {
        return Ok(None);
    }

    if !contents.is_empty() && !contents.ends_with('\n') {
        out.pop();
    }

    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents: out,
    }))
}

/// Insert or replace the version argument on a `gem` line.
fn rewrite_gem_line(raw: &str, pin_by_name: &HashMap<&str, &str>) -> (String, bool) {
    let code = strip_ruby_comment(raw);
    let trimmed = code.trim();
    if !trimmed.starts_with("gem") {
        return (raw.to_string(), false);
    }

    let indent_end = code.len() - code.trim_start().len();
    let after_indent = &code[indent_end..];
    let Some(rest) = after_indent.strip_prefix("gem") else {
        return (raw.to_string(), false);
    };
    let rest = rest.trim_start();
    if !rest.starts_with(['"', '\'']) {
        return (raw.to_string(), false);
    }
    let Some((name, after_name)) = parse_quoted(rest) else {
        return (raw.to_string(), false);
    };
    let Some(pinned) = pin_by_name.get(name.as_str()) else {
        return (raw.to_string(), false);
    };

    let quote = rest.as_bytes()[0] as char;
    let after_name = after_name.trim_start();

    // Preserve trailing comment from the original line.
    let comment = comment_suffix(raw);

    // Determine remaining args after name (excluding version string if present).
    let remainder = if let Some(after_comma) = after_name.strip_prefix(',') {
        let after_comma = after_comma.trim_start();
        if after_comma.starts_with(['"', '\'']) {
            // Drop existing version string; keep anything after it.
            if let Some((_, after_ver)) = parse_quoted(after_comma) {
                let after_ver = after_ver.trim();
                if after_ver.is_empty() {
                    String::new()
                } else if let Some(rest) = after_ver.strip_prefix(',') {
                    format!(", {}", rest.trim_start())
                } else {
                    format!(", {after_ver}")
                }
            } else {
                format!(", {}", after_comma.trim_end())
            }
        } else {
            // Non-string args (kwargs): keep `, kwargs`.
            format!(", {}", after_comma.trim_end())
        }
    } else {
        String::new()
    };

    // Already pinned exactly?
    if let Some(after_comma) = after_name.strip_prefix(',') {
        let after_comma = after_comma.trim_start();
        if after_comma.starts_with(['"', '\''])
            && let Some((old, _)) = parse_quoted(after_comma)
            && old == *pinned
            && remainder.is_empty()
        {
            return (raw.to_string(), false);
        }
    }

    let mut rewritten = String::new();
    rewritten.push_str(&code[..indent_end]);
    rewritten.push_str("gem ");
    rewritten.push(quote);
    rewritten.push_str(&name);
    rewritten.push(quote);
    rewritten.push_str(", ");
    rewritten.push(quote);
    rewritten.push_str(pinned);
    rewritten.push(quote);
    rewritten.push_str(&remainder);
    if let Some(comment) = comment {
        rewritten.push(' ');
        rewritten.push_str(comment.trim_start());
    }

    (rewritten, true)
}

fn comment_suffix(line: &str) -> Option<&str> {
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
            return Some(&line[i..]);
        }
        i += 1;
    }
    None
}

fn strip_ruby_comment(line: &str) -> &str {
    match comment_suffix(line) {
        Some(suffix) => {
            let idx = line.len() - suffix.len();
            &line[..idx]
        }
        None => line,
    }
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

#[cfg(test)]
mod tests {
    use super::rewrite_gem_line;
    use std::collections::HashMap;

    #[test]
    fn inserts_version_when_missing() {
        let pins = HashMap::from([("rake", "13.2.1")]);
        let (out, changed) = rewrite_gem_line(r#"gem "rake""#, &pins);
        assert!(changed);
        assert_eq!(out, r#"gem "rake", "13.2.1""#);
    }

    #[test]
    fn replaces_floating_constraint() {
        let pins = HashMap::from([("rspec", "3.13.0")]);
        let (out, changed) = rewrite_gem_line(r#"gem "rspec", ">= 3.0""#, &pins);
        assert!(changed);
        assert_eq!(out, r#"gem "rspec", "3.13.0""#);
    }

    #[test]
    fn preserves_kwargs_and_comment() {
        let pins = HashMap::from([("foo", "1.2.3")]);
        let (out, changed) = rewrite_gem_line(r#"gem "foo", require: false # keep"#, &pins);
        assert!(changed);
        assert_eq!(out, r#"gem "foo", "1.2.3", require: false # keep"#);
    }

    #[test]
    fn idempotent_when_already_exact() {
        let pins = HashMap::from([("rake", "13.2.1")]);
        let (out, changed) = rewrite_gem_line(r#"gem "rake", "13.2.1""#, &pins);
        assert!(!changed);
        assert_eq!(out, r#"gem "rake", "13.2.1""#);
    }
}
