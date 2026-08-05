use serde_json::{Map, Value};

use crate::{EvidenceKind, Finding, Pin};

pub fn upgrade_pin(
    finding: &Finding,
    previous: &str,
    newest: &str,
    evidence: EvidenceKind,
    channel: &str,
) -> Option<Pin> {
    if previous == newest {
        return None;
    }
    let mut metadata = Map::new();
    metadata.insert("upgrade".into(), Value::Bool(true));
    metadata.insert("previous".into(), Value::String(previous.to_string()));
    metadata.insert(
        "upgrade_channel".into(),
        Value::String(channel.to_string()),
    );
    Some(Pin {
        ecosystem: finding.ecosystem,
        name: finding.name.clone(),
        requested: finding.requested.clone(),
        pinned: newest.to_string(),
        path: finding.path.clone(),
        evidence,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::EcosystemKind;

    #[test]
    fn upgrade_pin_omits_unchanged() {
        let f = Finding {
            ecosystem: EcosystemKind::Mise,
            name: "node".into(),
            requested: "1.0.0".into(),
            path: PathBuf::from(".mise.toml"),
            is_floating: false,
        };
        assert!(upgrade_pin(&f, "1.0.0", "1.0.0", EvidenceKind::Registry, "map").is_none());
        let p = upgrade_pin(&f, "1.0.0", "2.0.0", EvidenceKind::Registry, "map").unwrap();
        assert_eq!(p.pinned, "2.0.0");
        assert_eq!(p.metadata["previous"], "1.0.0");
        assert_eq!(p.metadata["upgrade"], true);
        assert_eq!(p.metadata["upgrade_channel"], "map");
    }
}
