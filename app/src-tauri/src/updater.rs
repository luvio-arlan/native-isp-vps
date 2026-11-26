use crate::models::GitHubReleaseInfo;
use reqwest::Client;
use semver::Version;
use serde_json::Value;
use std::time::Duration;

const REPO_RELEASES_API: &str =
    "https://api.github.com/repos/luvio-arlan/native-isp-vps/releases/latest";
const REPO_RELEASES_PAGE: &str =
    "https://github.com/luvio-arlan/native-isp-vps/releases";

pub async fn check_github_release() -> GitHubReleaseInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let mut info = GitHubReleaseInfo {
        current_version: current.clone(),
        latest_version: None,
        has_update: false,
        release_url: REPO_RELEASES_PAGE.to_string(),
        release_notes: None,
        published_at: None,
        updater_ready: false,
    };

    let Ok(client) = Client::builder()
        .timeout(Duration::from_secs(6))
        .user_agent(format!("native-isp-cockpit/{}", current))
        .build()
    else {
        return info;
    };

    let Ok(resp) = client.get(REPO_RELEASES_API).send().await else {
        return info;
    };
    if !resp.status().is_success() {
        return info;
    }
    let Ok(json) = resp.json::<Value>().await else {
        return info;
    };

    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();

    if !tag.is_empty() {
        info.latest_version = Some(tag.clone());
        if let (Ok(cur_v), Ok(lat_v)) = (Version::parse(&current), Version::parse(&tag)) {
            info.has_update = lat_v > cur_v;
        }
    }

    if let Some(url) = json.get("html_url").and_then(|v| v.as_str()) {
        info.release_url = url.to_string();
    }
    info.release_notes = json
        .get("body")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    info.published_at = json
        .get("published_at")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if let Some(assets) = json.get("assets").and_then(|v| v.as_array()) {
        info.updater_ready = assets.iter().any(|a| {
            a.get("name")
                .and_then(|n| n.as_str())
                .is_some_and(|n| n == "latest.json")
        });
    }

    info
}
