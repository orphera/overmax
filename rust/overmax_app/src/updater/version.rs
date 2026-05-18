//! Version ordering (matches Python `_parse_version` / `_is_newer_version`).

pub fn parse_version_tuple(version_text: &str) -> Option<Vec<u32>> {
    let mut s = version_text.trim();
    if s.to_ascii_lowercase().starts_with('v') {
        s = &s[1..];
    }
    let mut out = Vec::new();
    for part in s.split('.') {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        out.push(part.parse().ok()?);
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

pub fn is_newer_version(remote_tag: &str, local_version: &str) -> bool {
    let remote = parse_version_tuple(remote_tag);
    let local = parse_version_tuple(local_version);
    match (remote, local) {
        (Some(r), Some(l)) => r > l,
        _ => {
            remote_tag.trim().to_ascii_lowercase()
                != format!("v{}", local_version.trim().to_ascii_lowercase())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_semver() {
        assert!(is_newer_version("v0.1.7", "0.1.6"));
        assert!(!is_newer_version("v0.1.6", "0.1.6"));
    }
}
