use std::collections::HashMap;
use std::env;

use pinner_ecosystem::{
    EcosystemCtx, EcosystemError, EcosystemKind, EvidenceKind, Finding, Pin,
};
use pinner_toolchain::CommandRunner;

use crate::MiseEcosystem;

impl MiseEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        let map = resolve_map_from_env();
        let mut pins = Vec::with_capacity(findings.len());
        for finding in findings {
            pins.push(resolve_one(self.runner.as_ref(), finding, ctx, &map)?);
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
        pin.ecosystem == EcosystemKind::Mise
            && pin.name == finding.name
            && pin.requested == finding.requested
    }) {
        return Ok(Pin {
            ecosystem: EcosystemKind::Mise,
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            pinned: lock.pinned.clone(),
            path: finding.path.clone(),
            evidence: EvidenceKind::Lock,
            metadata: lock.metadata.clone(),
        });
    }

    // PINNER_MISE_RESOLVE_MAP is checked before invoking mise (Task 10 e2e seam).
    if let Some(pinned) = map.get(&finding.name) {
        return Ok(tool_pin(finding, pinned.clone()));
    }

    if ctx.offline {
        return Err(EcosystemError::Offline {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
        });
    }

    let pinned = resolve_via_mise(runner, &finding.name).map_err(|hint| {
        EcosystemError::Resolve {
            name: finding.name.clone(),
            requested: finding.requested.clone(),
            hint,
        }
    })?;
    Ok(tool_pin(finding, pinned))
}

fn tool_pin(finding: &Finding, pinned: String) -> Pin {
    Pin {
        ecosystem: EcosystemKind::Mise,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned,
        path: finding.path.clone(),
        evidence: EvidenceKind::Tool,
        metadata: Default::default(),
    }
}

fn resolve_via_mise(runner: &dyn CommandRunner, name: &str) -> Result<String, String> {
    match runner.run("mise", &["latest", name]) {
        Ok(output) if output.status == 0 => {
            let version = first_nonempty_line(&output.stdout);
            if !version.is_empty() {
                return Ok(version);
            }
        }
        Ok(output) => {
            // Fall through to ls-remote; keep latest stderr for final hint.
            let latest_err = output.stderr;
            return resolve_via_ls_remote(runner, name, Some(latest_err));
        }
        Err(err) => {
            return Err(format!("mise latest {name}: {err}"));
        }
    }

    resolve_via_ls_remote(runner, name, None)
}

fn resolve_via_ls_remote(
    runner: &dyn CommandRunner,
    name: &str,
    latest_err: Option<String>,
) -> Result<String, String> {
    let output = runner
        .run("mise", &["ls-remote", name])
        .map_err(|err| format!("mise ls-remote {name}: {err}"))?;
    if output.status != 0 {
        let mut hint = format!(
            "mise ls-remote {name} failed (status {}): {}",
            output.status,
            output.stderr.trim()
        );
        if let Some(prev) = latest_err {
            hint = format!("mise latest failed: {}; {hint}", prev.trim());
        }
        return Err(hint);
    }
    let version = last_nonempty_line(&output.stdout);
    if version.is_empty() {
        return Err(format!("mise ls-remote {name} returned no versions"));
    }
    Ok(version)
}

fn first_nonempty_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

fn last_nonempty_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Parse `PINNER_MISE_RESOLVE_MAP=node=22.11.0,python=3.12.7`.
fn resolve_map_from_env() -> HashMap<String, String> {
    let Ok(raw) = env::var("PINNER_MISE_RESOLVE_MAP") else {
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
        let Some((name, version)) = entry.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let version = version.trim();
        if !name.is_empty() && !version.is_empty() {
            map.insert(name.to_string(), version.to_string());
        }
    }
    map
}
