use pinner_ecosystem::{EcosystemKind, Finding};
use pinner_ruby::resolve_rubygems_latest;
use std::path::PathBuf;

#[test]
fn network_rubygems_optional() {
    if std::env::var("PINNER_NETWORK").ok().as_deref() != Some("1") {
        eprintln!("skip network_rubygems_optional (set PINNER_NETWORK=1)");
        return;
    }
    let finding = Finding {
        ecosystem: EcosystemKind::Ruby,
        name: "rake".into(),
        requested: "13.2.1".into(),
        path: PathBuf::from("Gemfile"),
        is_floating: false,
    };
    let pinned = resolve_rubygems_latest(&finding, &|url| {
        let output = std::process::Command::new("curl")
            .args(["-fsSL", "--max-time", "60", url])
            .output()
            .expect("curl");
        assert!(output.status.success(), "curl failed for {url}");
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    })
    .expect("network rubygems resolve");
    assert!(
        !pinned.is_empty() && pinned.chars().next().unwrap().is_ascii_digit(),
        "expected gem version, got {pinned}"
    );
}
