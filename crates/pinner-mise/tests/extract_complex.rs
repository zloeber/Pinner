use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pinner_ecosystem::{Ecosystem, EcosystemCtx, ResolveMode};
use pinner_mise::MiseEcosystem;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mise-complex")
}

fn findings() -> Vec<pinner_ecosystem::Finding> {
    let repo = fixture();
    let eco = MiseEcosystem::default();
    let manifests = eco.discover(&repo).unwrap();
    assert_eq!(manifests.len(), 1, "mise-complex has a single .mise.toml");
    let ctx = EcosystemCtx {
        repo: Path::new("."),
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
        resolve_mode: ResolveMode::Pin,
    };
    eco.extract(&manifests[0], &ctx).unwrap()
}

#[test]
fn complex_discovers_single_mise_toml() {
    let eco = MiseEcosystem::default();
    let manifests = eco.discover(&fixture()).unwrap();
    assert_eq!(manifests.len(), 1);
    assert!(manifests[0].path.ends_with(".mise.toml"));
}

#[test]
fn complex_extracts_all_tools_including_backends() {
    let findings = findings();
    let names: BTreeSet<_> = findings.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains("task"));
    assert!(names.contains("npm:skills"));
    assert!(names.contains("npm:@tobilu/qmd"));
    assert!(names.contains("github:zloeber/pinner"));
    assert!(names.contains("pipx:copier"));
    assert!(names.contains("http:gkg"));
    assert!(names.contains("awscli"));
    assert_eq!(findings.len(), 29, "one finding per [tools] entry");
}

#[test]
fn complex_marks_latest_and_partial_versions_floating() {
    let findings = findings();
    let by_name: std::collections::HashMap<_, _> = findings
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();

    for name in [
        "task",
        "uv",
        "npm:skills",
        "github:zloeber/pinner",
        "glab",
        "awscli",
    ] {
        let f = by_name.get(name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(f.requested, "latest", "{name}");
        assert!(f.is_floating, "{name} should be floating");
    }

    let python = by_name["python"];
    assert_eq!(python.requested, "3.14");
    assert!(python.is_floating, "MAJOR.MINOR is not an exact pin");

    let node = by_name["node"];
    assert_eq!(node.requested, "22");
    assert!(node.is_floating, "MAJOR-only is not an exact pin");
}

#[test]
fn complex_marks_exact_semver_tools_not_floating() {
    let findings = findings();
    let by_name: std::collections::HashMap<_, _> = findings
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();

    for (name, requested) in [
        ("yamllint", "1.38.0"),
        ("ruff", "0.16.0"),
        ("npm:@tobilu/qmd", "2.5.3"),
        ("tflint", "0.63.1"),
        ("tfsec", "1.28.14"),
        ("pipx:copier", "9.16.0"),
        ("http:gkg", "0.24.0"),
    ] {
        let f = by_name.get(name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(f.requested, requested, "{name}");
        assert!(!f.is_floating, "{name} should be exact");
    }
}
