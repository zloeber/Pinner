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

pub(crate) fn validate_rewrite(
    manifest: &pinner_ecosystem::Manifest,
    new_contents: &str,
) -> Result<(), EcosystemError> {
    let file_name = manifest
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    match file_name {
        ".mise.toml" => {
            new_contents
                .parse::<DocumentMut>()
                .map_err(|e| EcosystemError::Parse {
                    path: manifest.path.clone(),
                    message: format!("invalid rewritten .mise.toml: {e}"),
                })?;
            Ok(())
        }
        ".tool-versions" => {
            for (idx, line) in new_contents.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let mut parts = trimmed.split_whitespace();
                let name = parts.next();
                let version = parts.next();
                if name.is_none() || version.is_none() {
                    return Err(EcosystemError::Parse {
                        path: manifest.path.clone(),
                        message: format!(
                            "invalid rewritten .tool-versions at line {}: expected '<tool> <version>'",
                            idx + 1
                        ),
                    });
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
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
        set_tool_version(&mut doc, &pin.name, &pin.pinned);
    }

    Ok(doc.to_string())
}

/// Prefer updating `version` inside inline/full tables so backends like
/// `awscli = { version = "...", symlink_bins = "true" }` and
/// `[tools."http:gkg"]` keep their non-version keys.
fn set_tool_version(doc: &mut DocumentMut, name: &str, pinned: &str) {
    let Some(tools) = doc.get_mut("tools") else {
        doc["tools"][name] = value(pinned);
        return;
    };

    if let Some(item) = tools.get_mut(name) {
        if let Some(inline) = item.as_inline_table_mut() {
            inline["version"] = value(pinned).into_value().expect("string value");
            return;
        }
        if let Some(table) = item.as_table_mut() {
            table["version"] = value(pinned);
            return;
        }
    }

    tools[name] = value(pinned);
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
