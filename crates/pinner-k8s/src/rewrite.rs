use std::collections::HashMap;
use std::path::Path;

use pinner_ecosystem::{EcosystemError, Manifest, Pin, Rewrite};
use serde::Deserialize;
use serde_yaml::Value;

use crate::discover::is_target_kind;

pub(crate) fn rewrite(
    manifest: &Manifest,
    pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    if pins.is_empty() {
        return Ok(None);
    }

    let new_contents = rewrite_yaml(&manifest.path, pins)?;
    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents,
    }))
}

fn pin_by_requested(pins: &[Pin]) -> HashMap<&str, &str> {
    pins.iter()
        .map(|p| (p.requested.as_str(), p.pinned.as_str()))
        .collect()
}

fn rewrite_yaml(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let by_requested = pin_by_requested(pins);
    let mut docs = Vec::new();

    for doc in serde_yaml::Deserializer::from_str(&contents) {
        let mut value = Value::deserialize(doc).map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        rewrite_doc(&mut value, &by_requested);
        docs.push(value);
    }

    if docs.len() == 1 {
        return serde_yaml::to_string(&docs[0]).map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        });
    }

    let mut out = String::new();
    for (i, doc) in docs.iter().enumerate() {
        if i > 0 {
            out.push_str("---\n");
        }
        let rendered = serde_yaml::to_string(doc).map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        out.push_str(&rendered);
    }
    Ok(out)
}

fn rewrite_doc(value: &mut Value, by_requested: &HashMap<&str, &str>) {
    let Some(kind) = value
        .get("kind")
        .and_then(|k| k.as_str())
        .map(str::to_string)
    else {
        return;
    };
    if !is_target_kind(&kind) {
        return;
    }
    let Some(pod_spec) = pod_spec_mut(value, &kind) else {
        return;
    };
    rewrite_container_list(pod_spec, "initContainers", by_requested);
    rewrite_container_list(pod_spec, "containers", by_requested);
}

fn pod_spec_mut<'a>(value: &'a mut Value, kind: &str) -> Option<&'a mut Value> {
    let spec = value.get_mut("spec")?;
    if kind == "CronJob" {
        spec.get_mut("jobTemplate")?
            .get_mut("spec")?
            .get_mut("template")?
            .get_mut("spec")
    } else {
        spec.get_mut("template")?.get_mut("spec")
    }
}

fn rewrite_container_list(pod_spec: &mut Value, field: &str, by_requested: &HashMap<&str, &str>) {
    let Some(containers) = pod_spec.get_mut(field).and_then(|c| c.as_sequence_mut()) else {
        return;
    };
    for container in containers {
        let Some(image) = container.get("image").and_then(|i| i.as_str()) else {
            continue;
        };
        let image = image.trim();
        let Some(pinned) = by_requested.get(image) else {
            continue;
        };
        if let Some(mapping) = container.as_mapping_mut() {
            mapping.insert(
                Value::String("image".into()),
                Value::String((*pinned).to_string()),
            );
        }
    }
}
