use pinner_iac_common::{image_name, normalize_digest_ref, parse_resolve_map};

#[test]
fn image_name_strips_tag_and_digest() {
    assert_eq!(image_name("ghcr.io/org/app:1.2.3"), "ghcr.io/org/app");
    assert_eq!(
        image_name("ghcr.io/org/app@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "ghcr.io/org/app"
    );
}

#[test]
fn normalize_digest_builds_name_at_sha() {
    assert_eq!(
        normalize_digest_ref(
            "alpine:latest",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        )
        .as_deref(),
        Some("alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
}

#[test]
fn parse_resolve_map_still_works() {
    let map = parse_resolve_map("a=b,c=d");
    assert_eq!(map.get("a").map(String::as_str), Some("b"));
}
