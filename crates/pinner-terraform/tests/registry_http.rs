use pinner_terraform::{resolve_terraform_registry_module, resolve_terraform_registry_provider};

#[test]
fn registry_module_picks_latest_matching_via_injected_http() {
    let body = r#"{
      "modules": [{
        "source": "terraform-aws-modules/vpc/aws",
        "versions": [
          {"version": "5.0.0"},
          {"version": "5.1.2"},
          {"version": "6.0.0"}
        ]
      }]
    }"#;
    let pinned =
        resolve_terraform_registry_module("terraform-aws-modules/vpc/aws", "~> 5.0", &|url| {
            assert!(url.contains("/v1/modules/terraform-aws-modules/vpc/aws/versions"));
            Ok(body.to_string())
        })
        .unwrap();
    assert_eq!(pinned, "5.1.2");
}

#[test]
fn registry_provider_picks_latest_matching_via_injected_http() {
    let body = r#"{
      "id": "hashicorp/aws",
      "versions": [
        {"version": "5.0.0"},
        {"version": "5.100.0"},
        {"version": "6.0.0"}
      ]
    }"#;
    let pinned = resolve_terraform_registry_provider("hashicorp/aws", "~> 5.0", &|url| {
        assert!(url.contains("/v1/providers/hashicorp/aws/versions"));
        Ok(body.to_string())
    })
    .unwrap();
    assert_eq!(pinned, "5.100.0");
}

#[test]
fn network_registry_module_optional() {
    if std::env::var("PINNER_NETWORK").ok().as_deref() != Some("1") {
        eprintln!("skip network_registry_module_optional (set PINNER_NETWORK=1)");
        return;
    }
    let pinned =
        resolve_terraform_registry_module("terraform-aws-modules/vpc/aws", "~> 5.0", &|url| {
            let output = std::process::Command::new("curl")
                .args(["-fsSL", "--max-time", "60", url])
                .output()
                .expect("curl");
            assert!(output.status.success(), "curl failed for {url}");
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        })
        .expect("network module resolve");
    let major = pinned.split('.').next().unwrap_or("");
    assert_eq!(major, "5", "expected ~> 5.0 pin, got {pinned}");
}
