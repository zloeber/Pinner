use pinner_ecosystem::{EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest};
use serde_yaml::Value;

pub(crate) fn extract(
    manifest: &Manifest,
    _ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    let contents = std::fs::read_to_string(&manifest.path)?;
    let mut findings = Vec::new();

    for line in contents.lines() {
        let Some(uses) = parse_uses_value(line) else {
            continue;
        };
        let Some((name, ref_)) = split_owner_action_ref(&uses) else {
            continue;
        };
        let floating = is_floating_ref(ref_);
        // Floating refs keep owner/action@ref (resolve-map key). Pinned SHAs use the
        // bare SHA so check can match lock entries where requested == pinned == sha.
        let requested = if floating {
            format!("{name}@{ref_}")
        } else {
            ref_.to_string()
        };
        findings.push(Finding {
            ecosystem: EcosystemKind::Actions,
            name: name.to_string(),
            requested,
            path: manifest.path.clone(),
            is_floating: floating,
        });
    }

    findings.extend(extract_job_images(&manifest.path, &contents)?);
    Ok(findings)
}

fn extract_job_images(
    path: &std::path::Path,
    contents: &str,
) -> Result<Vec<Finding>, EcosystemError> {
    let value: Value = serde_yaml::from_str(contents).map_err(|e| EcosystemError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let Some(jobs) = value.get("jobs").and_then(|j| j.as_mapping()) else {
        return Ok(Vec::new());
    };

    let mut findings = Vec::new();
    for (job_key, job) in jobs {
        let Some(job_name) = job_key.as_str() else {
            continue;
        };
        let Some(job_map) = job.as_mapping() else {
            continue;
        };

        if let Some(container) = job_map.get(Value::String("container".into())) {
            push_container_finding(path, job_name, container, &mut findings);
        }

        if let Some(services) = job_map
            .get(Value::String("services".into()))
            .and_then(|s| s.as_mapping())
        {
            for (svc_key, svc) in services {
                let Some(svc_name) = svc_key.as_str() else {
                    continue;
                };
                let Some(image) = service_image(svc) else {
                    continue;
                };
                push_image_finding(
                    path,
                    &format!("service:{job_name}/{svc_name}"),
                    &image,
                    &mut findings,
                );
            }
        }
    }
    Ok(findings)
}

fn push_container_finding(
    path: &std::path::Path,
    job_name: &str,
    container: &Value,
    out: &mut Vec<Finding>,
) {
    let image = match container {
        Value::String(s) => Some(s.as_str()),
        Value::Mapping(map) => map
            .get(Value::String("image".into()))
            .and_then(|v| v.as_str()),
        _ => None,
    };
    let Some(image) = image.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    push_image_finding(path, &format!("container:{job_name}"), image, out);
}

fn service_image(svc: &Value) -> Option<String> {
    match svc {
        Value::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        }
        Value::Mapping(map) => map
            .get(Value::String("image".into()))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn push_image_finding(path: &std::path::Path, name: &str, image: &str, out: &mut Vec<Finding>) {
    out.push(Finding {
        ecosystem: EcosystemKind::Actions,
        name: name.to_string(),
        requested: image.to_string(),
        path: path.to_path_buf(),
        is_floating: is_floating_image(image),
    });
}

/// Image findings use synthetic names `container:<job>` / `service:<job>/<svc>`.
pub(crate) fn is_image_finding(finding: &Finding) -> bool {
    finding.name.starts_with("container:") || finding.name.starts_with("service:")
}

/// Floating if missing `@sha256:` digest.
pub(crate) fn is_floating_image(image: &str) -> bool {
    let image = image.trim();
    if image.is_empty() {
        return false;
    }
    !image.contains("@sha256:")
}

/// Parse `uses: owner/action@ref` (optional leading `- ` / indentation).
pub(crate) fn parse_uses_value(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = if let Some(after) = trimmed.strip_prefix('-') {
        after.trim_start()
    } else {
        trimmed
    };
    let rest = rest.strip_prefix("uses:")?.trim_start();
    if rest.is_empty() {
        return None;
    }
    Some(unquote(rest.split_whitespace().next().unwrap_or(rest)))
}

/// `owner/action@ref` → (`owner/action`, `ref`). Skips local (`./`) and `docker://` uses.
pub(crate) fn split_owner_action_ref(uses: &str) -> Option<(&str, &str)> {
    let uses = uses.trim();
    if uses.starts_with("./") || uses.starts_with(".\\") || uses.starts_with("docker://") {
        return None;
    }
    // owner/name@ref — require at least one '/' before '@'
    let at = uses.rfind('@')?;
    let name = &uses[..at];
    let ref_ = &uses[at + 1..];
    if name.is_empty() || ref_.is_empty() || !name.contains('/') {
        return None;
    }
    // Optional nested path: owner/repo/path@ref — still valid GitHub Action form.
    Some((name, ref_))
}

/// Non-full-hex SHA (not 40 or 64 hex) is floating.
pub(crate) fn is_floating_ref(ref_: &str) -> bool {
    !is_full_git_sha(ref_)
}

pub(crate) fn is_full_git_sha(ref_: &str) -> bool {
    let ref_ = ref_.trim();
    let len = ref_.len();
    (len == 40 || len == 64) && ref_.chars().all(|c| c.is_ascii_hexdigit())
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1].to_string()
    } else {
        // Strip trailing inline comment if present without space handling elsewhere.
        s.split('#').next().unwrap_or(s).trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{is_floating_image, is_floating_ref, parse_uses_value, split_owner_action_ref};

    #[test]
    fn parses_uses_lines() {
        assert_eq!(
            parse_uses_value("      - uses: actions/checkout@v4").as_deref(),
            Some("actions/checkout@v4")
        );
        assert_eq!(
            parse_uses_value("uses: \"actions/setup-node@v4\"").as_deref(),
            Some("actions/setup-node@v4")
        );
    }

    #[test]
    fn floating_vs_sha() {
        assert!(is_floating_ref("v4"));
        assert!(is_floating_ref("main"));
        assert!(is_floating_ref("11bd719")); // short sha
        assert!(!is_floating_ref("11bd71901bbe5b1630ceea73d27597364c9af683"));
        assert!(!is_floating_ref(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
    }

    #[test]
    fn splits_owner_action() {
        assert_eq!(
            split_owner_action_ref("actions/checkout@v4"),
            Some(("actions/checkout", "v4"))
        );
        assert_eq!(
            split_owner_action_ref("org/repo/.github/workflows/reuse.yml@v1"),
            Some(("org/repo/.github/workflows/reuse.yml", "v1"))
        );
        assert_eq!(split_owner_action_ref("./local-action"), None);
        assert_eq!(split_owner_action_ref("docker://alpine:3"), None);
    }

    #[test]
    fn floating_image_detection() {
        assert!(is_floating_image("node:20"));
        assert!(is_floating_image("redis:latest"));
        assert!(!is_floating_image(
            "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
    }
}
