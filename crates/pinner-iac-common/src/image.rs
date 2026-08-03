use pinner_toolchain::CommandRunner;

/// Repository/name portion before tag or digest.
pub fn image_name(image: &str) -> String {
    let image = image.trim();
    if let Some((repo, _)) = image.split_once('@') {
        return repo.to_string();
    }
    // Tag separator is the last ':' that is not part of a registry port (host:port/repo).
    if let Some(idx) = find_tag_colon(image) {
        return image[..idx].to_string();
    }
    image.to_string()
}

fn find_tag_colon(image: &str) -> Option<usize> {
    // Prefer last ':' after the final '/'; if none, last ':' only when no '/'.
    let after_slash = image.rfind('/').map(|i| i + 1).unwrap_or(0);
    image[after_slash..].rfind(':').map(|i| after_slash + i)
}

/// Accept `repo@sha256:…` or bare `sha256:…` and return `name@sha256:…`.
pub fn normalize_digest_ref(requested: &str, digest_or_ref: &str) -> Option<String> {
    let value = digest_or_ref.trim();
    if value.is_empty() || value == "<no value>" {
        return None;
    }
    if value.contains("@sha256:") {
        return Some(value.to_string());
    }
    let digest = if let Some(rest) = value.strip_prefix("sha256:") {
        format!("sha256:{rest}")
    } else if value.chars().all(|c| c.is_ascii_hexdigit()) && value.len() == 64 {
        format!("sha256:{value}")
    } else {
        return None;
    };
    let name = image_name(requested);
    Some(format!("{name}@{digest}"))
}

/// Resolve an image reference to `name@sha256:…` via docker inspect, then buildx imagetools.
pub fn resolve_image_digest(runner: &dyn CommandRunner, image: &str) -> Result<String, String> {
    if let Some(pinned) = resolve_via_docker_inspect(runner, image) {
        return Ok(pinned);
    }
    resolve_via_registry(runner, image)
}

fn resolve_via_docker_inspect(runner: &dyn CommandRunner, image: &str) -> Option<String> {
    let output = runner
        .run(
            "docker",
            &[
                "image",
                "inspect",
                "--format",
                "{{index .RepoDigests 0}}",
                image,
            ],
        )
        .ok()?;
    if output.status != 0 {
        return None;
    }
    let digest = first_nonempty_line(&output.stdout);
    normalize_digest_ref(image, &digest)
}

fn resolve_via_registry(runner: &dyn CommandRunner, image: &str) -> Result<String, String> {
    let output = runner
        .run(
            "docker",
            &[
                "buildx",
                "imagetools",
                "inspect",
                "--format",
                "{{.Manifest.Digest}}",
                image,
            ],
        )
        .map_err(|err| format!("docker buildx imagetools inspect {image}: {err}"))?;
    if output.status != 0 {
        return Err(format!(
            "docker buildx imagetools inspect {image} failed (status {}): {}",
            output.status,
            output.stderr.trim()
        ));
    }
    let digest = first_nonempty_line(&output.stdout);
    normalize_digest_ref(image, &digest)
        .ok_or_else(|| format!("docker buildx imagetools inspect {image} returned no digest"))
}

fn first_nonempty_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{image_name, normalize_digest_ref};

    #[test]
    fn image_name_strips_tag() {
        assert_eq!(image_name("python:3.12"), "python");
        assert_eq!(image_name("ghcr.io/org/app:1.0"), "ghcr.io/org/app");
    }

    #[test]
    fn normalize_bare_and_full_digests() {
        assert_eq!(
            normalize_digest_ref(
                "python:3.12",
                "python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            )
            .as_deref(),
            Some("python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            normalize_digest_ref(
                "alpine:latest",
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            )
            .as_deref(),
            Some("alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }
}
