use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use pinner_ecosystem::{
    Ecosystem, EcosystemCtx, EcosystemKind, EvidenceKind, Finding, Pin, ResolveMode,
};
use pinner_terraform::TerraformEcosystem;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

const GIT_SHA: &str = "11bd71901bbe5b1630ceea73d27597364c9af683";
const GIT_SOURCE: &str = "git::https://example.com/org/mod.git?ref=main";

fn upgrade_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/terraform-upgrade")
}

#[test]
fn resolve_and_rewrite_via_env_map() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only env seam for deterministic resolve.
    unsafe {
        std::env::set_var(
            "PINNER_TERRAFORM_RESOLVE_MAP",
            format!("vpc@~> 5.0=5.1.0,hashicorp/aws@~> 5.0=5.100.0,git_mod@{GIT_SOURCE}={GIT_SHA}"),
        );
    }

    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/terraform-floating");
    let tmp = tempfile::tempdir().unwrap();
    for name in ["modules.tf", "providers.tf"] {
        std::fs::copy(fixture.join(name), tmp.path().join(name)).unwrap();
    }

    let eco = TerraformEcosystem;
    let ctx = EcosystemCtx {
        repo: tmp.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };

    let manifests = eco.discover(tmp.path()).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .filter(|f| f.is_floating)
        .collect();

    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert!(pins.iter().any(|p| p.name == "vpc" && p.pinned == "5.1.0"));
    assert!(pins.iter().any(|p| {
        p.name == "git_mod"
            && p.pinned == format!("git::https://example.com/org/mod.git?ref={GIT_SHA}")
    }));
    assert!(
        pins.iter()
            .any(|p| p.name == "hashicorp/aws" && p.pinned == "5.100.0")
    );

    let modules = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("modules.tf"))
        .expect("modules.tf");
    let module_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("modules.tf"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(modules, &module_pins)
        .unwrap()
        .expect("modules rewrite");
    assert!(
        rw.new_contents.contains("version = \"5.1.0\""),
        "expected exact vpc version, got:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains(&format!("ref={GIT_SHA}")),
        "expected git ref sha, got:\n{}",
        rw.new_contents
    );
    assert!(
        rw.new_contents.contains("module \"local_mod\""),
        "local module must be preserved"
    );

    let providers = manifests
        .iter()
        .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some("providers.tf"))
        .expect("providers.tf");
    let provider_pins: Vec<_> = pins
        .iter()
        .filter(|p| p.path == Path::new("providers.tf"))
        .cloned()
        .collect();
    let rw = eco
        .rewrite(providers, &provider_pins)
        .unwrap()
        .expect("providers rewrite");
    assert!(
        rw.new_contents.contains("version = \"5.100.0\""),
        "expected exact provider version, got:\n{}",
        rw.new_contents
    );

    unsafe {
        std::env::remove_var("PINNER_TERRAFORM_RESOLVE_MAP");
    }
}

#[test]
fn native_lock_wins_over_env_resolve_map() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: map must lose to .terraform.lock.hcl when both are present.
    unsafe {
        std::env::set_var(
            "PINNER_TERRAFORM_RESOLVE_MAP",
            "hashicorp/aws@~> 5.0=5.100.0",
        );
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("providers.tf"),
        r#"terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join(".terraform.lock.hcl"),
        r#"provider "registry.terraform.io/hashicorp/aws" {
  version     = "5.42.0"
  constraints = "~> 5.0"
  hashes = [
    "h1:placeholder",
  ]
}
"#,
    )
    .unwrap();

    let eco = TerraformEcosystem;
    let ctx = EcosystemCtx {
        repo: tmp.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    let manifests = eco.discover(tmp.path()).unwrap();
    let findings: Vec<_> = manifests
        .iter()
        .flat_map(|m| eco.extract(m, &ctx).unwrap())
        .filter(|f| f.is_floating)
        .collect();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    let aws = pins
        .iter()
        .find(|p| p.name == "hashicorp/aws")
        .expect("aws pin");
    assert_eq!(
        aws.pinned, "5.42.0",
        "native .terraform.lock.hcl must win over PINNER_TERRAFORM_RESOLVE_MAP; pin={aws:?}"
    );
    assert_eq!(
        aws.evidence,
        EvidenceKind::NativeLock,
        "expected NativeLock evidence; pin={aws:?}"
    );

    unsafe {
        std::env::remove_var("PINNER_TERRAFORM_RESOLVE_MAP");
    }
}

#[test]
fn upgrade_prefers_resolve_map_over_native_lock() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_TERRAFORM_RESOLVE_MAP",
            "hashicorp/aws@~> 5.0=5.200.0",
        );
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("providers.tf"),
        r#"terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join(".terraform.lock.hcl"),
        r#"provider "registry.terraform.io/hashicorp/aws" {
  version     = "5.42.0"
  constraints = "~> 5.0"
  hashes = [
    "h1:placeholder",
  ]
}
"#,
    )
    .unwrap();

    let eco = TerraformEcosystem;
    let stale_lock = [Pin {
        ecosystem: EcosystemKind::Terraform,
        name: "hashicorp/aws".into(),
        requested: "~> 5.0".into(),
        pinned: "5.42.0".into(),
        path: PathBuf::from("providers.tf"),
        evidence: EvidenceKind::Lock,
        metadata: Default::default(),
    }];
    let ctx = EcosystemCtx {
        repo: tmp.path(),
        lock_pins: &stale_lock,
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Terraform,
        name: "hashicorp/aws".into(),
        requested: "~> 5.0".into(),
        path: PathBuf::from("providers.tf"),
        is_floating: true,
    };
    let pins = eco.resolve(&[finding], &ctx);
    unsafe {
        std::env::remove_var("PINNER_TERRAFORM_RESOLVE_MAP");
    }
    let pins = pins.unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].pinned, "5.200.0");
    assert_eq!(pins[0].metadata["previous"], "5.42.0");
    assert_eq!(pins[0].metadata["upgrade"], true);
    assert_eq!(pins[0].metadata["upgrade_channel"], "map");
    assert_ne!(pins[0].evidence, EvidenceKind::Lock);
    assert_ne!(pins[0].evidence, EvidenceKind::NativeLock);
}

#[test]
fn upgrade_omits_when_map_matches_previous() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var(
            "PINNER_TERRAFORM_RESOLVE_MAP",
            "hashicorp/aws@5.100.0=5.100.0",
        );
    }
    let eco = TerraformEcosystem;
    let repo = upgrade_fixture();
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Terraform,
        name: "hashicorp/aws".into(),
        requested: "5.100.0".into(),
        path: PathBuf::from("providers.tf"),
        is_floating: false,
    };
    let pins = eco.resolve(&[finding], &ctx);
    unsafe {
        std::env::remove_var("PINNER_TERRAFORM_RESOLVE_MAP");
    }
    let pins = pins.unwrap();
    assert!(
        pins.is_empty(),
        "unchanged upgrade must be omitted, got {pins:?}"
    );
}

#[test]
fn upgrade_offline_without_map_ignores_native_lock() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: clear map so resolve cannot succeed via seam.
    unsafe {
        std::env::remove_var("PINNER_TERRAFORM_RESOLVE_MAP");
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("providers.tf"),
        r#"terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join(".terraform.lock.hcl"),
        r#"provider "registry.terraform.io/hashicorp/aws" {
  version     = "5.42.0"
  constraints = "~> 5.0"
  hashes = [
    "h1:placeholder",
  ]
}
"#,
    )
    .unwrap();

    let eco = TerraformEcosystem;
    let ctx = EcosystemCtx {
        repo: tmp.path(),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Upgrade,
    };
    let finding = Finding {
        ecosystem: EcosystemKind::Terraform,
        name: "hashicorp/aws".into(),
        requested: "~> 5.0".into(),
        path: PathBuf::from("providers.tf"),
        is_floating: true,
    };
    let err = eco.resolve(&[finding], &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("offline") || msg.contains("PINNER_TERRAFORM_RESOLVE_MAP"),
        "upgrade must not freeze on .terraform.lock.hcl; got {msg}"
    );
}
