use std::collections::HashMap;
use std::path::Path;

use pinner_ecosystem::{EcosystemError, Manifest, Pin, Rewrite};
use serde_yaml::Value;

use crate::extract::{include_ref, is_include_finding};

pub(crate) fn rewrite(
    manifest: &Manifest,
    pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    if pins.is_empty() {
        return Ok(None);
    }

    let image_pins: Vec<&Pin> = pins
        .iter()
        .filter(|p| pin_kind(p) != Some("include"))
        .collect();
    let include_pins: Vec<&Pin> = pins
        .iter()
        .filter(|p| pin_kind(p) == Some("include") || looks_like_include_pin(p))
        .collect();

    // Prefer line-aware image replace (docker compose style). When include pins are
    // present, also apply a targeted YAML walk for `ref:` under matching projects.
    let contents = std::fs::read_to_string(&manifest.path)?;
    let mut new_contents = if image_pins.is_empty() {
        contents.clone()
    } else {
        rewrite_images_line_aware(&contents, &image_pins)
    };

    if !include_pins.is_empty() {
        new_contents = rewrite_include_refs(&manifest.path, &new_contents, &include_pins)?;
    }

    if new_contents == contents {
        return Ok(None);
    }

    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents,
    }))
}

fn pin_kind(pin: &Pin) -> Option<&str> {
    pin.metadata.get("kind").and_then(|v| v.as_str())
}

fn looks_like_include_pin(pin: &Pin) -> bool {
    // Fallback when metadata missing: requested is project@ref without digest.
    is_include_finding(&pinner_ecosystem::Finding {
        ecosystem: pin.ecosystem,
        name: pin.name.clone(),
        requested: pin.requested.clone(),
        path: pin.path.clone(),
        is_floating: true,
    })
}

fn rewrite_images_line_aware(contents: &str, pins: &[&Pin]) -> String {
    let by_requested: HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.requested.as_str(), p.pinned.as_str()))
        .collect();

    let mut out = String::new();
    let mut changed = false;

    for line in contents.lines() {
        if let Some((prefix, value)) = split_image_line(line)
            && let Some(pinned) = by_requested.get(value.as_str())
        {
            out.push_str(&prefix);
            out.push_str(pinned);
            out.push('\n');
            changed = true;
        } else if let Some((prefix, value)) = split_image_name_line(line)
            && let Some(pinned) = by_requested.get(value.as_str())
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
        return contents.to_string();
    }
    if !contents.is_empty() && !contents.ends_with('\n') {
        out.pop();
    }
    out
}

/// `    image: node:latest` → (`    image: `, `node:latest`)
fn split_image_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("image:")?.trim_start();
    if rest.is_empty() {
        return None;
    }
    // Skip mapping form `image:` with no inline value.
    if rest.starts_with('#') {
        return None;
    }
    let value = unquote(rest);
    let prefix = format!("{}image: ", &line[..indent_len]);
    Some((prefix, value))
}

/// `      name: node:latest` under an image mapping.
fn split_image_name_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("name:")?.trim_start();
    if rest.is_empty() {
        return None;
    }
    let value = unquote(rest);
    // Only treat as image name when value looks like an image ref (has tag/digest or no spaces).
    if value.contains(' ') {
        return None;
    }
    let prefix = format!("{}name: ", &line[..indent_len]);
    Some((prefix, value))
}

fn rewrite_include_refs(
    path: &Path,
    contents: &str,
    pins: &[&Pin],
) -> Result<String, EcosystemError> {
    // Line-aware: track current `project:` in an include list item, rewrite matching `ref:`.
    let pin_by_project_ref: HashMap<(&str, &str), &str> = pins
        .iter()
        .filter_map(|p| {
            let ref_ = include_ref(&p.requested)?;
            Some(((p.name.as_str(), ref_), p.pinned.as_str()))
        })
        .collect();

    if pin_by_project_ref.is_empty() {
        return Ok(contents.to_string());
    }

    let mut out = String::new();
    let mut changed = false;
    let mut current_project: Option<String> = None;

    for line in contents.lines() {
        if let Some(project) = parse_project_line(line) {
            current_project = Some(project);
            out.push_str(line);
            out.push('\n');
            continue;
        }

        if let Some((prefix, ref_value)) = split_ref_line(line)
            && let Some(project) = current_project.as_deref()
            && let Some(pinned) = pin_by_project_ref.get(&(project, ref_value.as_str()))
        {
            out.push_str(&prefix);
            out.push_str(pinned);
            out.push('\n');
            changed = true;
            continue;
        }

        // Reset project when a new list item starts without project yet handled.
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") && !trimmed.contains("project:") {
            current_project = None;
        }

        out.push_str(line);
        out.push('\n');
    }

    if !changed {
        // Fallback: serde_yaml mutate for oddly formatted includes.
        return rewrite_include_refs_yaml(path, contents, pins);
    }
    if !contents.is_empty() && !contents.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

fn rewrite_include_refs_yaml(
    path: &Path,
    contents: &str,
    pins: &[&Pin],
) -> Result<String, EcosystemError> {
    let mut value: Value = serde_yaml::from_str(contents).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let by_requested: HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.requested.as_str(), p.pinned.as_str()))
        .collect();

    let mut changed = false;
    if let Some(include) = value.get_mut("include") {
        changed |= rewrite_include_value(include, &by_requested);
    }

    if !changed {
        return Ok(contents.to_string());
    }

    // Preserve image line-aware edits by only serializing when the input was already
    // parsed from the post-image-rewrite contents — callers pass that string.
    serde_yaml::to_string(&value).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

fn rewrite_include_value(include: &mut Value, by_requested: &HashMap<&str, &str>) -> bool {
    match include {
        Value::Sequence(items) => {
            let mut changed = false;
            for item in items {
                changed |= rewrite_include_item(item, by_requested);
            }
            changed
        }
        other => rewrite_include_item(other, by_requested),
    }
}

fn rewrite_include_item(item: &mut Value, by_requested: &HashMap<&str, &str>) -> bool {
    let Some(map) = item.as_mapping_mut() else {
        return false;
    };
    let Some(project) = map
        .get(Value::String("project".into()))
        .and_then(|v| v.as_str())
        .map(str::to_string)
    else {
        return false;
    };
    let ref_ = map
        .get(Value::String("ref".into()))
        .and_then(|v| v.as_str())
        .unwrap_or("main")
        .to_string();
    let requested = format!("{project}@{ref_}");
    let Some(pinned) = by_requested.get(requested.as_str()) else {
        return false;
    };
    map.insert(
        Value::String("ref".into()),
        Value::String((*pinned).to_string()),
    );
    true
}

fn parse_project_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = if let Some(after) = trimmed.strip_prefix('-') {
        after.trim_start()
    } else {
        trimmed
    };
    let rest = rest.strip_prefix("project:")?.trim_start();
    if rest.is_empty() {
        return None;
    }
    Some(unquote(rest))
}

fn split_ref_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("ref:")?.trim_start();
    if rest.is_empty() {
        return None;
    }
    let value = unquote(rest);
    let prefix = format!("{}ref: ", &line[..indent_len]);
    Some((prefix, value))
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
    use super::{rewrite_images_line_aware, split_image_line, split_ref_line};
    use pinner_ecosystem::{EcosystemKind, EvidenceKind, Pin};
    use serde_json::{Map, Value};
    use std::path::PathBuf;

    fn image_pin(requested: &str, pinned: &str) -> Pin {
        let mut metadata = Map::new();
        metadata.insert("kind".into(), Value::String("image".into()));
        Pin {
            ecosystem: EcosystemKind::Gitlab,
            name: "node".into(),
            requested: requested.into(),
            pinned: pinned.into(),
            path: PathBuf::from(".gitlab-ci.yml"),
            evidence: EvidenceKind::Registry,
            metadata,
        }
    }

    #[test]
    fn split_image_preserves_indent() {
        let (prefix, value) = split_image_line("image: node:latest").unwrap();
        assert_eq!(prefix, "image: ");
        assert_eq!(value, "node:latest");
    }

    #[test]
    fn split_ref_preserves_indent() {
        let (prefix, value) = split_ref_line("    ref: main").unwrap();
        assert_eq!(prefix, "    ref: ");
        assert_eq!(value, "main");
    }

    #[test]
    fn rewrite_image_line_aware() {
        let pins = [image_pin(
            "node:latest",
            "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )];
        let refs: Vec<&Pin> = pins.iter().collect();
        let out = rewrite_images_line_aware(
            "image: node:latest\ninclude:\n  - project: 'g/t'\n    ref: main\n",
            &refs,
        );
        assert!(out.contains(
            "image: node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        assert!(out.contains("ref: main"));
        assert!(!out.contains("node:latest"));
    }
}
