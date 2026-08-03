use pinner_ecosystem::{EcosystemError, Manifest, Pin, Rewrite};

pub(crate) fn rewrite(
    _manifest: &Manifest,
    _pins: &[Pin],
) -> Result<Option<Rewrite>, EcosystemError> {
    Ok(None)
}
