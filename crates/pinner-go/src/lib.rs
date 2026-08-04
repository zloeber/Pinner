mod discover;
mod extract;
mod resolve;
mod rewrite;

use std::path::Path;

use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest, Pin, Rewrite,
};

/// Go ecosystem: discover/extract/resolve/rewrite `go.mod` (+ lockfile evidence).
#[derive(Debug, Default, Clone, Copy)]
pub struct GoEcosystem;

impl Ecosystem for GoEcosystem {
    fn kind(&self) -> EcosystemKind {
        EcosystemKind::Go
    }

    fn discover(&self, repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
        discover::discover(repo)
    }

    fn extract(
        &self,
        manifest: &Manifest,
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, EcosystemError> {
        extract::extract(manifest, ctx)
    }

    fn resolve(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        self.resolve_findings(findings, ctx)
    }

    fn rewrite(
        &self,
        manifest: &Manifest,
        pins: &[Pin],
    ) -> Result<Option<Rewrite>, EcosystemError> {
        rewrite::rewrite(manifest, pins)
    }
}
