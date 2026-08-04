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
    let mut in_require_block = false;

    for raw in contents.lines() {
        let code = strip_go_comment(raw);
        let trimmed = code.trim();

        if let Some(rest) = trimmed.strip_prefix("require") {
            let rest = rest.trim();
            if rest == "(" {
                in_require_block = true;
                out.push_str(raw);
                out.push('\n');
                continue;
            }
            if rest == "()" {
                out.push_str(raw);
                out.push('\n');
                continue;
            }
            if !rest.is_empty() {
                let (rewritten, did) = rewrite_require_line(raw, &pin_by_name);
                changed |= did;
                out.push_str(&rewritten);
                out.push('\n');
                continue;
            }
        }

        if in_require_block {
            if trimmed == ")" {
                in_require_block = false;
                out.push_str(raw);
                out.push('\n');
                continue;
            }
            if !trimmed.is_empty() {
                let (rewritten, did) = rewrite_require_line(raw, &pin_by_name);
                changed |= did;
                out.push_str(&rewritten);
                out.push('\n');
                continue;
            }
        }

        out.push_str(raw);
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

/// Rewrite the version token on a require line while preserving indentation
/// and trailing `//` comments.
fn rewrite_require_line(raw: &str, pin_by_name: &HashMap<&str, &str>) -> (String, bool) {
    let (code, comment) = split_comment(raw);
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return (raw.to_string(), false);
    }

    // Single-line: `require path version`
    let (prefix, body) = if let Some(rest) = trimmed.strip_prefix("require") {
        let rest = rest.trim_start();
        if rest.is_empty() {
            return (raw.to_string(), false);
        }
        let indent_end = code.len() - code.trim_start().len();
        let require_idx = code[indent_end..].find("require").unwrap_or(0) + indent_end;
        let after_require = require_idx + "require".len();
        let body_start = after_require
            + code[after_require..]
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(|c| c.len_utf8())
                .sum::<usize>();
        (&code[..body_start], code[body_start..].trim_end())
    } else {
        // Block entry: `path version` (preserve leading whitespace from `code`)
        let indent_end = code.len() - code.trim_start().len();
        (&code[..indent_end], trimmed)
    };

    let mut parts = body.split_whitespace();
    let Some(name) = parts.next() else {
        return (raw.to_string(), false);
    };
    let old_version = parts.next();
    let Some(pinned) = pin_by_name.get(name) else {
        return (raw.to_string(), false);
    };
    if old_version == Some(*pinned) {
        return (raw.to_string(), false);
    }

    let mut rewritten = String::new();
    rewritten.push_str(prefix);
    rewritten.push_str(name);
    rewritten.push(' ');
    rewritten.push_str(pinned);
    if let Some(comment) = comment {
        rewritten.push(' ');
        rewritten.push_str(comment.trim_start());
    }
    (rewritten, true)
}

fn split_comment(line: &str) -> (&str, Option<&str>) {
    match line.find("//") {
        Some(idx) => (&line[..idx], Some(&line[idx..])),
        None => (line, None),
    }
}

fn strip_go_comment(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::rewrite_require_line;
    use std::collections::HashMap;

    #[test]
    fn rewrites_single_line_require() {
        let pins = HashMap::from([("github.com/example/lib", "v1.2.3")]);
        let (out, changed) = rewrite_require_line("require github.com/example/lib latest", &pins);
        assert!(changed);
        assert_eq!(out, "require github.com/example/lib v1.2.3");
    }

    #[test]
    fn rewrites_block_entry_preserving_indent() {
        let pins = HashMap::from([("github.com/example/lib", "v1.2.3")]);
        let (out, changed) = rewrite_require_line("\tgithub.com/example/lib latest", &pins);
        assert!(changed);
        assert_eq!(out, "\tgithub.com/example/lib v1.2.3");
    }

    #[test]
    fn preserves_trailing_comment() {
        let pins = HashMap::from([("github.com/example/lib", "v1.2.3")]);
        let (out, changed) =
            rewrite_require_line("require github.com/example/lib latest // pin me", &pins);
        assert!(changed);
        assert_eq!(out, "require github.com/example/lib v1.2.3 // pin me");
    }
}
