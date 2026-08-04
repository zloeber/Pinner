use std::process::Command;

fn main() {
    let version = git_tag_version().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=PINNER_VERSION={version}");

    // Rebuild when HEAD / tags move so `pinner --version` tracks git.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/tags");
    if let Ok(head) = std::fs::read_to_string("../../.git/HEAD")
        && let Some(r) = head.strip_prefix("ref: ")
    {
        println!("cargo:rerun-if-changed=../../.git/{}", r.trim());
    }
}

/// Latest `v*` tag without the leading `v`, e.g. `0.2.0`.
fn git_tag_version() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--match", "v*", "--abbrev=0"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let tag = String::from_utf8(output.stdout).ok()?;
    let tag = tag.trim();
    if tag.is_empty() {
        return None;
    }
    Some(tag.strip_prefix('v').unwrap_or(tag).to_string())
}
