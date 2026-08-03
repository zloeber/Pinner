mod discover;
mod extract;
mod resolve;
mod rewrite;

use std::path::Path;

use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest, Pin, Rewrite,
};

/// Kubernetes ecosystem: discover/extract/resolve/rewrite container image refs in manifests.
#[derive(Debug, Default, Clone, Copy)]
pub struct K8sEcosystem;

impl Ecosystem for K8sEcosystem {
    fn kind(&self) -> EcosystemKind {
        EcosystemKind::K8s
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
