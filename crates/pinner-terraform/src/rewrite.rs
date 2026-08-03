use std::collections::HashMap;
use std::str::FromStr;

use hcl_edit::expr::{Expression, Object, ObjectKey};
use hcl_edit::structure::{Attribute, Body};
use hcl_edit::Ident;
use pinner_ecosystem::{EcosystemError, Manifest, Pin, Rewrite};

pub(crate) fn rewrite(
    manifest: &Manifest,
    pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    if pins.is_empty() {
        return Ok(None);
    }

    let new_contents = rewrite_file(&manifest.path, pins)?;
    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents,
    }))
}

fn rewrite_file(path: &std::path::Path, pins: &[Pin]) -> Result<String, EcosystemError> {
    let contents = std::fs::read_to_string(path)?;
    let mut body = Body::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let by_name: HashMap<&str, &Pin> = pins.iter().map(|p| (p.name.as_str(), p)).collect();

    for block in body.get_blocks_mut("module") {
        let Some(label) = block.labels.first().map(|l| l.as_str().to_string()) else {
            continue;
        };
        let Some(pin) = by_name.get(label.as_str()) else {
            continue;
        };

        let source = attr_str(&block.body, "source");
        if source.as_deref().is_some_and(is_git_or_http_source)
            || is_git_or_http_requested(&pin.requested)
        {
            if let Some(mut attr) = block.body.get_attribute_mut("source") {
                let current = attr.value.as_str().unwrap_or("").to_string();
                let updated = rewrite_git_ref(&current, &pin.pinned);
                *attr.value_mut() = Expression::from(updated);
            }
        } else if let Some(mut attr) = block.body.get_attribute_mut("version") {
            *attr.value_mut() = Expression::from(pin.pinned.clone());
        } else {
            block
                .body
                .push(Attribute::new(Ident::new("version"), pin.pinned.as_str()));
        }
    }

    for tf in body.get_blocks_mut("terraform") {
        for providers in tf.body.get_blocks_mut("required_providers") {
            // Collect keys first to avoid borrow issues while mutating.
            let keys: Vec<String> = providers
                .body
                .attributes()
                .map(|attr| attr.key.as_str().to_string())
                .collect();
            for key in keys {
                let Some(mut attr) = providers.body.get_attribute_mut(&key) else {
                    continue;
                };
                let Some(obj) = attr.value_mut().as_object_mut() else {
                    continue;
                };
                let source = object_str(obj, "source").unwrap_or_else(|| key.clone());
                let Some(pin) = by_name.get(source.as_str()).or_else(|| by_name.get(key.as_str()))
                else {
                    continue;
                };
                set_object_str(obj, "version", &pin.pinned);
            }
        }
    }

    Ok(body.to_string())
}

fn set_object_str(obj: &mut Object, key: &str, value: &str) {
    let object_key = ObjectKey::from(Ident::new(key));
    if let Some(entry) = obj.get_mut(&object_key) {
        *entry.expr_mut() = Expression::from(value);
    } else {
        obj.insert(object_key, Expression::from(value));
    }
}

fn rewrite_git_ref(source: &str, sha: &str) -> String {
    if let Some((base, query)) = source.split_once('?') {
        let mut parts: Vec<String> = query
            .split('&')
            .map(|part| {
                if let Some(rest) = part.strip_prefix("ref=") {
                    let _ = rest;
                    format!("ref={sha}")
                } else {
                    part.to_string()
                }
            })
            .collect();
        if !parts.iter().any(|p| p.starts_with("ref=")) {
            parts.push(format!("ref={sha}"));
        }
        format!("{base}?{}", parts.join("&"))
    } else if source.contains('?') {
        source.to_string()
    } else {
        format!("{source}?ref={sha}")
    }
}

fn is_git_or_http_source(source: &str) -> bool {
    is_git_or_http_requested(source)
}

fn is_git_or_http_requested(requested: &str) -> bool {
    let s = requested.trim();
    s.starts_with("git::")
        || s.starts_with("git@")
        || s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("github.com/")
        || s.starts_with("bitbucket.org/")
        || s.starts_with("gitlab.com/")
}

fn attr_str(body: &Body, key: &str) -> Option<String> {
    body.get_attribute(key)
        .and_then(|attr| attr.value.as_str().map(str::to_string))
}

fn object_str(obj: &Object, key: &str) -> Option<String> {
    obj.iter()
        .find(|(k, _)| object_key_eq(k, key))
        .and_then(|(_, v)| v.expr().as_str().map(str::to_string))
}

fn object_key_eq(key: &ObjectKey, expected: &str) -> bool {
    match key {
        ObjectKey::Ident(ident) => ident.as_str() == expected,
        ObjectKey::Expression(Expression::String(s)) => s.as_str() == expected,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::rewrite_git_ref;

    #[test]
    fn rewrite_git_ref_replaces_existing() {
        assert_eq!(
            rewrite_git_ref(
                "git::https://example.com/org/mod.git?ref=main",
                "11bd71901bbe5b1630ceea73d27597364c9af683"
            ),
            "git::https://example.com/org/mod.git?ref=11bd71901bbe5b1630ceea73d27597364c9af683"
        );
    }
}
