use pinner_go::resolve_proxy_golang_latest;

#[test]
fn network_proxy_golang_optional() {
    if std::env::var("PINNER_NETWORK").ok().as_deref() != Some("1") {
        eprintln!("skip network_proxy_golang_optional (set PINNER_NETWORK=1)");
        return;
    }
    let pinned = resolve_proxy_golang_latest("golang.org/x/sync", &|url| {
        let output = std::process::Command::new("curl")
            .args(["-fsSL", "--max-time", "60", url])
            .output()
            .expect("curl");
        assert!(output.status.success(), "curl failed for {url}");
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    })
    .expect("network proxy.golang.org resolve");
    assert!(
        pinned.starts_with('v'),
        "expected module version, got {pinned}"
    );
}
