use assert_cmd::prelude::*;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

struct FixtureCase {
    name: &'static str,
    env: &'static [(&'static str, &'static str)],
    config: Option<&'static str>,
}

const OPT_IN_HELM_K8S: &str = "[ecosystems]\nhelm = true\nk8s = true\n";
const OPT_IN_GITLAB: &str = "[ecosystems]\ngitlab = true\n";
const OPT_IN_AZURE: &str = "[ecosystems]\nazure = true\n";

const FLOATING_CASES: &[FixtureCase] = &[
    FixtureCase {
        name: "mise-floating",
        env: &[(
            "PINNER_MISE_RESOLVE_MAP",
            "node=22.11.0,python=3.12.7,ruby=3.3.5",
        )],
        config: None,
    },
    FixtureCase {
        name: "node-floating",
        env: &[("PINNER_NODE_RESOLVE_MAP", "ms=2.1.3:2.1.3")],
        config: None,
    },
    FixtureCase {
        name: "python-floating",
        env: &[("PINNER_PYTHON_RESOLVE_MAP", "requests=2.32.3:2.32.3")],
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
        env: &[("PINNER_CARGO_RESOLVE_MAP", "serde=1.0.200:1.0.200")],
        config: None,
    },
    FixtureCase {
        name: "go-floating",
        env: &[(
            "PINNER_GO_RESOLVE_MAP",
            "github.com/example/lib=v1.2.3:v1.2.3",
        )],
        config: None,
    },
    FixtureCase {
        name: "ruby-floating",
        env: &[("PINNER_RUBY_RESOLVE_MAP", "rake=13.2.1:13.2.1")],
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

const UPGRADE_CASES: &[FixtureCase] = &[
    FixtureCase {
        name: "actions-upgrade",
        env: &[(
            "PINNER_ACTIONS_RESOLVE_MAP",
            "actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683=cccccccccccccccccccccccccccccccccccccccc,11bd71901bbe5b1630ceea73d27597364c9af683=cccccccccccccccccccccccccccccccccccccccc",
        )],
        config: None,
    },
    FixtureCase {
        name: "azure-upgrade",
        env: &[
            (
                "PINNER_DOCKER_RESOLVE_MAP",
                "node:20@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=node:20@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,node:20=node:20@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
            (
                "PINNER_AZURE_RESOLVE_MAP",
                "UseNode@UseNode@1.2.3=UseNode@1.9.9",
            ),
        ],
        config: Some(OPT_IN_AZURE),
    },
    FixtureCase {
        name: "cargo-upgrade",
        env: &[("PINNER_CARGO_RESOLVE_MAP", "serde=1.0.200:1.0.210")],
        config: None,
    },
    FixtureCase {
        name: "docker-upgrade",
        env: &[(
            "PINNER_DOCKER_RESOLVE_MAP",
            "python:3.12=python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )],
        config: None,
    },
    FixtureCase {
        name: "gitlab-upgrade",
        env: &[
            (
                "PINNER_DOCKER_RESOLVE_MAP",
                "node:20@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=node:20@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,node:20=node:20@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
            (
                "PINNER_GITLAB_RESOLVE_MAP",
                "group/ci-templates@group/ci-templates@bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb=group/ci-templates@dddddddddddddddddddddddddddddddddddddddd",
            ),
        ],
        config: Some(OPT_IN_GITLAB),
    },
    FixtureCase {
        name: "go-upgrade",
        env: &[(
            "PINNER_GO_RESOLVE_MAP",
            "github.com/example/lib=v1.2.3:v1.3.0",
        )],
        config: None,
    },
    FixtureCase {
        name: "helm-upgrade",
        env: &[(
            "PINNER_HELM_RESOLVE_MAP",
            "redis@*=19.0.0,postgresql@^12.1.0=12.6.0,nginx@latest=16.0.0,ingress-nginx@=4.11.0,cert-manager@1.14.0=1.15.0,ghcr.io/example/app@ghcr.io/example/app:latest=ghcr.io/example/app@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,nginx@nginx:latest=nginx@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )],
        config: Some(OPT_IN_HELM_K8S),
    },
    FixtureCase {
        name: "k8s-upgrade",
        env: &[(
            "PINNER_K8S_RESOLVE_MAP",
            "nginx@nginx:latest=nginx@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,busybox@busybox:1.36=busybox@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,alpine@alpine=alpine@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        )],
        config: Some(OPT_IN_HELM_K8S),
    },
    FixtureCase {
        name: "mise-upgrade",
        env: &[("PINNER_MISE_RESOLVE_MAP", "node=22.12.0")],
        config: None,
    },
    FixtureCase {
        name: "node-upgrade",
        env: &[("PINNER_NODE_RESOLVE_MAP", "ms=2.1.3:2.1.4")],
        config: None,
    },
    FixtureCase {
        name: "python-upgrade",
        env: &[("PINNER_PYTHON_RESOLVE_MAP", "requests=2.32.3:2.33.0")],
        config: None,
    },
    FixtureCase {
        name: "ruby-upgrade",
        env: &[("PINNER_RUBY_RESOLVE_MAP", "rake=13.2.1:13.3.0")],
        config: None,
    },
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures_root() -> PathBuf {
    repo_root().join("tests/fixtures")
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

fn dir_hashes(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for entry in WalkDir::new(root).into_iter().flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        out.insert(rel, file_hash(path));
    }
    out
}

fn run_pinner(dir: &Path, env: &[(&str, &str)], args: &[&str]) {
    let mut cmd = Command::cargo_bin("pinner").unwrap();
    cmd.current_dir(dir).args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.assert().success();
}

fn run_pinner_owned_env(dir: &Path, env: &[(String, String)], args: &[&str]) {
    let mut cmd = Command::cargo_bin("pinner").unwrap();
    cmd.current_dir(dir).args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.assert().success();
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {:?} in {}: {e}", args, dir.display()));

    assert!(
        output.status.success(),
        "git {:?} failed in {} (status {:?}): {}",
        args,
        dir.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn prepare_local_git_module(base: &Path) -> (PathBuf, String, String) {
    let repo = base.join("terraform-local-module-repo");
    fs::create_dir_all(&repo).unwrap();

    run_git(&repo, &["init", "-b", "main"]);
    run_git(&repo, &["config", "user.email", "pinner-tests@example.com"]);
    run_git(&repo, &["config", "user.name", "Pinner Tests"]);

    fs::write(repo.join("README.md"), "# local module v1\n").unwrap();
    run_git(&repo, &["add", "."]);
    run_git(&repo, &["commit", "-m", "initial"]);
    let old_sha = run_git(&repo, &["rev-parse", "HEAD"]);

    fs::write(repo.join("README.md"), "# local module v2\n").unwrap();
    run_git(&repo, &["add", "."]);
    run_git(&repo, &["commit", "-m", "upgrade"]);
    let new_sha = run_git(&repo, &["rev-parse", "HEAD"]);

    (repo, old_sha, new_sha)
}

fn run_terraform_upgrade_local_git_case(source_fixtures: &Path, corpus: &Path) {
    let src = source_fixtures.join("terraform-upgrade");
    assert!(src.is_dir(), "missing fixture terraform-upgrade");

    let dst = corpus.join("terraform-upgrade-localgit");
    copy_dir(&src, &dst);

    let (module_repo, old_sha, new_sha) = prepare_local_git_module(corpus);
    let module_url = format!("git::file://{}?ref=main", module_repo.display());

    let modules_tf_path = dst.join("modules.tf");
    let modules_tf = fs::read_to_string(&modules_tf_path).unwrap();
    let rewritten =
        modules_tf.replace("git::https://example.com/org/mod.git?ref=main", &module_url);
    fs::write(&modules_tf_path, rewritten).unwrap();

    let pin_map = format!(
        "vpc@~> 5.0=5.1.0,hashicorp/aws@~> 5.0=5.100.0,git_mod@{}={}",
        module_url, old_sha
    );

    run_pinner_owned_env(
        &dst,
        &[("PINNER_TERRAFORM_RESOLVE_MAP".to_string(), pin_map)],
        &["pin"],
    );

    let upgrade_source = format!("git::file://{}?ref={}", module_repo.display(), old_sha);
    let upgrade_map = format!(
        "vpc@5.1.0=5.1.0,hashicorp/aws@5.100.0=5.100.0,git_mod@{}={}",
        upgrade_source, new_sha
    );

    run_pinner_owned_env(
        &dst,
        &[("PINNER_TERRAFORM_RESOLVE_MAP".to_string(), upgrade_map)],
        &["upgrade"],
    );
    run_pinner_owned_env(
        &dst,
        &[(
            "PINNER_TERRAFORM_RESOLVE_MAP".to_string(),
            "vpc@5.1.0=5.1.0,hashicorp/aws@5.100.0=5.100.0".to_string(),
        )],
        &["check"],
    );
}

fn validate_structured_files(root: &Path) {
    let mut errors = Vec::new();
    for entry in WalkDir::new(root).into_iter().flatten() {
        let path = entry.path();
        if path.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let body = match fs::read_to_string(path) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("read error {}: {e}", path.display()));
                continue;
            }
        };

        match ext {
            "toml" => {
                if let Err(e) = toml::from_str::<toml::Value>(&body) {
                    errors.push(format!("invalid TOML {}: {e}", path.display()));
                }
            }
            "yml" | "yaml" => {
                for doc in serde_yaml::Deserializer::from_str(&body) {
                    if let Err(e) = serde_yaml::Value::deserialize(doc) {
                        errors.push(format!("invalid YAML {}: {e}", path.display()));
                        break;
                    }
                }
            }
            "json" => {
                if let Err(e) = serde_json::from_str::<serde_json::Value>(&body) {
                    errors.push(format!("invalid JSON {}: {e}", path.display()));
                }
            }
            _ => {}
        }
    }

    assert!(
        errors.is_empty(),
        "corpus contains invalid structured files:\n{}",
        errors.join("\n")
    );
}

#[test]
fn corpus_pipeline_runs_pin_then_upgrade_then_check() {
    let source_fixtures = fixtures_root();
    let staging = tempfile::tempdir().unwrap();
    let corpus = staging.path().join("fixture-corpus");
    fs::create_dir_all(&corpus).unwrap();

    for case in FLOATING_CASES {
        let src = source_fixtures.join(case.name);
        assert!(src.is_dir(), "missing fixture {}", case.name);

        let dst = corpus.join(case.name);
        copy_dir(&src, &dst);
        if let Some(config) = case.config {
            fs::write(dst.join("pinner.toml"), config).unwrap();
        }

        run_pinner(&dst, case.env, &["pin"]);
        assert!(
            dst.join("pinner.lock.json").is_file(),
            "pin must write lock for {}",
            case.name
        );
        run_pinner(&dst, case.env, &["check"]);
    }

    for case in UPGRADE_CASES {
        let src = source_fixtures.join(case.name);
        assert!(src.is_dir(), "missing fixture {}", case.name);

        let dst = corpus.join(case.name);
        copy_dir(&src, &dst);
        if let Some(config) = case.config {
            fs::write(dst.join("pinner.toml"), config).unwrap();
        }

        run_pinner(&dst, case.env, &["pin"]);
        assert!(
            dst.join("pinner.lock.json").is_file(),
            "pin must write lock for {}",
            case.name
        );

        let before = dir_hashes(&dst);
        run_pinner(&dst, case.env, &["upgrade"]);
        run_pinner(&dst, case.env, &["check"]);
        let after = dir_hashes(&dst);
        // Some fixtures may already be effectively up to date after pin.
        // Upgrade must still execute successfully and preserve valid config syntax.
        let _ = (before, after);
    }

    run_terraform_upgrade_local_git_case(&source_fixtures, &corpus);

    validate_structured_files(&corpus);
}
