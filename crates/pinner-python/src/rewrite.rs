use std::collections::HashMap;
use std::path::Path;

use pinner_ecosystem::{EcosystemError, Pin, Rewrite};
use toml_edit::{DocumentMut, Item, Value};

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

    let new_contents = if file_name == "pyproject.toml" {
        rewrite_pyproject(&manifest.path, pins)?
    } else if is_requirements_file(file_name) {
        rewrite_requirements(&manifest.path, pins)?
    } else {
        return Ok(None);
    };

    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents,
    }))
}

fn is_requirements_file(name: &str) -> bool {
    name == "requirements.txt" || (name.starts_with("requirements") && name.ends_with(".txt"))
}

fn rewrite_requirements(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let pin_by_name: HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.name.as_str(), p.pinned.as_str()))
        .collect();

    let mut out = String::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        let code = if let Some(idx) = trimmed.find(" #") {
            &trimmed[..idx]
        } else {
            trimmed
        };
        let marker = code.find(';').map(|i| &code[i..]);
        let before_marker = marker
            .map(|m| &code[..code.len() - m.len()])
            .unwrap_or(code);
        let name = package_name(before_marker.trim());

        if let Some(name) = name
            && let Some(pinned) = pin_by_name.get(name.as_str())
        {
            out.push_str(&name);
            out.push_str("==");
            out.push_str(pinned);
            if let Some(m) = marker {
                out.push(' ');
                out.push_str(m.trim());
            }
            // Preserve trailing inline comment if present.
            if let Some(idx) = line.find(" #") {
                out.push_str(&line[idx..]);
            }
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

fn rewrite_pyproject(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let mut doc = contents
        .parse::<DocumentMut>()
        .map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

    let pin_by_name: HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.name.as_str(), p.pinned.as_str()))
        .collect();

    if let Some(deps) = doc
        .get_mut("project")
        .and_then(|p| p.get_mut("dependencies"))
        .and_then(|d| d.as_array_mut())
    {
        rewrite_dep_array(deps, &pin_by_name);
    }

    if let Some(optional) = doc
        .get_mut("project")
        .and_then(|p| p.get_mut("optional-dependencies"))
        .and_then(|t| t.as_table_like_mut())
    {
        let keys: Vec<String> = optional.iter().map(|(k, _)| k.to_string()).collect();
        for key in keys {
            if let Some(arr) = optional.get_mut(&key).and_then(|i| i.as_array_mut()) {
                rewrite_dep_array(arr, &pin_by_name);
            }
        }
    }

    // Poetry table of name = version (only rewrite existing keys).
    if doc
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
        .is_some()
    {
        for (name, pinned) in &pin_by_name {
            if *name == "python" {
                continue;
            }
            let exists = doc["tool"]["poetry"]["dependencies"].get(name).is_some();
            if exists {
                doc["tool"]["poetry"]["dependencies"][name] =
                    Item::Value(Value::from(format!("=={pinned}")));
            }
        }
    }

    Ok(doc.to_string())
}

fn rewrite_dep_array(arr: &mut toml_edit::Array, pin_by_name: &HashMap<&str, &str>) {
    for idx in 0..arr.len() {
        let Some(item) = arr.get(idx) else {
            continue;
        };
        let Some(s) = item.as_str() else {
            continue;
        };
        let Some(name) = package_name(s) else {
            continue;
        };
        let Some(pinned) = pin_by_name.get(name.as_str()) else {
            continue;
        };
        let marker = s.find(';').map(|i| s[i..].trim());
        let mut new_req = format!("{name}=={pinned}");
        if let Some(m) = marker {
            new_req.push(' ');
            new_req.push_str(m);
        }
        arr.replace(idx, new_req);
    }
}

fn package_name(req: &str) -> Option<String> {
    let req = req.trim();
    let req = req.split(';').next()?.trim();
    let bytes = req.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b'.' {
            i += 1;
        } else {
            break;
        }
    }
    if i == 0 {
        None
    } else {
        Some(req[..i].to_string())
    }
}
