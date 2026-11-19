//! Checks GitHub for a newer public release.
//!
//! Release builds carry their release tag (the workflow sets
//! `HOI4_ANALYZER_RELEASE` at build time); development builds have none and
//! never check.

use std::time::Duration;

use serde::Deserialize;

use crate::inject::version_key;

pub const RELEASES_PAGE: &str = "https://github.com/zVSciy/Hoi4_IC_Graphs/releases";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/zVSciy/Hoi4_IC_Graphs/releases/latest";

/// The release tag this build was published as, e.g. `v1.0.7`.
pub fn current_release() -> Option<&'static str> {
    option_env!("HOI4_ANALYZER_RELEASE").filter(|tag| !tag.trim().is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub tag: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

/// `Ok(Some(update))` if GitHub has a newer release than this build.
pub fn check() -> Result<Option<Update>, String> {
    let current = current_release().ok_or("this is a development build without a release tag")?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let release: Release = agent
        .get(LATEST_RELEASE_API)
        .header(
            "User-Agent",
            concat!("HOI4-Analyzer/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("could not reach GitHub: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("unexpected answer from GitHub: {e}"))?;
    log::info!(
        "Latest release: {} (this build: {current})",
        release.tag_name
    );
    let newer = is_newer(&release.tag_name, current);
    Ok(newer.then_some(Update {
        tag: release.tag_name,
        url: release.html_url,
    }))
}

/// Numeric comparison of release tags (`v1.0.10` > `v1.0.9`); tags that are
/// not version numbers never count as newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let key = |tag: &str| version_key(tag.trim().trim_start_matches(['v', 'V']));
    let (latest, current) = (key(latest), key(current));
    latest.len() >= 2 && current.len() >= 2 && latest > current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_tags() {
        assert!(is_newer("v1.0.10", "v1.0.9"));
        assert!(is_newer("v2.0", "v1.9.9"));
        assert!(!is_newer("v1.0.9", "v1.0.9"));
        assert!(!is_newer("v1.0.8", "v1.0.9"));
        assert!(!is_newer("nightly", "v1.0.9"));
        assert!(!is_newer("v1.0.9", "dev"));
    }
}
