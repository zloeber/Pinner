use pinner_cargo::resolve_crates_io_max_version;
use pinner_ecosystem::{EcosystemKind, Finding};
use std::path::PathBuf;

#[test]
fn network_crates_io_optional() {
    if std::env::var("PINNER_NETWORK").ok().as_deref() != Some("1") {
        eprintln!("skip network_crates_io_optional (set PINNER_NETWORK=1)");
        return;
    }
    let finding = Finding {
        ecosystem: EcosystemKind::Cargo,
        name: "serde".into(),
        requested: "1.0.0".into(),
        path: PathBuf::from("Cargo.toml"),
        is_floating: false,
    };
    let pinned = resolve_crates_io_max_version(&finding, &|url| {
        let output = std::process::Command::new("curl")
            .args(["-fsSL", "--max-time", "60", url])
            .output()
            .expect("curl");
        assert!(output.status.success(), "curl failed for {url}");
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    })
    .expect("network crates.io resolve");
    assert!(
        pinned.split('.').count() >= 3,
        "expected semver max_version, got {pinned}"
    );
}
