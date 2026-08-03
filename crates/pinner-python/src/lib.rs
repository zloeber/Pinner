use std::path::Path;

use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemError, EcosystemKind, Finding, Manifest, Pin, Rewrite,
};

/// Temporary empty ecosystem stub until a later task fills python discovery/extract/resolve/rewrite.
pub struct PythonEcosystem;

impl Ecosystem for PythonEcosystem {
    fn kind(&self) -> EcosystemKind {
        EcosystemKind::Python
    }

    fn discover(&self, _repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
        Ok(Vec::new())
    }

    fn extract(
        &self,
        _manifest: &Manifest,
        _ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, EcosystemError> {
        Ok(Vec::new())
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
