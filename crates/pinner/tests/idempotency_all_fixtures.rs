use assert_cmd::prelude::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct FixtureCase {
    name: &'static str,
    env: &'static [(&'static str, &'static str)],
    /// Optional `pinner.toml` for opt-in ecosystems (helm/k8s/gitlab/azure).
    config: Option<&'static str>,
}

const OPT_IN_HELM_K8S: &str = "[ecosystems]\nhelm = true\nk8s = true\n";
const OPT_IN_GITLAB: &str = "[ecosystems]\ngitlab = true\n";
const OPT_IN_AZURE: &str = "[ecosystems]\nazure = true\n";

const FIXTURES: &[FixtureCase] = &[
    FixtureCase {
        name: "mise-floating",
        env: &[(
            "PINNER_MISE_RESOLVE_MAP",
            "node=22.11.0,python=3.12.7,ruby=3.3.5",
        )],
        config: None,
    },
    FixtureCase {
        name: "mise-nested",
        env: &[("PINNER_MISE_RESOLVE_MAP", "node=22.11.0")],
        config: None,
    },
    FixtureCase {
        name: "node-floating",
        env: &[],
        config: None,
    },
    FixtureCase {
        name: "python-floating",
        env: &[],
        config: None,
    },
    FixtureCase {
        name: "docker-floating",
        env: &[(
            "PINNER_DOCKER_RESOLVE_MAP",
            "python:3.12=python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,alpine:latest=alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )],
        config: None,
    },
    FixtureCase {
        name: "actions-floating",
        env: &[
            (
                "PINNER_DOCKER_RESOLVE_MAP",
                "node:20=node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,redis:latest=redis@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
            (
                "PINNER_ACTIONS_RESOLVE_MAP",
                "actions/checkout@v4=11bd71901bbe5b1630ceea73d27597364c9af683,org/repo/.github/workflows/reuse.yml@v1=cccccccccccccccccccccccccccccccccccccccc",
            ),
        ],
        config: None,
    },
    FixtureCase {
        name: "terraform-floating",
        env: &[(
            "PINNER_TERRAFORM_RESOLVE_MAP",
            "vpc@~> 5.0=5.1.0,hashicorp/aws@~> 5.0=5.100.0,git_mod@git::https://example.com/org/mod.git?ref=main=11bd71901bbe5b1630ceea73d27597364c9af683",
        )],
        config: None,
    },
    FixtureCase {
        name: "cargo-floating",
        env: &[],
        config: None,
    },
    FixtureCase {
        name: "go-floating",
        env: &[],
        config: None,
    },
    FixtureCase {
        name: "ruby-floating",
        env: &[],
        config: None,
    },
    FixtureCase {
        name: "gitlab-floating",
        env: &[
            (
                "PINNER_DOCKER_RESOLVE_MAP",
                "node:latest=node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            (
                "PINNER_GITLAB_RESOLVE_MAP",
                "group/ci-templates@group/ci-templates@main=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
        ],
        config: Some(OPT_IN_GITLAB),
    },
    FixtureCase {
        name: "azure-floating",
        env: &[
            (
                "PINNER_DOCKER_RESOLVE_MAP",
                "node:latest=node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            (
                "PINNER_AZURE_RESOLVE_MAP",
                "UseNode@UseNode@1=UseNode@1.2.3",
            ),
        ],
        config: Some(OPT_IN_AZURE),
    },
    FixtureCase {
        name: "helm-floating",
        env: &[(
            "PINNER_HELM_RESOLVE_MAP",
            "redis@*=18.6.1,postgresql@^12.1.0=12.5.8,nginx@latest=15.5.0,ingress-nginx@=4.10.0,podinfo@>=6.0.0=6.5.4,argo-cd@~2.4.0=2.4.17,ghcr.io/example/app@ghcr.io/example/app:latest=ghcr.io/example/app@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,nginx@nginx:latest=nginx@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,redis@redis:latest=redis@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        )],
        config: Some(OPT_IN_HELM_K8S),
    },
    FixtureCase {
        name: "k8s-floating",
        env: &[(
            "PINNER_K8S_RESOLVE_MAP",
            "nginx@nginx:latest=nginx@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,busybox@busybox:1.36=busybox@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,alpine@alpine=alpine@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc,python@python:3.12=python@sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        )],
        config: Some(OPT_IN_HELM_K8S),
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
        if let Some(config) = case.config {
            fs::write(dir.path().join("pinner.toml"), config).unwrap_or_else(|e| {
                panic!("write pinner.toml for {}: {e}", case.name);
            });
        }

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
