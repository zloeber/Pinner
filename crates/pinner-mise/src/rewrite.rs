use std::path::Path;

use pinner_ecosystem::{EcosystemError, Pin, Rewrite};
use toml_edit::{DocumentMut, value};

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

    let new_contents = match file_name {
        ".mise.toml" => rewrite_mise_toml(&manifest.path, pins)?,
        ".tool-versions" => rewrite_tool_versions(&manifest.path, pins)?,
        _ => return Ok(None),
    };

    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents,
    }))
}

fn rewrite_mise_toml(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let mut doc = contents
        .parse::<DocumentMut>()
        .map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

    for pin in pins {
        doc["tools"][&pin.name] = value(pin.pinned.as_str());
    }

    Ok(doc.to_string())
}

fn rewrite_tool_versions(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let pin_by_name: std::collections::HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.name.as_str(), p.pinned.as_str()))
        .collect();

    let mut out = String::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(name) = parts.next() else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        if let Some(pinned) = pin_by_name.get(name) {
            out.push_str(name);
            out.push(' ');
            out.push_str(pinned);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    if contents.is_empty() {
        return Ok(String::new());
    }
    if !contents.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}
