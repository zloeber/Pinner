use assert_cmd::prelude::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct FixtureCase {
    name: &'static str,
    env: &'static [(&'static str, &'static str)],
}

const FIXTURES: &[FixtureCase] = &[
    FixtureCase {
        name: "mise-floating",
        env: &[(
            "PINNER_MISE_RESOLVE_MAP",
            "node=22.11.0,python=3.12.7,ruby=3.3.5",
        )],
    },
    FixtureCase {
        name: "node-floating",
        env: &[],
    },
    FixtureCase {
        name: "python-floating",
        env: &[],
    },
    FixtureCase {
        name: "docker-floating",
        env: &[(
            "PINNER_DOCKER_RESOLVE_MAP",
            "python:3.12=python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,alpine:latest=alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )],
    },
    FixtureCase {
        name: "actions-floating",
        env: &[(
            "PINNER_ACTIONS_RESOLVE_MAP",
            "actions/checkout@v4=11bd71901bbe5b1630ceea73d27597364c9af683",
        )],
    },
    FixtureCase {
        name: "terraform-floating",
        env: &[(
            "PINNER_TERRAFORM_RESOLVE_MAP",
            "vpc@~> 5.0=5.1.0,hashicorp/aws@~> 5.0=5.100.0,git_mod@git::https://example.com/org/mod.git?ref=main=11bd71901bbe5b1630ceea73d27597364c9af683",
        )],
    },
];

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap_or_else(|e| panic!("mkdir {}: {e}", dst.display()));
    for entry in fs::read_dir(src).unwrap_or_else(|e| panic!("read_dir {}: {e}", src.display())) {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to);
        } else if ty.is_file() {
            fs::copy(entry.path(), &to).unwrap_or_else(|e| {
                panic!("copy {} -> {}: {e}", entry.path().display(), to.display());
            });
        }
    }
}

fn file_hash(path: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Content hashes of every regular file under `root`, keyed by relative path.
fn dir_hashes(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    walk_files(root, root, &mut out);
    out
}

fn walk_files(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let entry = entry.unwrap();
        let path = entry.path();
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            walk_files(root, &path, out);
        } else if ty.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, file_hash(&path));
        }
    }
}

fn run_pinner(dir: &Path, env: &[(&str, &str)], args: &[&str]) {
    let mut cmd = Command::cargo_bin("pinner").unwrap();
    cmd.current_dir(dir).args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.assert().success();
}

#[test]
fn all_floating_fixtures_pin_idempotent_and_check_clean() {
    for case in FIXTURES {
        let src = fixtures_root().join(case.name);
        assert!(src.is_dir(), "missing fixture {}", case.name);

        let dir = tempfile::tempdir().unwrap();
        copy_dir(&src, dir.path());

        run_pinner(dir.path(), case.env, &["pin"]);

        let lock = dir.path().join("pinner.lock.json");
        assert!(
            lock.is_file(),
            "pin must write pinner.lock.json for {}",
            case.name
        );

        let before = dir_hashes(dir.path());
        run_pinner(dir.path(), case.env, &["pin"]);
        let after = dir_hashes(dir.path());
        assert_eq!(
            before, after,
            "second pin must not change any files for {}",
            case.name
        );

        run_pinner(dir.path(), case.env, &["check"]);
    }
}
