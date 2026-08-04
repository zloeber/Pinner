use std::collections::HashMap;

use pinner_ecosystem::{EcosystemError, Manifest, Pin, Rewrite};

use crate::extract::{is_task_finding, parse_task_ref};

pub(crate) fn rewrite(
    manifest: &Manifest,
    pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    if pins.is_empty() {
        return Ok(None);
    }

    let image_pins: Vec<&Pin> = pins
        .iter()
        .filter(|p| pin_kind(p) != Some("task"))
        .collect();
    let task_pins: Vec<&Pin> = pins
        .iter()
        .filter(|p| pin_kind(p) == Some("task") || looks_like_task_pin(p))
        .collect();

    let contents = std::fs::read_to_string(&manifest.path)?;
    let mut new_contents = if image_pins.is_empty() {
        contents.clone()
    } else {
        rewrite_images_line_aware(&contents, &image_pins)
    };

    if !task_pins.is_empty() {
        new_contents = rewrite_tasks_line_aware(&new_contents, &task_pins);
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

fn looks_like_task_pin(pin: &Pin) -> bool {
    is_task_finding(&pinner_ecosystem::Finding {
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
        } else if let Some((prefix, value)) = split_container_line(line)
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

fn rewrite_tasks_line_aware(contents: &str, pins: &[&Pin]) -> String {
    let by_requested: HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.requested.as_str(), p.pinned.as_str()))
        .collect();

    let mut out = String::new();
    let mut changed = false;

    for line in contents.lines() {
        if let Some((prefix, value)) = split_task_line(line)
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

/// `      image: node:latest` → (`      image: `, `node:latest`)
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

/// Job-level `container: node:latest` (string form only).
fn split_container_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("container:")?.trim_start();
    if rest.is_empty() || rest.starts_with('#') {
        return None;
    }
    // Skip mapping form / named container refs like `container: build`.
    let value = unquote(rest);
    if !looks_like_image_ref(&value) {
        return None;
    }
    let prefix = format!("{}container: ", &line[..indent_len]);
    Some((prefix, value))
}

fn looks_like_image_ref(value: &str) -> bool {
    value.contains(':') || value.contains('/') || value.contains('@')
}

/// `  - task: UseNode@1` or `    task: UseNode@1`
fn split_task_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();

    let (after_dash, dash_prefix) = if let Some(rest) = trimmed.strip_prefix('-') {
        let rest = rest.trim_start();
        let dash_ws = trimmed.len() - rest.len();
        (rest, format!("{}{}", &line[..indent_len], &trimmed[..dash_ws]))
    } else {
        (trimmed, line[..indent_len].to_string())
    };

    let rest = after_dash.strip_prefix("task:")?.trim_start();
    if rest.is_empty() {
        return None;
    }
    let value = unquote(rest);
    parse_task_ref(&value)?;
    let prefix = format!("{dash_prefix}task: ");
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
    use super::{rewrite_images_line_aware, rewrite_tasks_line_aware, split_task_line};
    use pinner_ecosystem::{EcosystemKind, EvidenceKind, Pin};
    use serde_json::{Map, Value};
    use std::path::PathBuf;

    fn image_pin(requested: &str, pinned: &str) -> Pin {
        let mut metadata = Map::new();
        metadata.insert("kind".into(), Value::String("image".into()));
        Pin {
            ecosystem: EcosystemKind::Azure,
            name: "node".into(),
            requested: requested.into(),
            pinned: pinned.into(),
            path: PathBuf::from("azure-pipelines.yml"),
            evidence: EvidenceKind::Registry,
            metadata,
        }
    }

    fn task_pin(requested: &str, pinned: &str) -> Pin {
        let mut metadata = Map::new();
        metadata.insert("kind".into(), Value::String("task".into()));
        Pin {
            ecosystem: EcosystemKind::Azure,
            name: "UseNode".into(),
            requested: requested.into(),
            pinned: pinned.into(),
            path: PathBuf::from("azure-pipelines.yml"),
            evidence: EvidenceKind::Registry,
            metadata,
        }
    }

    #[test]
    fn split_task_preserves_list_dash() {
        let (prefix, value) = split_task_line("  - task: UseNode@1").unwrap();
        assert_eq!(prefix, "  - task: ");
        assert_eq!(value, "UseNode@1");
    }

    #[test]
    fn rewrite_image_and_task_line_aware() {
        let image = image_pin(
            "node:latest",
            "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        let task = task_pin("UseNode@1", "UseNode@1.2.3");
        let img_refs: Vec<&Pin> = vec![&image];
        let task_refs: Vec<&Pin> = vec![&task];

        let yaml = "\
pool:
  vmImage: ubuntu-latest
resources:
  containers:
    - container: build
      image: node:latest
steps:
  - task: UseNode@1
";
        let after_image = rewrite_images_line_aware(yaml, &img_refs);
        let out = rewrite_tasks_line_aware(&after_image, &task_refs);
        assert!(out.contains(
            "image: node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        assert!(out.contains("task: UseNode@1.2.3"));
        assert!(!out.contains("node:latest"));
        assert!(!out.contains("UseNode@1\n") && !out.ends_with("UseNode@1"));
        assert!(out.contains("vmImage: ubuntu-latest"));
        assert!(out.contains("container: build"));
    }
}
