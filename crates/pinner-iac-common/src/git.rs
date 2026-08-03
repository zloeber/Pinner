use pinner_toolchain::CommandRunner;

/// Resolve a git ref to its full 40-character SHA via `git ls-remote`.
pub fn resolve_git_sha(
    runner: &dyn CommandRunner,
    repo_url: &str,
    ref_name: &str,
) -> Result<String, String> {
    let output = runner
        .run("git", &["ls-remote", repo_url, ref_name])
        .map_err(|err| format!("git ls-remote {repo_url} {ref_name}: {err}"))?;
    if output.status != 0 {
        return Err(format!(
            "git ls-remote {repo_url} {ref_name} failed (status {}): {}",
            output.status,
            output.stderr.trim()
        ));
    }
    parse_git_ls_remote_sha(&output.stdout)
        .ok_or_else(|| format!("git ls-remote {repo_url} {ref_name} returned no SHA"))
}

fn parse_git_ls_remote_sha(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let sha = line.split_whitespace().next()?;
        if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(sha.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_git_ls_remote_sha;

    #[test]
    fn parses_first_full_sha() {
        let stdout = "abc123def456789012345678901234567890abcd\trefs/heads/main\n";
        assert_eq!(
            parse_git_ls_remote_sha(stdout).as_deref(),
            Some("abc123def456789012345678901234567890abcd")
        );
    }

    #[test]
    fn ignores_empty_lines() {
        let stdout = "\n\nfedcba9876543210fedcba9876543210fedcba98\trefs/tags/v1.0\n";
        assert_eq!(
            parse_git_ls_remote_sha(stdout).as_deref(),
            Some("fedcba9876543210fedcba9876543210fedcba98")
        );
    }
}
