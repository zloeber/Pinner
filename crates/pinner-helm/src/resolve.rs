use pinner_ecosystem::{EcosystemCtx, EcosystemError, Finding, Pin};

use crate::HelmEcosystem;

impl HelmEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        _findings: &[Finding],
        _ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        Ok(vec![])
    }
}
