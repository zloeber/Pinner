use std::collections::HashMap;

use pinner_ecosystem::{EcosystemError, Pin, Rewrite};
use toml_edit::{DocumentMut, Item, Value};

const DEP_SECTIONS: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];

pub(crate) fn rewrite(
    manifest: &pinner_ecosystem::Manifest,
    pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    if pins.is_empty() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&manifest.path)?;
    let mut doc = contents
        .parse::<DocumentMut>()
        .map_err(|e| EcosystemError::Parse {
            path: manifest.path.clone(),
            message: e.to_string(),
        })?;

    let pin_by_name: HashMap<&str, &str> = pins
        .iter()
        .map(|p| (p.name.as_str(), p.pinned.as_str()))
        .collect();

    let mut changed = false;
    changed |= rewrite_dep_tables(doc.as_table_mut(), &pin_by_name);

    if let Some(targets) = doc.get_mut("target").and_then(|t| t.as_table_like_mut()) {
        let keys: Vec<String> = targets.iter().map(|(k, _)| k.to_string()).collect();
        for key in keys {
            if let Some(table) = targets.get_mut(&key).and_then(|i| i.as_table_like_mut()) {
                changed |= rewrite_dep_tables(table, &pin_by_name);
            }
        }
    }

    if !changed {
        return Ok(None);
    }

    Ok(Some(Rewrite {
        path: manifest.path.clone(),
        new_contents: doc.to_string(),
    }))
}

fn rewrite_dep_tables(
    table: &mut dyn toml_edit::TableLike,
    pin_by_name: &HashMap<&str, &str>,
) -> bool {
    let mut changed = false;
    for section in DEP_SECTIONS {
        let Some(deps) = table.get_mut(section).and_then(|i| i.as_table_like_mut()) else {
            continue;
        };
        let names: Vec<String> = deps.iter().map(|(k, _)| k.to_string()).collect();
        for name in names {
            let Some(pinned) = pin_by_name.get(name.as_str()) else {
                continue;
            };
            let Some(item) = deps.get_mut(&name) else {
                continue;
            };
            if apply_pin(item, pinned) {
                changed = true;
            }
        }
    }
    changed
}

fn apply_pin(item: &mut Item, pinned: &str) -> bool {
    match item {
        Item::Value(Value::String(s)) => {
            if s.value() == pinned {
                return false;
            }
            *item = Item::Value(Value::from(pinned));
            true
        }
        Item::Value(Value::InlineTable(table)) => {
            if table.get("path").is_some() || table.get("git").is_some() {
                return false;
            }
            match table.get("version") {
                Some(Value::String(s)) if s.value() == pinned => false,
                Some(_) | None => {
                    table.insert("version", Value::from(pinned));
                    true
                }
            }
        }
        Item::Table(table) => {
            if table.contains_key("path") || table.contains_key("git") {
                return false;
            }
            match table.get("version") {
                Some(Item::Value(Value::String(s))) if s.value() == pinned => false,
                _ => {
                    table.insert("version", Item::Value(Value::from(pinned)));
                    true
                }
            }
        }
        _ => false,
    }
}
