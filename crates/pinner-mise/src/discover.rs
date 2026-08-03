use std::path::Path;

use pinner_ecosystem::{EcosystemError, EcosystemKind, Manifest};

const MANIFEST_NAMES: &[&str] = &[".mise.toml", ".tool-versions"];

pub(crate) fn discover(repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
    let mut manifests = Vec::new();
    for name in MANIFEST_NAMES {
        let path = repo.join(name);
        if path.is_file() {
            manifests.push(Manifest {
                ecosystem: EcosystemKind::Mise,
                path,
            });
        }
    }
    Ok(manifests)
}
