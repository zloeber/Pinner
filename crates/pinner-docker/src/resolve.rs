use std::collections::HashMap;
use std::env;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin,
};
use pinner_toolchain::{CommandRunner, RealCommandRunner};

use crate::DockerEcosystem;

impl DockerEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let runner = RealCommandRunner;
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            pins.push(resolve_one(&runner, finding, ctx, &map)?);
        }
        Ok(pins)
    }
}

fn resolve_one(
    runner: &dyn CommandRunner,
    finding: &Finding,
    ctx: &EcosystemCtx<'_>,
    map: &HashMap<String, String>,
) -> Result<Pin, EcosystemError> {
    if let Some(lock) = ctx.lock_pins.iter().find(|pin| {
        pin.ecosystem == EcosystemKind::Docker
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Docker,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    // PINNER_DOCKER_RESOLVE_MAP is checked before docker inspect/registry (test seam).
    if let Some(pinned) = map.get(&finding.requested) {
        return Ok(registry_pin(finding, pinned.clone()));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    if let Some(pinned) = resolve_via_docker_inspect(runner, &finding.requested) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Docker,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned,
            path: finding.path.clone(),
            evidence: EvidenceKind::Tool,
            metadata: Default::default(),
        });
    }

    let pinned = resolve_via_registry(runner, &finding.requested).map_err(|hint| {
        EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        }
    })?;
    Ok(registry_pin(finding, pinned))
}

fn registry_pin(finding: &Finding, pinned: String) -> Pin {
    Pin {
        ecosystem: EcosystemKind::Docker,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence: EvidenceKind::Registry,
        metadata: Default::default(),
    }
}

fn resolve_via_docker_inspect(runner: &dyn CommandRunner, image: &str) -> Option<String> {
    let output = runner
        .run(
            "docker",
            &[
                "image",
                "inspect",
                "--format",
                "{{index .RepoDigests 0}}",
                image,
            ],
        )
        .ok()?;
    if output.status != 0 {
        return None;
    }
    let digest = first_nonempty_line(&output.stdout);
    normalize_digest_ref(image, &digest)
}

fn resolve_via_registry(runner: &dyn CommandRunner, image: &str) -> Result<String, String> {
    let output = runner
        .run(
            "docker",
            &[
                "buildx",
                "imagetools",
                "inspect",
                "--format",
                "{{.Manifest.Digest}}",
                image,
            ],
        )
        .map_err(|err| format!("docker buildx imagetools inspect {image}: {err}"))?;
    if output.status != 0 {
        return Err(format!(
            "docker buildx imagetools inspect {image} failed (status {}): {}",
            output.status,
            output.stderr.trim()
        ));
    }
    let digest = first_nonempty_line(&output.stdout);
    normalize_digest_ref(image, &digest)
        .ok_or_else(|| format!("docker buildx imagetools inspect {image} returned no digest"))
}

/// Accept `repo@sha256:…` or bare `sha256:…` and return `name@sha256:…`.
fn normalize_digest_ref(requested: &str, digest_or_ref: &str) -> Option<String> {
    let value = digest_or_ref.trim();
    if value.is_empty() || value == "<no value>" {
        return None;
    }
    if value.contains("@sha256:") {
        return Some(value.to_string());
    }
    let digest = if let Some(rest) = value.strip_prefix("sha256:") {
        format!("sha256:{rest}")
    } else if value.chars().all(|c| c.is_ascii_hexdigit()) && value.len() == 64 {
        format!("sha256:{value}")
    } else {
        return None;
    };
    let name = crate::extract::image_name(requested);
    Some(format!("{name}@{digest}"))
}

fn first_nonempty_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Parse `PINNER_DOCKER_RESOLVE_MAP=python:3.12=python@sha256:aaa,alpine:latest=alpine@sha256:bbb`.
fn resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_DOCKER_RESOLVE_MAP") else {
        return HashMap::new();
    };
    parse_resolve_map(&raw)
}

fn parse_resolve_map(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::{normalize_digest_ref, parse_resolve_map};

    #[test]
    fn parse_resolve_map_entries() {
        let map = parse_resolve_map(
            "python:3.12=python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,alpine:latest=alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        assert_eq!(
            map.get("python:3.12").map(String::as_str),
            Some(
                "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            )
        );
        assert_eq!(
            map.get("alpine:latest").map(String::as_str),
            Some(
                "alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            )
        );
    }

    #[test]
    fn normalize_bare_and_full_digests() {
        assert_eq!(
            normalize_digest_ref(
                "python:3.12",
                "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            )
            .as_deref(),
            Some(
                "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            )
        );
        assert_eq!(
            normalize_digest_ref(
                "alpine:latest",
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            )
            .as_deref(),
            Some(
                "alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            )
        );
    }
}
