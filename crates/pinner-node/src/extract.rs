use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest};
use serde_json::Value;

const DEP_SECTIONS: &[&str] = &["dependencies", "devDependencies", "peerDependencies"];

pub(crate) fn extract(
    manifest: &Manifest,
    ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(&manifest.path)?;
    let value: Value = serde_json::from_str(&contents).map_err(|e| EcosystemError::Parse {
        path: manifest.path.clone(),
        message: e.to_string(),
    })?;

    let mut findings = Vec::new();
    for section in DEP_SECTIONS {
        let Some(deps) = value.get(*section).and_then(|v| v.as_object()) else {
            continue;
        };
        for (name, req) in deps {
            let Some(requested) = req.as_str() else {
                continue;
            };
            findings.push(Finding {
                ecosystem: EcosystemKind::Node,
                name: name.clone(),
                requested: requested.to_string(),
                path: manifest.path.clone(),
                is_floating: is_floating(requested, ctx.pin_exact_ranges),
            });
        }
    }
    Ok(findings)
}

fn is_floating(requested: &str, pin_exact_ranges: bool) -> bool {
    let requested = requested.trim();
    if requested == "latest" || requested == "*" {
        return true;
    }
    if pin_exact_ranges
        && (requested.starts_with('^') || requested.starts_with('~') || requested.starts_with(">="))
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::is_floating;

    #[test]
    fn floating_signals() {
        assert!(is_floating("latest", false));
        assert!(is_floating("*", false));
        assert!(!is_floating("^1.0.0", false));
        assert!(is_floating("^1.0.0", true));
        assert!(is_floating("~1.0.0", true));
        assert!(is_floating(">=1.0.0", true));
        assert!(!is_floating("1.2.3", true));
    }
}
