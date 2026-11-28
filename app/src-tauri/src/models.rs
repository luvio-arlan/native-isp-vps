use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeConfig {
    pub id: String,
    pub name: String,
    pub flag: String,
    pub city: String,
    pub isp_tag: String,
    pub host: String,
    #[serde(default = "default_vless_port")]
    pub vless_port: u16,
    pub panel_url: String,
    pub panel_username: Option<String>,
    pub sub_url: Option<String>,
    pub mihomo_sub_url: Option<String>,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    #[serde(default = "default_ssh_user")]
    pub ssh_user: String,
    pub ssh_key_path: Option<String>,
    pub reality_target: Option<String>,
    #[serde(default = "default_monthly_gb")]
    pub monthly_total_gb: f64,
    #[serde(default = "default_reset_day")]
    pub reset_day: u32,
    pub created_at: String,
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_vless_port() -> u16 {
    443
}
fn default_ssh_port() -> u16 {
    22
}
fn default_ssh_user() -> String {
    "root".to_string()
}
fn default_monthly_gb() -> f64 {
    1000.0
}
fn default_reset_day() -> u32 {
    1
}
fn default_source() -> String {
    "manual".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NodeSecrets {
    pub api_token: Option<String>,
    pub panel_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffPayload {
    pub name: Option<String>,
    pub flag: Option<String>,
    pub city: Option<String>,
    pub isp_tag: Option<String>,
    pub host: String,
    pub vless_port: Option<u16>,
    pub panel_url: String,
    pub panel_username: Option<String>,
    pub panel_password: Option<String>,
    pub api_token: Option<String>,
    pub sub_url: Option<String>,
    pub mihomo_sub_url: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_user: Option<String>,
    pub ssh_key_path: Option<String>,
    pub reality_target: Option<String>,
    pub monthly_total_gb: Option<f64>,
    pub reset_day: Option<u32>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct XuiStatus {
    pub connected: bool,
    pub error: Option<String>,
    pub cpu_percent: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub uptime_secs: u64,
    pub loads: [f64; 3],
    pub xray_state: String,
    pub xray_version: String,
    pub net_up_bps: u64,
    pub net_down_bps: u64,
    pub net_sent_total: u64,
    pub net_recv_total: u64,
    pub tcp_count: u64,
    pub udp_count: u64,
    /// How the panel API was reached: `ssh-tunnel` | `tls` | `plaintext` | `paused` (see `panel_tunnel.rs`).
    #[serde(default)]
    pub panel_transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientQuota {
    pub email: String,
    pub enable: bool,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub total_bytes: u64,
    pub reset_day: u32,
    pub online: bool,
    pub sub_id: Option<String>,
    pub sub_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryPoint {
    pub ts: i64,
    pub rtt_ms: Option<f64>,
    pub vps_down_bps: u64,
    pub vps_up_bps: u64,
    pub local_down_bps: u64,
    pub local_up_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryBucket {
    pub ts: i64,
    pub avg_rtt_ms: Option<f64>,
    pub p95_rtt_ms: Option<f64>,
    pub loss_pct: f64,
    pub avg_local_down_bps: u64,
    pub avg_vps_down_bps: u64,
    pub samples: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProbeResult {
    pub name: String,
    pub host: String,
    pub ok: bool,
    pub status_code: Option<u16>,
    pub latency_ms: Option<f64>,
    #[serde(default)]
    pub cold_latency_ms: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalShieldStatus {
    pub mihomo_connected: bool,
    pub mihomo_source: String,
    pub mode: String,
    pub local_up_bps: u64,
    pub local_down_bps: u64,
    pub local_up_total: u64,
    pub local_down_total: u64,
    pub active_connections: usize,
    pub proxy_connections: usize,
    pub direct_connections: usize,
    pub ipv6_disabled: Option<bool>,
    pub tun_enabled: Option<bool>,
    pub tun_stack: Option<String>,
    pub fake_ip_enabled: Option<bool>,
    pub quic_blocked: Option<bool>,
    pub tcp_concurrent: Option<bool>,
    pub exit_ipv4: Option<String>,
    pub ipv6_leak: Option<bool>,
    pub matches_vps_ip: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditItem {
    pub key: String,
    pub label: String,
    pub passed: bool,
    pub actual: String,
    pub expected: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub node_id: String,
    pub checked_at: String,
    pub score: u32,
    pub fail2ban_banned_total: Option<u64>,
    pub fail2ban_banned_current: Option<u64>,
    pub items: Vec<AuditItem>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySnapshot {
    pub node_id: String,
    pub timestamp: i64,
    pub health_score: u32,
    pub health_status: String,
    pub rtt_ms: Option<f64>,
    pub jitter_ms: f64,
    pub packet_loss_pct: f64,
    pub xui: XuiStatus,
    pub clients: Vec<ClientQuota>,
    pub local_shield: LocalShieldStatus,
    pub history: Vec<TelemetryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub tray_display: String,
    pub poll_active_secs: u64,
    pub poll_idle_secs: u64,
    pub demo_mode: bool,
    pub auto_check_updates: bool,
    pub inbox_dir: String,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    "titanium".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        let inbox = crate::storage::inbox_dir()
            .to_string_lossy()
            .to_string();
        Self {
            tray_display: "speed-rtt".to_string(),
            poll_active_secs: 2,
            poll_idle_secs: 15,
            demo_mode: false,
            auto_check_updates: true,
            inbox_dir: inbox,
            theme: default_theme(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubReleaseInfo {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub has_update: bool,
    pub release_url: String,
    pub release_notes: Option<String>,
    pub published_at: Option<String>,
    pub updater_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeakCheckResult {
    pub exit_ipv4: Option<String>,
    pub ipv6_leak: bool,
    pub matches_vps_ip: bool,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarrierSample {
    pub ts: i64,
    pub rtt_ms: Option<f64>,
    pub loss_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarrierLineMetrics {
    pub key: String,
    pub label: String,
    pub target: String,
    pub latest_rtt_ms: Option<f64>,
    pub loss_pct: f64,
    pub samples: Vec<CarrierSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarrierQualitySnapshot {
    pub node_id: String,
    pub updated_at: i64,
    pub carriers: Vec<CarrierLineMetrics>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CachedDiagnostics {
    pub ai_probes: Vec<AiProbeResult>,
    pub leak_result: Option<LeakCheckResult>,
    pub audit_report: Option<AuditReport>,
    pub carrier_quality: Option<CarrierQualitySnapshot>,
}
