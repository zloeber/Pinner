use std::collections::HashMap;
use std::path::Path;

use pinner_ecosystem::{EcosystemError, Pin, Rewrite};

use crate::extract::{is_image_finding, parse_uses_value};

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

fn pin_map_by_requested(pins: &[Pin]) -> HashMap<&str, &Pin> {
    pins.iter().map(|p| (p.requested.as_str(), p)).collect()
}

fn rewrite_file(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let pins_by_requested = pin_map_by_requested(pins);
    let mut out = String::new();
    let mut changed = false;

    for line in contents.lines() {
        if let Some(uses) = parse_uses_value(line)
            && let Some(pin) = pins_by_requested.get(uses.as_str())
            && !is_image_finding_pin(pin)
        {
            out.push_str(&replace_uses_line(line, &uses, pin));
            out.push('\n');
            changed = true;
        } else if let Some((prefix, value)) = split_container_line(line)
            && let Some(pin) = pins_by_requested.get(value.as_str())
        {
            out.push_str(&prefix);
            out.push_str(&pin.pinned);
            out.push('\n');
            changed = true;
        } else if let Some((prefix, value)) = split_image_line(line)
            && let Some(pin) = pins_by_requested.get(value.as_str())
        {
            out.push_str(&prefix);
            out.push_str(&pin.pinned);
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

fn is_image_finding_pin(pin: &Pin) -> bool {
    is_image_finding(&pinner_ecosystem::Finding {
        ecosystem: pin.ecosystem,
        name: pin.name.clone(),
        requested: pin.requested.clone(),
        path: pin.path.clone(),
        is_floating: true,
    })
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

/// Job-level `container: node:20` (string form only).
fn split_container_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("container:")?.trim_start();
    if rest.is_empty() || rest.starts_with('#') {
        return None;
    }
    let value = unquote(rest);
    if !looks_like_image_ref(&value) {
        return None;
    }
    let prefix = format!("{}container: ", &line[..indent_len]);
    Some((prefix, value))
}

/// `    image: redis:latest` → (`    image: `, `redis:latest`)
fn split_image_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("image:")?.trim_start();
    if rest.is_empty() || rest.starts_with('#') {
        return None;
    }
    let value = unquote(rest);
    let prefix = format!("{}image: ", &line[..indent_len]);
    Some((prefix, value))
}

fn looks_like_image_ref(value: &str) -> bool {
    value.contains(':') || value.contains('/') || value.contains('@')
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s.split('#').next().unwrap_or(s).trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{replace_uses_line, rewrite_file, split_container_line, split_image_line};
    use pinner_ecosystem::{EcosystemKind, EvidenceKind, Pin};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn checkout_pin() -> Pin {
        Pin {
            ecosystem: EcosystemKind::Actions,
            name: "actions/checkout".into(),
            requested: "actions/checkout@v4".into(),
            pinned: "11bd71901bbe5b1630ceea73d27597364c9af683".into(),
            path: PathBuf::from("ci.yml"),
            evidence: EvidenceKind::Registry,
            metadata: Default::default(),
        }
    }

    fn reusable_pin() -> Pin {
        Pin {
            ecosystem: EcosystemKind::Actions,
            name: "org/repo/.github/workflows/reuse.yml".into(),
            requested: "org/repo/.github/workflows/reuse.yml@v1".into(),
            pinned: "cccccccccccccccccccccccccccccccccccccccc".into(),
            path: PathBuf::from("ci.yml"),
            evidence: EvidenceKind::Registry,
            metadata: Default::default(),
        }
    }

    fn container_pin() -> Pin {
        Pin {
            ecosystem: EcosystemKind::Actions,
            name: "container:build".into(),
            requested: "node:20".into(),
            pinned: "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            path: PathBuf::from("ci.yml"),
            evidence: EvidenceKind::Registry,
            metadata: Default::default(),
        }
    }

    #[test]
    fn rewrites_with_sha_and_tag_comment() {
        let pin = checkout_pin();
        let line = "      - uses: actions/checkout@v4";
        let rewritten = replace_uses_line(line, "actions/checkout@v4", &pin);
        assert_eq!(
            rewritten,
            "      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4"
        );
    }

    #[test]
    fn rewrites_reusable_workflow_uses() {
        let pin = reusable_pin();
        let line = "      - uses: org/repo/.github/workflows/reuse.yml@v1";
        let rewritten =
            replace_uses_line(line, "org/repo/.github/workflows/reuse.yml@v1", &pin);
        assert_eq!(
            rewritten,
            "      - uses: org/repo/.github/workflows/reuse.yml@cccccccccccccccccccccccccccccccccccccccc # v1"
        );
    }

    #[test]
    fn split_container_and_image_lines() {
        let (prefix, value) = split_container_line("    container: node:20").unwrap();
        assert_eq!(prefix, "    container: ");
        assert_eq!(value, "node:20");
        let (prefix, value) = split_image_line("      image: redis:latest").unwrap();
        assert_eq!(prefix, "      image: ");
        assert_eq!(value, "redis:latest");
    }

    #[test]
    fn rewrites_nested_steps_and_preserves_surrounding_comments() {
        // Line-oriented rewrite: preserves indentation and unrelated comments.
        // Limitation: does not round-trip full YAML AST / flow-style nodes.
        let dir = tempdir().unwrap();
        let path = dir.path().join("ci.yml");
        fs::write(
            &path,
            "name: ci\njobs:\n  build:\n    steps:\n      # checkout first\n      - name: Checkout\n        uses: actions/checkout@v4\n      - run: echo hi\n  nest:\n    steps:\n      - uses: actions/checkout@v4\n",
        )
        .unwrap();
        let out = rewrite_file(&path, &[checkout_pin()]).unwrap();
        assert!(out.contains("# checkout first"));
        assert!(out.contains(
            "        uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4"
        ));
        assert!(out.contains(
            "      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4"
        ));
        assert!(out.contains("- run: echo hi"));
        assert!(!out.contains("actions/checkout@v4\n"));
    }

    #[test]
    fn rewrites_container_image_scalars() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ci.yml");
        fs::write(
            &path,
            "jobs:\n  build:\n    container: node:20\n    services:\n      redis:\n        image: redis:latest\n",
        )
        .unwrap();
        let out = rewrite_file(
            &path,
            &[
                container_pin(),
                Pin {
                    ecosystem: EcosystemKind::Actions,
                    name: "service:build/redis".into(),
                    requested: "redis:latest".into(),
                    pinned:
                        "redis@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                            .into(),
                    path: PathBuf::from("ci.yml"),
                    evidence: EvidenceKind::Registry,
                    metadata: Default::default(),
                },
            ],
        )
        .unwrap();
        assert!(out.contains(
            "container: node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        assert!(out.contains(
            "image: redis@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ));
        assert!(!out.contains("node:20"));
        assert!(!out.contains("redis:latest"));
    }
}
