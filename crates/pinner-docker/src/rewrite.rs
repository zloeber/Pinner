use std::collections::HashMap;
use std::path::Path;

use pinner_ecosystem::{EcosystemError, Pin, Rewrite};

use crate::extract::parse_from_image;

pub(crate) fn rewrite(
    manifest: &pinner_ecosystem::Manifest,
    pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    if pins.is_empty() {
        return Ok(None);
    }

    let file_name = manifest
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let new_contents = if is_dockerfile_name(file_name) {
        rewrite_dockerfile(&manifest.path, pins)?
    } else if is_compose_name(file_name) {
        rewrite_compose(&manifest.path, pins)?
    } else {
        return Ok(None);
    };

    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents,
    }))
}

fn is_dockerfile_name(name: &str) -> bool {
    name.starts_with("Dockerfile")
}

fn is_compose_name(name: &str) -> bool {
    matches!(
        name,
        "compose.yaml" | "compose.yml" | "docker-compose.yml" | "docker-compose.yaml"
    )
}

fn pin_map(pins: &[Pin]) -> HashMap<&str, &str> {
    pins.iter()
        .map(|p| (p.requested.as_str(), p.pinned.as_str()))
        .collect()
}

fn rewrite_dockerfile(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let pins = pin_map(pins);
    let mut out = String::new();
    let mut changed = false;

    for line in contents.lines() {
        if let Some(image) = parse_from_image(line)
            && let Some(pinned) = pins.get(image.as_str())
        {
            out.push_str(&replace_image_token(line, &image, pinned));
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

/// Replace the image token once, preserving whitespace, flags, and `AS stage`.
fn replace_image_token(line: &str, image: &str, pinned: &str) -> String {
    if let Some(idx) = line.find(image) {
        let mut rewritten = String::with_capacity(line.len() - image.len() + pinned.len());
        rewritten.push_str(&line[..idx]);
        rewritten.push_str(pinned);
        rewritten.push_str(&line[idx + image.len()..]);
        rewritten
    } else {
        line.to_string()
    }
}

fn rewrite_compose(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let pins = pin_map(pins);
    let mut out = String::new();
    let mut changed = false;

    for line in contents.lines() {
        if let Some((prefix, value)) = split_image_line(line)
            && let Some(pinned) = pins.get(value.as_str())
        {
            out.push_str(&prefix);
            out.push_str(pinned);
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

/// `    image: alpine:latest` → (`    image: `, `alpine:latest`)
fn split_image_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("image:")?.trim_start();
    if rest.is_empty() {
        return None;
    }
    let value = unquote(rest);
    let prefix = format!("{}image: ", &line[..indent_len]);
    Some((prefix, value))
}

fn unquote(s: &str) -> String {
    let s = s.trim();
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
    use super::replace_image_token;

    #[test]
    fn preserves_as_stage_alias() {
        let line = "FROM python:3.12 AS build";
        let rewritten = replace_image_token(
            line,
            "python:3.12",
            "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        assert_eq!(
            rewritten,
            "FROM python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa AS build"
        );
    }
}
