use std::cmp::Ordering;

/// Pick the highest version from `versions` that satisfies `constraint`.
pub fn select_matching_version(versions: &[String], constraint: &str) -> Option<String> {
    let mut matching: Vec<&String> = versions
        .iter()
        .filter(|v| matches_version_constraint(v, constraint))
        .collect();
    matching.sort_by(|a, b| compare_semver(a, b));
    matching.last().map(|s| (*s).clone())
}

/// Best-effort Terraform/Helm-style constraint match for registry pin selection.
pub fn matches_version_constraint(version: &str, constraint: &str) -> bool {
    let version = version.trim();
    let constraint = constraint.trim();
    if version.is_empty() {
        return false;
    }
    if constraint.is_empty() || constraint == "*" || constraint.eq_ignore_ascii_case("latest") {
        return true;
    }

    let constraint = constraint
        .strip_prefix('=')
        .map(str::trim)
        .unwrap_or(constraint);

    if let Some(rest) = constraint.strip_prefix("~>") {
        return matches_pessimistic(version, rest.trim());
    }
    if let Some(rest) = constraint.strip_prefix(">=") {
        return compare_semver(version, rest.trim()) != Ordering::Less;
    }
    if let Some(rest) = constraint.strip_prefix("<=") {
        return compare_semver(version, rest.trim()) != Ordering::Greater;
    }
    if let Some(rest) = constraint.strip_prefix('>') {
        return compare_semver(version, rest.trim()) == Ordering::Greater;
    }
    if let Some(rest) = constraint.strip_prefix('<') {
        return compare_semver(version, rest.trim()) == Ordering::Less;
    }
    if let Some(rest) = constraint.strip_prefix('^') {
        return matches_caret(version, rest.trim());
    }
    if let Some(rest) = constraint.strip_prefix('~') {
        // npm-style ~1.2.3 → >=1.2.3 <1.3.0; treat like pessimistic with patch.
        return matches_pessimistic(version, rest.trim());
    }

    // Exact or prefix equality on core semver.
    version == constraint || version.starts_with(&format!("{constraint}-"))
}

fn matches_pessimistic(version: &str, bound: &str) -> bool {
    let parts = semver_parts(bound);
    if parts.is_empty() {
        return false;
    }
    if compare_semver(version, bound) == Ordering::Less {
        return false;
    }
    let mut upper = parts;
    if upper.len() == 1 {
        upper[0] += 1;
        upper.push(0);
        upper.push(0);
    } else if upper.len() == 2 {
        upper[0] += 1;
        upper[1] = 0;
        upper.push(0);
    } else {
        // ~> x.y.z → >= x.y.z, < x.(y+1).0
        upper.truncate(3);
        upper[1] += 1;
        upper[2] = 0;
    }
    let upper_s = join_parts(&upper);
    compare_semver(version, &upper_s) == Ordering::Less
}

fn matches_caret(version: &str, bound: &str) -> bool {
    if compare_semver(version, bound) == Ordering::Less {
        return false;
    }
    let parts = semver_parts(bound);
    if parts.is_empty() {
        return false;
    }
    let mut upper = parts;
    if upper[0] > 0 {
        upper[0] += 1;
        for p in upper.iter_mut().skip(1) {
            *p = 0;
        }
        while upper.len() < 3 {
            upper.push(0);
        }
    } else if upper.len() > 1 && upper[1] > 0 {
        upper[1] += 1;
        for p in upper.iter_mut().skip(2) {
            *p = 0;
        }
        while upper.len() < 3 {
            upper.push(0);
        }
    } else {
        // ^0.0.x → patch bump
        while upper.len() < 3 {
            upper.push(0);
        }
        upper[2] += 1;
    }
    let upper_s = join_parts(&upper);
    compare_semver(version, &upper_s) == Ordering::Less
}

/// Compare dotted numeric versions (prerelease suffix sorts before release of same core).
pub fn compare_semver(a: &str, b: &str) -> Ordering {
    let (a_core, a_pre) = split_prerelease(a);
    let (b_core, b_pre) = split_prerelease(b);
    let a_parts = semver_parts(a_core);
    let b_parts = semver_parts(b_core);
    let len = a_parts.len().max(b_parts.len());
    for i in 0..len {
        let av = a_parts.get(i).copied().unwrap_or(0);
        let bv = b_parts.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (Some(a), Some(b)) => a.cmp(b),
    }
}

fn split_prerelease(version: &str) -> (&str, Option<&str>) {
    let version = version.trim();
    match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    }
}

fn semver_parts(version: &str) -> Vec<u64> {
    let core = version.split('+').next().unwrap_or(version);
    core.split('.')
        .filter_map(|p| {
            let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse().ok()
            }
        })
        .collect()
}

fn join_parts(parts: &[u64]) -> String {
    parts
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::{matches_version_constraint, select_matching_version};

    #[test]
    fn pessimistic_constraint() {
        assert!(matches_version_constraint("5.1.0", "~> 5.0"));
        assert!(matches_version_constraint("5.9.9", "~> 5.0"));
        assert!(!matches_version_constraint("6.0.0", "~> 5.0"));
        assert!(matches_version_constraint("5.0.2", "~> 5.0.1"));
        assert!(!matches_version_constraint("5.1.0", "~> 5.0.1"));
    }

    #[test]
    fn caret_and_star() {
        assert!(matches_version_constraint("18.6.1", "^18.0.0"));
        assert!(!matches_version_constraint("19.0.0", "^18.0.0"));
        assert!(matches_version_constraint("1.2.3", "*"));
        assert!(matches_version_constraint("1.2.3", ""));
    }

    #[test]
    fn select_latest_matching() {
        let versions = vec![
            "17.0.0".into(),
            "18.6.1".into(),
            "18.0.0".into(),
            "19.0.0".into(),
        ];
        assert_eq!(
            select_matching_version(&versions, "^18.0.0").as_deref(),
            Some("18.6.1")
        );
        assert_eq!(
            select_matching_version(&versions, "*").as_deref(),
            Some("19.0.0")
        );
    }
}
