mod git;
mod image;

pub use git::resolve_git_sha;
pub use image::{image_name, normalize_digest_ref, resolve_image_digest};

use std::collections::HashMap;

pub fn parse_resolve_map(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        // Prefer last `=` so keys may contain `=` (e.g. Terraform git `?ref=main`).
        let Some((key, value)) = entry.rsplit_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        // Empty keys are allowed (e.g. Helm missing chart version → `=1.2.3`).
        if !value.is_empty() {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::parse_resolve_map;

    #[test]
    fn parse_entries() {
        let m = parse_resolve_map("a=b,c=d");
        assert_eq!(m.get("a").map(String::as_str), Some("b"));
        assert_eq!(m.get("c").map(String::as_str), Some("d"));
    }

    #[test]
    fn parse_entries_key_may_contain_equals() {
        let m = parse_resolve_map(
            "git::https://example.com/org/mod.git?ref=main=11bd71901bbe5b1630ceea73d27597364c9af683",
        );
        assert_eq!(
            m.get("git::https://example.com/org/mod.git?ref=main")
                .map(String::as_str),
            Some("11bd71901bbe5b1630ceea73d27597364c9af683")
        );
    }

    #[test]
    fn parse_entries_empty_key() {
        let m = parse_resolve_map("=4.10.0,*=18.6.1");
        assert_eq!(m.get("").map(String::as_str), Some("4.10.0"));
        assert_eq!(m.get("*").map(String::as_str), Some("18.6.1"));
    }
}
