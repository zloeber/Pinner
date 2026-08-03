mod discover;
mod extract;
mod resolve;
mod rewrite;

use std::path::Path;
use std::sync::Arc;

use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest, Pin, Rewrite,
};
use pinner_toolchain::{CommandRunner, RealCommandRunner};

/// Mise ecosystem: discover/extract/resolve/rewrite `.mise.toml` and `.tool-versions`.
pub struct MiseEcosystem {
    pub(crate) runner: Arc<dyn CommandRunner>,
}

impl Default for MiseEcosystem {
    fn default() -> Self {
        Self::new()
    }
}

impl MiseEcosystem {
    pub fn new() -> Self {
        Self {
            runner: Arc::new(RealCommandRunner),
        }
    }

    pub fn with_runner(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }
}

impl Ecosystem for MiseEcosystem {
    fn kind(&self) -> EcosystemKind {
        EcosystemKind::Mise
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
