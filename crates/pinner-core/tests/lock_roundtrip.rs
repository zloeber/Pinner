use pinner_core::lock::LockFile;
use pinner_ecosystem::{EcosystemKind, EvidenceKind, Pin};
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn lock_roundtrip_preserves_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pinner.lock.json");
    let pins = vec![Pin {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "lts".into(),
        pinned: "22.11.0".into(),
        path: PathBuf::from(".mise.toml"),
        evidence: EvidenceKind::Tool,
        metadata: Default::default(),
    }];
    let lock = LockFile::from_pins(&pins, "0.1.0", "2026-08-03T15:00:00Z");
    lock.write(&path).unwrap();
    let loaded = LockFile::read(&path).unwrap();
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.entries.len(), 1);
    assert_eq!(loaded.entries[0].pinned, "22.11.0");
}

#[test]
fn lock_rejects_unknown_version() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pinner.lock.json");
    std::fs::write(
        &path,
        r#"{"version":99,"generated_at":"","pinner_version":"","entries":[]}"#,
    )
    .unwrap();
    let err = LockFile::read(&path).unwrap_err();
    assert!(err.to_string().contains("version"));
}
