use std::path::Path;

use pinner_ecosystem::{EcosystemError, Manifest};

pub(crate) fn discover(_repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
    Ok(Vec::new())
}
