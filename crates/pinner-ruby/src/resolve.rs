use pinner_ecosystem::{EcosystemCtx, EcosystemError, Finding, Pin};

use crate::RubyEcosystem;

impl RubyEcosystem {
    pub(crate) fn resolve_findings(
        &self,
        _findings: &[Finding],
        _ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError> {
        Ok(Vec::new())
    }
}
