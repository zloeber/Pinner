use pinner_ecosystem::{EcosystemError, Pin, Rewrite};
use serde_json::Value;

const DEP_SECTIONS: &[&str] = &["dependencies", "devDependencies", "peerDependencies"];

pub(crate) fn rewrite(
    manifest: &pinner_ecosystem::Manifest,
    pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    if pins.is_empty() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&manifest.path)?;
    let mut value: Value = serde_json::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: manifest.path.clone(),
        message: e.to_string(),
    })?;

    let pin_by_name: std::collections::HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.name.as_str(), p.pinned.as_str()))
        .collect();

    let mut changed = false;
    for section in DEP_SECTIONS {
        let Some(deps) = value.get_mut(*section).and_then(|v| v.as_object_mut()) else {
            continue;
        };
        for (name, req) in deps.iter_mut() {
            if let Some(pinned) = pin_by_name.get(name.as_str()) {
                *req = Value::String((*pinned).to_string());
                changed = true;
            }
        }
    }

    if !changed {
        return Ok(None);
    }

    let new_contents = serde_json::to_string_pretty(&value).map_err(|e| EcosystemError::Parse {
        path: manifest.path.clone(),
        message: e.to_string(),
    })?;
    // serde_json pretty omits trailing newline; package.json usually has one.
    let new_contents = if contents.ends_with('\n') && !new_contents.ends_with('\n') {
        format!("{new_contents}\n")
    } else {
        new_contents
    };

    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents,
    }))
}
