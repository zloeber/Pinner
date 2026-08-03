use std::fs;
use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn golden_mise_lock_matches_schema() {
    let root = repo_root();
    let schema_raw = fs::read_to_string(root.join("schemas/pinner.lock.schema.json"))
        .expect("read schemas/pinner.lock.schema.json");
    let lock_raw = fs::read_to_string(root.join("tests/fixtures/golden/pinner.lock.json"))
        .expect("read tests/fixtures/golden/pinner.lock.json");

    let schema: serde_json::Value =
        serde_json::from_str(&schema_raw).expect("parse lock schema JSON");
    let instance: serde_json::Value =
        serde_json::from_str(&lock_raw).expect("parse golden lock JSON");

    if let Err(err) = jsonschema::validate(&schema, &instance) {
        panic!("golden lock failed schema validation: {err}");
    }
}

#[test]
fn pin_output_from_mise_fixture_matches_schema() {
    let root = repo_root();
    let schema_raw = fs::read_to_string(root.join("schemas/pinner.lock.schema.json"))
        .expect("read schemas/pinner.lock.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_raw).expect("parse lock schema JSON");

    let src = root.join("tests/fixtures/mise-floating");
    let dir = tempfile::tempdir().unwrap();
    for name in [".mise.toml", ".tool-versions"] {
        fs::copy(src.join(name), dir.path().join(name)).unwrap();
    }

    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .env(
            "PINNER_MISE_RESOLVE_MAP",
            "node=22.11.0,python=3.12.7,ruby=3.3.5",
        )
        .args(["pin"])
        .assert()
        .success();

    let lock_raw = fs::read_to_string(dir.path().join("pinner.lock.json")).unwrap();
    let instance: serde_json::Value =
        serde_json::from_str(&lock_raw).expect("parse pinned lock JSON");

    if let Err(err) = jsonschema::validate(&schema, &instance) {
        panic!("mise pin lock failed schema validation: {err}\n{lock_raw}");
    }
}
