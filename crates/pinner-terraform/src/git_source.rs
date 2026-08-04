/// Rewrite `ref=` in a git/http module source to `sha` (40-char hex or branch/tag name).
pub(crate) fn rewrite_git_ref(source: &str, sha: &str) -> String {
    if let Some((base, query)) = source.split_once('?') {
        let mut parts: Vec<String> = query
            .split('&')
            .map(|part| {
                if part.strip_prefix("ref=").is_some() {
                    format!("ref={sha}")
                } else {
                    part.to_string()
                }
            })
            .collect();
        if !parts.iter().any(|p| p.starts_with("ref=")) {
            parts.push(format!("ref={sha}"));
        }
        format!("{base}?{}", parts.join("&"))
    } else if source.contains('?') {
        source.to_string()
    } else {
        format!("{source}?ref={sha}")
    }
}

/// Resolved pin for a git module: full source URL with `ref=` set to `resolved_ref`.
pub(crate) fn git_pinned_source(requested: &str, resolved_ref: &str) -> String {
    rewrite_git_ref(requested, resolved_ref)
}

/// Extract the `ref=` query value from a git/http module source, if present.
pub(crate) fn git_ref_from_source(source: &str) -> Option<&str> {
    let after_q = source.split_once('?')?.1;
    for part in after_q.split('&') {
        if let Some(value) = part.strip_prefix("ref=") {
            return Some(value);
        }
    }
    None
}

/// Ref token to pass to [`rewrite_git_ref`] — either a bare sha/tag or embedded in a full source URL.
pub(crate) fn git_ref_for_rewrite(pinned: &str) -> &str {
    git_ref_from_source(pinned).unwrap_or(pinned)
}

pub(crate) fn is_git_or_http_requested(requested: &str) -> bool {
    let s = requested.trim();
    s.starts_with("git::")
        || s.starts_with("git@")
        || s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("github.com/")
        || s.starts_with("bitbucket.org/")
        || s.starts_with("gitlab.com/")
}

#[cfg(test)]
mod tests {
    use super::{git_pinned_source, git_ref_for_rewrite, rewrite_git_ref};

    const SHA: &str = "11bd71901bbe5b1630ceea73d27597364c9af683";
    const SOURCE: &str = "git::https://example.com/org/mod.git?ref=main";

    #[test]
    fn rewrite_git_ref_replaces_existing() {
        assert_eq!(rewrite_git_ref(SOURCE, SHA), git_pinned_source(SOURCE, SHA));
    }

    #[test]
    fn git_ref_for_rewrite_accepts_bare_sha_or_full_url() {
        assert_eq!(git_ref_for_rewrite(SHA), SHA);
        assert_eq!(git_ref_for_rewrite(&git_pinned_source(SOURCE, SHA)), SHA);
    }
}
