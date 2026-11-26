use crate::models::{AppSettings, HandoffPayload, NodeConfig, NodeSecrets};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

const KEYRING_SERVICE: &str = "dev.luvio.native-isp-cockpit";

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

pub fn save_nodes(nodes: &[NodeConfig]) -> Result<(), String> {
    ensure_dirs()?;
    let json = serde_json::to_string_pretty(nodes).map_err(|e| e.to_string())?;
    let path = nodes_file();
    fs::write(&path, json).map_err(|e| format!("write nodes.json failed: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
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
    fs::write(settings_file(), json).map_err(|e| format!("write settings.json failed: {e}"))
}

pub fn save_secrets(node_id: &str, secrets: &NodeSecrets) -> Result<(), String> {
    if secrets.api_token.is_none() && secrets.panel_password.is_none() {
        return Ok(());
    }
    let key = format!("node-{}", node_id);
    let entry = keyring::Entry::new(KEYRING_SERVICE, &key).map_err(|e| e.to_string())?;
    let payload = serde_json::to_string(secrets).map_err(|e| e.to_string())?;
    entry
        .set_password(&payload)
        .map_err(|e| format!("keychain write failed: {e}"))
}

pub fn load_secrets(node_id: &str) -> NodeSecrets {
    let key = format!("node-{}", node_id);
    let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, &key) else {
        return NodeSecrets::default();
    };
    match entry.get_password() {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => NodeSecrets::default(),
    }
}

pub fn delete_secrets(node_id: &str) {
    let key = format!("node-{}", node_id);
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, &key) {
        let _ = entry.delete_credential();
    }
}

pub fn upsert_from_handoff(payload: HandoffPayload) -> Result<NodeConfig, String> {
    let mut nodes = load_nodes();
    let clean_host = payload.host.trim().to_string();
    if clean_host.is_empty() {
        return Err("host cannot be empty".to_string());
    }

    let existing_id = nodes
        .iter()
        .find(|n| n.host == clean_host)
        .map(|n| n.id.clone());
    let id = existing_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    let flag = payload
        .flag
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "🌐".to_string());
    let city = payload
        .city
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| clean_host.clone());
    let isp_tag = payload
        .isp_tag
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Dual ISP".to_string());
    let name = payload
        .name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("{} {} · {}", flag, city, isp_tag));

    let node = NodeConfig {
        id: id.clone(),
        name,
        flag,
        city,
        isp_tag,
        host: clean_host,
        vless_port: payload.vless_port.unwrap_or(443),
        panel_url: payload.panel_url.trim().trim_end_matches('/').to_string(),
        panel_username: payload.panel_username.filter(|s| !s.trim().is_empty()),
        sub_url: payload.sub_url.filter(|s| !s.trim().is_empty()),
        mihomo_sub_url: payload.mihomo_sub_url.filter(|s| !s.trim().is_empty()),
        ssh_port: payload.ssh_port.unwrap_or(22),
        ssh_user: payload
            .ssh_user
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "root".to_string()),
        ssh_key_path: payload.ssh_key_path.filter(|s| !s.trim().is_empty()),
        reality_target: payload.reality_target.filter(|s| !s.trim().is_empty()),
        monthly_total_gb: payload.monthly_total_gb.unwrap_or(1000.0),
        reset_day: payload.reset_day.unwrap_or(1).clamp(1, 31),
        created_at: Utc::now().to_rfc3339(),
        source: payload.source.unwrap_or_else(|| "ai-handoff".to_string()),
    };

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
    let _ = save_secrets(&id, &secrets);

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
