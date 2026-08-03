use std::path::Path;

use pinner_ecosystem::{EcosystemError, Manifest, Pin, Rewrite};
use serde::Deserialize;
use serde_yaml::Value;

pub(crate) fn rewrite(
    manifest: &Manifest,
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

    // Never rewrite values.yaml (image pins belong to k8s).
    if matches!(file_name, "values.yaml" | "values.yml") {
        return Ok(None);
    }

    let new_contents = if matches!(file_name, "Chart.yaml" | "Chart.yml") {
        rewrite_chart_yaml(&manifest.path, pins)?
    } else {
        rewrite_gitops_yaml(&manifest.path, pins)?
    };

    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents,
    }))
}

fn pin_for<'a>(pins: &'a [Pin], name: &str, repository: &str) -> Option<&'a Pin> {
    pins.iter().find(|p| {
        if p.name != name {
            return false;
        }
        let pin_repo = p
            .metadata
            .get("repository")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        pin_repo == repository
    })
}

fn rewrite_chart_yaml(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let mut value: Value = serde_yaml::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    if let Some(deps) = value
        .get_mut("dependencies")
        .and_then(|d| d.as_sequence_mut())
    {
        for dep in deps {
            let Some(name) = dep.get("name").and_then(|n| n.as_str()).map(str::to_string) else {
                continue;
            };
            let repository = dep
                .get("repository")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            let Some(pin) = pin_for(pins, &name, &repository) else {
                continue;
            };
            if let Some(mapping) = dep.as_mapping_mut() {
                mapping.insert(
                    Value::String("version".into()),
                    Value::String(pin.pinned.clone()),
                );
            }
        }
    }

    serde_yaml::to_string(&value).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

fn rewrite_gitops_yaml(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let mut docs = Vec::new();

    for doc in serde_yaml::Deserializer::from_str(&contents) {
        let mut value = Value::deserialize(doc).map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        rewrite_gitops_doc(&mut value, pins);
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

fn rewrite_gitops_doc(value: &mut Value, pins: &[Pin]) {
    let Some(kind) = value
        .get("kind")
        .and_then(|k| k.as_str())
        .map(str::to_string)
    else {
        return;
    };
    match kind.as_str() {
        "HelmRelease" => {
            let Some(chart_spec) = value
                .get_mut("spec")
                .and_then(|s| s.get_mut("chart"))
                .and_then(|c| c.get_mut("spec"))
            else {
                return;
            };
            let Some(name) = chart_spec
                .get("chart")
                .and_then(|c| c.as_str())
                .map(str::to_string)
            else {
                return;
            };
            let repository = chart_spec
                .get("sourceRef")
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let Some(pin) = pin_for(pins, &name, &repository) else {
                return;
            };
            if let Some(mapping) = chart_spec.as_mapping_mut() {
                mapping.insert(
                    Value::String("version".into()),
                    Value::String(pin.pinned.clone()),
                );
            }
        }
        "Application" => {
            let Some(source) = value.get_mut("spec").and_then(|s| s.get_mut("source")) else {
                return;
            };
            let Some(name) = source
                .get("chart")
                .and_then(|c| c.as_str())
                .map(str::to_string)
            else {
                return;
            };
            let repository = source
                .get("repoURL")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            let Some(pin) = pin_for(pins, &name, &repository) else {
                return;
            };
            if let Some(mapping) = source.as_mapping_mut() {
                mapping.insert(
                    Value::String("targetRevision".into()),
                    Value::String(pin.pinned.clone()),
                );
            }
        }
        _ => {}
    }
}
