use pinner_ecosystem::{EcosystemCtx, EcosystemError, Finding, Manifest};

pub(crate) fn extract(
    _manifest: &Manifest,
    _ctx: &EcosystemCtx<'_>,
) -> Result<Vec<Finding>, EcosystemError> {
    Ok(vec![])
}
