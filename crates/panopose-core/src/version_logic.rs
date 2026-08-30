pub(crate) fn display_version(manifest_version: &str, commit_count: Option<u64>) -> String {
    let Some((major, minor, patch)) = parse_semver_core(manifest_version) else {
        return format!("v{manifest_version}");
    };

    if minor % 2 == 1 {
        format!("v{major}.{minor}.{}", commit_count.unwrap_or(patch))
    } else if patch == 0 {
        format!("v{major}.{minor}")
    } else {
        format!("v{major}.{minor}.{patch}")
    }
}

fn parse_semver_core(version: &str) -> Option<(u64, u64, u64)> {
    let core = version
        .split_once('-')
        .map_or(version, |(core, _)| core)
        .split_once('+')
        .map_or(version, |(core, _)| core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_minor_uses_commit_count_as_patch() {
        assert_eq!(display_version("0.1.0", Some(7)), "v0.1.7");
    }

    #[test]
    fn odd_minor_falls_back_to_manifest_patch_without_git() {
        assert_eq!(display_version("0.1.3", None), "v0.1.3");
    }

    #[test]
    fn even_minor_zero_patch_is_displayed_without_patch() {
        assert_eq!(display_version("1.2.0", Some(99)), "v1.2");
    }

    #[test]
    fn even_minor_nonzero_patch_is_preserved() {
        assert_eq!(display_version("1.2.1", Some(99)), "v1.2.1");
    }
}
