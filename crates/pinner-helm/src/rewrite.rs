use std::path::Path;

use pinner_ecosystem::{EcosystemError, Manifest, Pin, Rewrite};
use serde::Deserialize;
use serde_yaml::Value;

use crate::discover::is_values_file;

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

    let new_contents = if matches!(file_name, "Chart.yaml" | "Chart.yml") {
        rewrite_chart_yaml(&manifest.path, pins)?
    } else if is_values_file(&manifest.path) {
        rewrite_values_yaml(&manifest.path, pins)?
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

fn rewrite_values_yaml(path: &Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let by_requested: std::collections::HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.requested.as_str(), p.pinned.as_str()))
        .collect();
    let mut docs = Vec::new();

    for doc in serde_yaml::Deserializer::from_str(&contents) {
        let mut value = Value::deserialize(doc).map_err(|e| EcosystemError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        rewrite_values_images(&mut value, &by_requested);
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

fn rewrite_values_images(value: &mut Value, by_requested: &std::collections::HashMap<&str, &str>) {
    match value {
        Value::Mapping(map) => {
            if let Some(current) = composed_repo_tag_image(map)
                && let Some(pinned) = by_requested.get(current.as_str())
            {
                apply_repo_tag_pin(map, pinned);
            }
            // Collect keys first to allow mutation while iterating logically.
            let keys: Vec<Value> = map.keys().cloned().collect();
            for key in keys {
                let Some(child) = map.get_mut(key.clone()) else {
                    continue;
                };
                if let Some(key_str) = key.as_str()
                    && key_str.eq_ignore_ascii_case("image")
                    && let Some(s) = child.as_str()
                    && let Some(pinned) = by_requested.get(s)
                {
                    *child = Value::String((*pinned).to_string());
                    continue;
                }
                rewrite_values_images(child, by_requested);
            }
        }
        Value::Sequence(seq) => {
            for child in seq {
                rewrite_values_images(child, by_requested);
            }
        }
        _ => {}
    }
}

fn composed_repo_tag_image(map: &serde_yaml::Mapping) -> Option<String> {
    let repository = map
        .get(Value::String("repository".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let tag = map
        .get(Value::String("tag".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    let registry = map
        .get(Value::String("registry".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let repo = if let Some(registry) = registry {
        format!("{registry}/{repository}")
    } else {
        repository.to_string()
    };
    if tag.is_empty() {
        Some(repo)
    } else {
        Some(format!("{repo}:{tag}"))
    }
}

fn apply_repo_tag_pin(map: &mut serde_yaml::Mapping, pinned: &str) {
    let (repo, digest) = split_name_digest(pinned);
    // When registry is a separate key (Bitnami-style), keep repository as the
    // path-only segment so templates that join registry/repository do not double the host.
    let repository = match map
        .get(Value::String("registry".into()))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(registry) => {
            let prefix = format!("{registry}/");
            repo.strip_prefix(&prefix).unwrap_or(repo)
        }
        None => repo,
    };
    map.insert(
        Value::String("repository".into()),
        Value::String(repository.to_string()),
    );
    map.insert(
        Value::String("digest".into()),
        Value::String(digest.to_string()),
    );
    map.insert(Value::String("tag".into()), Value::String(String::new()));
}

fn split_name_digest(pinned: &str) -> (&str, &str) {
    if let Some((name, digest)) = pinned.split_once('@') {
        (name, digest)
    } else {
        (pinned, "")
    }
}
