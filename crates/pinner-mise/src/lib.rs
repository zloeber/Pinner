mod discover;
mod extract;

use std::path::Path;

use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest, Pin, Rewrite,
};

/// Mise ecosystem: discover/extract `.mise.toml` and `.tool-versions`.
pub struct MiseEcosystem;

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
        _findings: &[Finding],
        _ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        Ok(Vec::new())
    }

    fn rewrite(
        &self,
        _manifest: &Manifest,
        _pins: &[Pin],
    ) -> Result<Option<Rewrite>, EcosystemError> {
        Ok(None)
    }
}
