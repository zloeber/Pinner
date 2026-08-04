mod git;
mod http;
mod image;
mod version;

pub use git::resolve_git_sha;
pub use http::http_get;
pub use image::{image_name, normalize_digest_ref, resolve_image_digest};
pub use version::{compare_semver, matches_version_constraint, select_matching_version};

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
        // Empty keys are allowed for legacy bare entries; prefer `name@` / `name@requested`.
        if !value.is_empty() {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

/// Canonical resolve-map key: `{name}@{requested}`.
///
/// Empty `requested` (e.g. Helm missing chart version) becomes `{name}@`
/// (written in maps as `{name}@=pinned`, which parses to key `{name}@`).
pub fn resolve_map_key(name: &str, requested: &str) -> String {
    format!("{name}@{requested}")
}

/// Look up a pinned value for `(name, requested)`.
///
/// Order: exact `{name}@{requested}`, then legacy bare `{requested}` for
/// backward compatibility with older maps/tests.
pub fn resolve_map_lookup(
    map: &HashMap<String, String>,
    name: &str,
    requested: &str,
) -> Option<String> {
    let key = resolve_map_key(name, requested);
    if let Some(pinned) = map.get(&key) {
        return Some(pinned.clone());
    }
    map.get(requested).cloned()
}

#[cfg(test)]
mod tests {
    use super::{parse_resolve_map, resolve_map_key, resolve_map_lookup};

    #[test]
    fn parse_entries() {
        let m = parse_resolve_map("a=b,c=d");
        assert_eq!(m.get("a").map(String::as_str), Some("b"));
        assert_eq!(m.get("c").map(String::as_str), Some("d"));
    }

    #[test]
    fn parse_entries_key_may_contain_equals() {
        let m = parse_resolve_map(
            "git_mod@git::https://example.com/org/mod.git?ref=main=11bd71901bbe5b1630ceea73d27597364c9af683",
        );
        assert_eq!(
            m.get("git_mod@git::https://example.com/org/mod.git?ref=main")
                .map(String::as_str),
            Some("11bd71901bbe5b1630ceea73d27597364c9af683")
        );
    }

    #[test]
    fn parse_entries_empty_requested_name_at() {
        let m = parse_resolve_map("ingress-nginx@=4.10.0,redis@*=18.6.1");
        assert_eq!(m.get("ingress-nginx@").map(String::as_str), Some("4.10.0"));
        assert_eq!(m.get("redis@*").map(String::as_str), Some("18.6.1"));
    }

    #[test]
    fn resolve_map_lookup_prefers_name_at_requested() {
        let m = parse_resolve_map("vpc@~> 5.0=5.1.0,hashicorp/aws@~> 5.0=5.100.0,~> 5.0=9.9.9");
        assert_eq!(
            resolve_map_lookup(&m, "vpc", "~> 5.0").as_deref(),
            Some("5.1.0")
        );
        assert_eq!(
            resolve_map_lookup(&m, "hashicorp/aws", "~> 5.0").as_deref(),
            Some("5.100.0")
        );
    }

    #[test]
    fn resolve_map_lookup_falls_back_to_bare_requested() {
        let m = parse_resolve_map("~> 5.0=5.1.0");
        assert_eq!(
            resolve_map_lookup(&m, "vpc", "~> 5.0").as_deref(),
            Some("5.1.0")
        );
    }

    #[test]
    fn resolve_map_key_empty_requested() {
        assert_eq!(resolve_map_key("ingress-nginx", ""), "ingress-nginx@");
    }
}
