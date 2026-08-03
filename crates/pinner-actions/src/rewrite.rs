use std::collections::HashMap;
use std::path::Path;

use pinner_ecosystem::{EcosystemError, Pin, Rewrite};

use crate::extract::parse_uses_value;

pub(crate) fn rewrite(
    manifest: &pinner_ecosystem::Manifest,
    pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    if pins.is_empty() {
        return Ok(None);
    }

    let new_contents = rewrite_file(&manifest.path, pins)?;
    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents,
    }))
}

fn pin_map(pins: &[Pin]) -> HashMap<&str, &Pin> {
    pins.iter().map(|p| (p.requested.as_str(), p)).collect()
}

fn rewrite_file(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let pins = pin_map(pins);
    let mut out = String::new();
    let mut changed = false;

    for line in contents.lines() {
        if let Some(uses) = parse_uses_value(line)
            && let Some(pin) = pins.get(uses.as_str())
        {
            out.push_str(&replace_uses_line(line, &uses, pin));
            out.push('\n');
            changed = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    if !changed {
        return Ok(contents);
    }
    if !contents.is_empty() && !contents.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

/// Rewrite `uses: owner/action@ref` → `uses: owner/action@<sha> # ref`.
fn replace_uses_line(line: &str, uses: &str, pin: &Pin) -> String {
    let comment_ref = pin
        .requested
        .rsplit_once('@')
        .map(|(_, r)| r)
        .unwrap_or(pin.requested.as_str());
    let replacement = format!("{}@{} # {}", pin.name, pin.pinned, comment_ref);

    if let Some(idx) = line.find(uses) {
        let mut rewritten = String::with_capacity(line.len() - uses.len() + replacement.len());
        rewritten.push_str(&line[..idx]);
        rewritten.push_str(&replacement);
        // Drop any existing trailing comment after the uses token.
        let after = &line[idx + uses.len()..];
        let after_trimmed = after.trim_start();
        if after_trimmed.starts_with('#') {
            // omit old comment
        } else if !after.is_empty() {
            // Preserve trailing content that isn't a comment (unusual).
            rewritten.push_str(after);
        }
        rewritten
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::replace_uses_line;
    use pinner_ecosystem::{EcosystemKind, EvidenceKind, Pin};
    use std::path::PathBuf;

    #[test]
    fn rewrites_with_sha_and_tag_comment() {
        let pin = Pin {
            ecosystem: EcosystemKind::Actions,
            name: "actions/checkout".into(),
            requested: "actions/checkout@v4".into(),
            pinned: "11bd71901bbe5b1630ceea73d27597364c9af683".into(),
            path: PathBuf::from("ci.yml"),
            evidence: EvidenceKind::Registry,
            metadata: Default::default(),
        };
        let line = "      - uses: actions/checkout@v4";
        let rewritten = replace_uses_line(line, "actions/checkout@v4", &pin);
        assert_eq!(
            rewritten,
            "      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4"
        );
    }
}
