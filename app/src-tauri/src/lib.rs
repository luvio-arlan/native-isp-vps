mod inbox;
mod local_probe;
mod models;
mod ssh_audit;
mod storage;
mod tray;
mod updater;
mod xui;

use chrono::Utc;
use models::{
    AiProbeResult, AppSettings, AuditReport, ClientQuota, GitHubReleaseInfo, HandoffPayload,
    LeakCheckResult, LocalShieldStatus, NodeConfig, TelemetryPoint, TelemetrySnapshot, XuiStatus,
};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::{Mutex as AsyncMutex, Notify, RwLock};

const MAX_HISTORY: usize = 60;

struct CockpitState {
    nodes: RwLock<Vec<NodeConfig>>,
    active_node_id: RwLock<Option<String>>,
    settings: RwLock<AppSettings>,
    history: RwLock<HashMap<String, VecDeque<TelemetryPoint>>>,
    latest_snapshot: RwLock<Option<TelemetrySnapshot>>,
    latest_leak: RwLock<Option<LeakCheckResult>>,
    demo_tick: RwLock<u64>,
    poll_lock: AsyncMutex<()>,
    poll_wake: Arc<Notify>,
}

impl CockpitState {
    fn new() -> Self {
        let _ = storage::ensure_dirs();
        let _ = inbox::drain_inbox();
        let nodes = storage::load_nodes();
        let active = nodes.first().map(|n| n.id.clone());
        let mut settings = storage::load_settings();
        if nodes.is_empty() {
            settings.demo_mode = true;
        }
        Self {
            nodes: RwLock::new(nodes),
            active_node_id: RwLock::new(active),
            settings: RwLock::new(settings),
            history: RwLock::new(HashMap::new()),
            latest_snapshot: RwLock::new(None),
            latest_leak: RwLock::new(None),
            demo_tick: RwLock::new(0),
            poll_lock: AsyncMutex::new(()),
            poll_wake: Arc::new(Notify::new()),
        }
    }
}

fn compute_health_score(
    rtt_ms: Option<f64>,
    loss_pct: f64,
    xui: &XuiStatus,
    shield: &LocalShieldStatus,
) -> (u32, String) {
    let mut score: i32 = 100;

    if rtt_ms.is_none() {
        score -= 40;
    } else if let Some(r) = rtt_ms {
        if r > 280.0 {
            score -= 18;
        } else if r > 200.0 {
            score -= 8;
        }
    }
    if loss_pct > 5.0 {
        score -= 20;
    } else if loss_pct > 0.0 {
        score -= 6;
    }

    if !xui.connected {
        score -= 18;
    } else {
        if xui.xray_state != "running" {
            score -= 25;
        }
        if xui.cpu_percent > 85.0 {
            score -= 10;
        }
        if xui.mem_total > 0 && (xui.mem_used as f64 / xui.mem_total as f64) > 0.90 {
            score -= 10;
        }
    }

    if shield.ipv6_leak == Some(true) {
        score -= 15;
    }
    if shield.mihomo_connected && shield.quic_blocked == Some(false) {
        score -= 5;
    }

    let clamped = score.clamp(0, 100) as u32;
    let status = if clamped >= 85 {
        "optimal"
    } else if clamped >= 60 {
        "degraded"
    } else {
        "critical"
    };
    (clamped, status.to_string())
}

fn compute_rtt_metrics(history: &VecDeque<TelemetryPoint>) -> (f64, f64) {
    if history.is_empty() {
        return (0.0, 0.0);
    }
    let total = history.len() as f64;
    let mut missing = 0.0;
    let mut rtts: Vec<f64> = Vec::with_capacity(history.len());
    for pt in history {
        match pt.rtt_ms {
            Some(v) => rtts.push(v),
            None => missing += 1.0,
        }
    }
    let loss_pct = ((missing / total) * 100.0 * 10.0).round() / 10.0;
    let jitter = if rtts.len() >= 2 {
        let mut diffs = 0.0;
        for w in rtts.windows(2) {
            diffs += (w[1] - w[0]).abs();
        }
        ((diffs / (rtts.len() - 1) as f64) * 10.0).round() / 10.0
    } else {
        0.0
    };
    (jitter, loss_pct)
}

async fn build_demo_snapshot(
    state: &CockpitState,
    real_shield: LocalShieldStatus,
) -> TelemetrySnapshot {
    let tick = {
        let mut t = state.demo_tick.write().await;
        *t += 1;
        *t
    };

    let phase = tick as f64 * 0.45;
    let rtt = (134.0 + phase.sin() * 7.5 + (phase * 1.7).cos() * 3.2).round();
    let vps_down = (4_200_000.0 + (phase * 0.8).sin() * 2_400_000.0 + (phase * 2.1).cos() * 900_000.0)
        .max(240_000.0) as u64;
    let vps_up = (780_000.0 + (phase * 0.6).cos() * 360_000.0).max(65_000.0) as u64;

    let local_down = if real_shield.mihomo_connected {
        real_shield.local_down_bps
    } else {
        (vps_down as f64 * 0.88) as u64
    };
    let local_up = if real_shield.mihomo_connected {
        real_shield.local_up_bps
    } else {
        (vps_up as f64 * 0.86) as u64
    };

    let now_ms = Utc::now().timestamp_millis();
    let history_vec = {
        let mut map = state.history.write().await;
        let ring = map.entry("demo-la".to_string()).or_insert_with(|| {
            let mut init = VecDeque::with_capacity(MAX_HISTORY);
            for i in (1..=36).rev() {
                let p = (36 - i) as f64 * 0.45;
                let d = (3_800_000.0 + (p * 0.8).sin() * 2_200_000.0).max(200_000.0) as u64;
                let u = (700_000.0 + (p * 0.6).cos() * 320_000.0).max(50_000.0) as u64;
                init.push_back(TelemetryPoint {
                    ts: now_ms - (i as i64) * 2000,
                    rtt_ms: Some((134.0 + p.sin() * 6.5).round()),
                    vps_down_bps: d,
                    vps_up_bps: u,
                    local_down_bps: (d as f64 * 0.9) as u64,
                    local_up_bps: (u as f64 * 0.85) as u64,
                });
            }
            init
        });
        if ring.len() >= MAX_HISTORY {
            ring.pop_front();
        }
        ring.push_back(TelemetryPoint {
            ts: now_ms,
            rtt_ms: Some(rtt),
            vps_down_bps: vps_down,
            vps_up_bps: vps_up,
            local_down_bps: local_down,
            local_up_bps: local_up,
        });
        ring.iter().cloned().collect::<Vec<_>>()
    };

    let mut shield = real_shield;
    if !shield.mihomo_connected {
        shield.mode = "rule".to_string();
        shield.local_down_bps = local_down;
        shield.local_up_bps = local_up;
        shield.local_up_total = 1_240_000_000;
        shield.local_down_total = 6_820_000_000;
        shield.active_connections = 34;
        shield.proxy_connections = 21;
        shield.direct_connections = 13;
        shield.ipv6_disabled = Some(true);
        shield.tun_enabled = Some(true);
        shield.tun_stack = Some("mixed".to_string());
        shield.fake_ip_enabled = Some(true);
        shield.quic_blocked = Some(true);
        shield.tcp_concurrent = Some(true);
        shield.exit_ipv4 = Some("38.55.142.88".to_string());
        shield.ipv6_leak = Some(false);
        shield.matches_vps_ip = Some(true);
    }

    let xui = XuiStatus {
        connected: true,
        error: None,
        cpu_percent: (9.4 + (phase * 0.5).sin() * 3.8).max(2.1),
        mem_used: 412 * 1024 * 1024,
        mem_total: 1024 * 1024 * 1024,
        swap_used: 68 * 1024 * 1024,
        swap_total: 1024 * 1024 * 1024,
        disk_used: 3_480_000_000,
        disk_total: 20_000_000_000,
        uptime_secs: 18 * 86400 + 7 * 3600 + 42 * 60 + tick * 2,
        loads: [0.14, 0.19, 0.16],
        xray_state: "running".to_string(),
        xray_version: "25.3.6".to_string(),
        net_up_bps: vps_up,
        net_down_bps: vps_down,
        net_sent_total: 198_420_000_000,
        net_recv_total: 184_110_000_000,
        tcp_count: 46,
        udp_count: 6,
    };

    let clients = vec![
        ClientQuota {
            email: "Me".to_string(),
            enable: true,
            up_bytes: 28_600_000_000,
            down_bytes: 136_400_000_000,
            total_bytes: 500 * 1024 * 1024 * 1024,
            reset_day: 25,
            online: true,
            sub_id: Some("demo7a9f2c8e1b4d".to_string()),
            sub_url: Some("http://38.55.142.88:2096/sub/demo7a9f2c8e1b4d".to_string()),
        },
        ClientQuota {
            email: "GF".to_string(),
            enable: true,
            up_bytes: 7_200_000_000,
            down_bytes: 49_800_000_000,
            total_bytes: 500 * 1024 * 1024 * 1024,
            reset_day: 25,
            online: true,
            sub_id: Some("demo4f1c9a2e7d0b".to_string()),
            sub_url: Some("http://38.55.142.88:2096/sub/demo4f1c9a2e7d0b".to_string()),
        },
    ];

    TelemetrySnapshot {
        node_id: "demo-la".to_string(),
        timestamp: now_ms,
        health_score: 98,
        health_status: "demo".to_string(),
        rtt_ms: Some(rtt),
        jitter_ms: 3.4,
        packet_loss_pct: 0.0,
        xui,
        clients,
        local_shield: shield,
        history: history_vec,
    }
}

async fn poll_once(app: &AppHandle, state: &CockpitState, skip_if_busy: bool) {
    let _poll_guard = if skip_if_busy {
        match state.poll_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return,
        }
    } else {
        state.poll_lock.lock().await
    };

    // 1. Drain AI handoff inbox
    let imported = inbox::drain_inbox();
    if !imported.is_empty() {
        let fresh_nodes = storage::load_nodes();
        let newest = imported.last().cloned();
        {
            let mut nodes_guard = state.nodes.write().await;
            *nodes_guard = fresh_nodes.clone();
        }
        if let Some(ref node) = newest {
            {
                let mut active = state.active_node_id.write().await;
                *active = Some(node.id.clone());
            }
            let updated_cfg = {
                let mut cfg = state.settings.write().await;
                cfg.demo_mode = false;
                let _ = storage::save_settings(&cfg);
                cfg.clone()
            };
            let _ = app.emit("cockpit://nodes-updated", &fresh_nodes);
            let _ = app.emit("cockpit://active-node-updated", Some(node.id.clone()));
            let _ = app.emit("cockpit://settings-updated", &updated_cfg);
            let _ = app.emit("cockpit://node-imported", node);
            let _ = app
                .notification()
                .builder()
                .title("VPS 节点已自动接入看板")
                .body(format!("{} ({}) 已完成本地安全纳管", node.name, node.host))
                .show();
        }
    }

    let settings = state.settings.read().await.clone();
    let active_node = {
        let nodes = state.nodes.read().await;
        let active_id = state.active_node_id.read().await;
        match active_id.as_deref() {
            Some(id) => nodes.iter().find(|n| n.id == id).cloned(),
            None => nodes.first().cloned(),
        }
    };

    let snapshot = if settings.demo_mode || active_node.is_none() {
        let real_shield = local_probe::probe_local_shield(None).await;
        build_demo_snapshot(state, real_shield).await
    } else if let Some(node) = active_node {
        let secrets = storage::load_secrets(&node.id);
        let (rtt_ms, (xui, clients), mut shield) = tokio::join!(
            local_probe::probe_real_rtt(&node.host, node.vless_port, node.ssh_port, &node.panel_url),
            xui::fetch_xui_telemetry(&node, &secrets),
            local_probe::probe_local_shield(Some(&node.host))
        );

        // Attach cached leak check if available, recomputing matches_vps_ip for the active node
        if let Some(leak) = state.latest_leak.read().await.clone() {
            let matches_current = leak
                .exit_ipv4
                .as_deref()
                .is_some_and(|exit| exit == node.host);
            shield.exit_ipv4 = leak.exit_ipv4;
            shield.ipv6_leak = Some(leak.ipv6_leak);
            shield.matches_vps_ip = Some(matches_current);
        }

        let now_ms = Utc::now().timestamp_millis();
        let (history_vec, jitter_ms, packet_loss_pct) = {
            let mut map = state.history.write().await;
            let ring = map
                .entry(node.id.clone())
                .or_insert_with(|| VecDeque::with_capacity(MAX_HISTORY));
            if ring.len() >= MAX_HISTORY {
                ring.pop_front();
            }
            ring.push_back(TelemetryPoint {
                ts: now_ms,
                rtt_ms,
                vps_down_bps: xui.net_down_bps,
                vps_up_bps: xui.net_up_bps,
                local_down_bps: shield.local_down_bps,
                local_up_bps: shield.local_up_bps,
            });
            let (j, loss) = compute_rtt_metrics(ring);
            (ring.iter().cloned().collect::<Vec<_>>(), j, loss)
        };

        let (health_score, health_status) =
            compute_health_score(rtt_ms, packet_loss_pct, &xui, &shield);

        TelemetrySnapshot {
            node_id: node.id.clone(),
            timestamp: now_ms,
            health_score,
            health_status,
            rtt_ms,
            jitter_ms,
            packet_loss_pct,
            xui,
            clients,
            local_shield: shield,
            history: history_vec,
        }
    } else {
        return;
    };

    tray::update_tray_title(app, &snapshot, &settings);

    {
        let mut latest = state.latest_snapshot.write().await;
        *latest = Some(snapshot.clone());
    }
    let _ = app.emit("cockpit://telemetry", &snapshot);
}

// ========================== TAURI COMMANDS ==========================

#[tauri::command]
async fn list_nodes(state: State<'_, Arc<CockpitState>>) -> Result<Vec<NodeConfig>, String> {
    Ok(state.nodes.read().await.clone())
}

#[tauri::command]
async fn get_active_node_id(state: State<'_, Arc<CockpitState>>) -> Result<Option<String>, String> {
    Ok(state.active_node_id.read().await.clone())
}

#[tauri::command]
async fn set_active_node(
    node_id: String,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<(), String> {
    {
        let mut active = state.active_node_id.write().await;
        *active = Some(node_id.clone());
    }
    let updated_cfg = {
        let mut cfg = state.settings.write().await;
        if cfg.demo_mode {
            cfg.demo_mode = false;
            let _ = storage::save_settings(&cfg);
            Some(cfg.clone())
        } else {
            None
        }
    };
    let _ = app.emit("cockpit://active-node-updated", Some(node_id));
    if let Some(cfg) = updated_cfg {
        let _ = app.emit("cockpit://settings-updated", &cfg);
    }
    poll_once(&app, &state, false).await;
    Ok(())
}

#[tauri::command]
async fn upsert_node(
    payload: HandoffPayload,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<NodeConfig, String> {
    let node = storage::upsert_from_handoff(payload)?;
    let nodes = storage::load_nodes();
    {
        let mut guard = state.nodes.write().await;
        *guard = nodes.clone();
    }
    {
        let mut active = state.active_node_id.write().await;
        *active = Some(node.id.clone());
    }
    let updated_cfg = {
        let mut cfg = state.settings.write().await;
        cfg.demo_mode = false;
        let _ = storage::save_settings(&cfg);
        cfg.clone()
    };
    let _ = app.emit("cockpit://nodes-updated", &nodes);
    let _ = app.emit("cockpit://active-node-updated", Some(node.id.clone()));
    let _ = app.emit("cockpit://settings-updated", &updated_cfg);
    poll_once(&app, &state, false).await;
    Ok(node)
}

#[tauri::command]
async fn delete_node(
    node_id: String,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<Vec<NodeConfig>, String> {
    let nodes = storage::remove_node(&node_id)?;
    xui::evict_client(&node_id);
    {
        let mut guard = state.nodes.write().await;
        *guard = nodes.clone();
    }
    let next_active = {
        let mut active = state.active_node_id.write().await;
        if active.as_deref() == Some(node_id.as_str()) {
            *active = nodes.first().map(|n| n.id.clone());
        }
        active.clone()
    };
    let updated_cfg = if nodes.is_empty() {
        let mut cfg = state.settings.write().await;
        cfg.demo_mode = true;
        let _ = storage::save_settings(&cfg);
        Some(cfg.clone())
    } else {
        None
    };
    let _ = app.emit("cockpit://nodes-updated", &nodes);
    let _ = app.emit("cockpit://active-node-updated", next_active);
    if let Some(cfg) = updated_cfg {
        let _ = app.emit("cockpit://settings-updated", &cfg);
    }
    poll_once(&app, &state, false).await;
    Ok(nodes)
}

#[tauri::command]
async fn get_latest_telemetry(
    state: State<'_, Arc<CockpitState>>,
) -> Result<Option<TelemetrySnapshot>, String> {
    Ok(state.latest_snapshot.read().await.clone())
}

#[tauri::command]
async fn trigger_refresh(
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<Option<TelemetrySnapshot>, String> {
    poll_once(&app, &state, false).await;
    Ok(state.latest_snapshot.read().await.clone())
}

#[tauri::command]
async fn probe_ai_endpoints_cmd() -> Result<Vec<AiProbeResult>, String> {
    Ok(local_probe::probe_ai_endpoints().await)
}

#[tauri::command]
async fn run_leak_check_cmd(
    state: State<'_, Arc<CockpitState>>,
) -> Result<LeakCheckResult, String> {
    let host = {
        let nodes = state.nodes.read().await;
        let active_id = state.active_node_id.read().await;
        active_id
            .as_deref()
            .and_then(|id| nodes.iter().find(|n| n.id == id))
            .map(|n| n.host.clone())
    };
    let result = local_probe::run_leak_check(host.as_deref()).await;
    {
        let mut cache = state.latest_leak.write().await;
        *cache = Some(result.clone());
    }
    Ok(result)
}

#[tauri::command]
async fn run_ssh_audit_cmd(
    node_id: Option<String>,
    state: State<'_, Arc<CockpitState>>,
) -> Result<AuditReport, String> {
    let settings = state.settings.read().await.clone();
    let target = {
        let nodes = state.nodes.read().await;
        let active_id = state.active_node_id.read().await;
        let id = node_id.or_else(|| active_id.clone());
        id.and_then(|i| nodes.iter().find(|n| n.id == i).cloned())
    };

    if settings.demo_mode || target.is_none() {
        return Ok(AuditReport {
            node_id: "demo-la".into(),
            checked_at: Utc::now().to_rfc3339(),
            score: 100,
            fail2ban_banned_total: Some(148),
            fail2ban_banned_current: Some(2),
            items: vec![
                models::AuditItem {
                    key: "qdisc".into(),
                    label: "网卡队列 FQ (Fair Queue)".into(),
                    passed: true,
                    actual: "fq".into(),
                    expected: "fq".into(),
                    severity: "critical".into(),
                },
                models::AuditItem {
                    key: "bbr".into(),
                    label: "TCP 拥塞控制算法".into(),
                    passed: true,
                    actual: "bbr".into(),
                    expected: "bbr".into(),
                    severity: "critical".into(),
                },
                models::AuditItem {
                    key: "tfo".into(),
                    label: "TCP Fast Open (防 SYN 黑洞)".into(),
                    passed: true,
                    actual: "0".into(),
                    expected: "0 (禁用)".into(),
                    severity: "warn".into(),
                },
                models::AuditItem {
                    key: "ecn".into(),
                    label: "TCP ECN (防运营商丢包)".into(),
                    passed: true,
                    actual: "0".into(),
                    expected: "0 (禁用)".into(),
                    severity: "warn".into(),
                },
                models::AuditItem {
                    key: "keepalive".into(),
                    label: "TCP Keepalive 保活周期".into(),
                    passed: true,
                    actual: "120s".into(),
                    expected: "120s".into(),
                    severity: "warn".into(),
                },
                models::AuditItem {
                    key: "xui".into(),
                    label: "3x-ui / Xray 守护进程".into(),
                    passed: true,
                    actual: "active".into(),
                    expected: "active".into(),
                    severity: "critical".into(),
                },
                models::AuditItem {
                    key: "ufw".into(),
                    label: "UFW 最小暴露防火墙".into(),
                    passed: true,
                    actual: "Status: active".into(),
                    expected: "Status: active".into(),
                    severity: "critical".into(),
                },
                models::AuditItem {
                    key: "swap".into(),
                    label: "小内存 Swap 防 OOM 保护".into(),
                    passed: true,
                    actual: "已启用 (1GB)".into(),
                    expected: ">= 1GB Swap".into(),
                    severity: "info".into(),
                },
                models::AuditItem {
                    key: "fail2ban".into(),
                    label: "Fail2ban SSH 防爆破".into(),
                    passed: true,
                    actual: "运行中（当前封禁 2 / 累计 148）".into(),
                    expected: "sshd jail active".into(),
                    severity: "warn".into(),
                },
            ],
            error: None,
        });
    }

    let Some(node) = target else {
        return Err("未找到目标节点".to_string());
    };
    Ok(ssh_audit::run_ssh_audit(&node).await)
}

#[tauri::command]
async fn get_settings(state: State<'_, Arc<CockpitState>>) -> Result<AppSettings, String> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
async fn update_settings(
    new_settings: AppSettings,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<AppSettings, String> {
    let mut sanitized = new_settings;
    sanitized.poll_active_secs = sanitized.poll_active_secs.clamp(1, 30);
    sanitized.poll_idle_secs = sanitized.poll_idle_secs.clamp(5, 120);
    sanitized.inbox_dir = storage::inbox_dir().to_string_lossy().to_string();
    storage::save_settings(&sanitized)?;
    {
        let mut guard = state.settings.write().await;
        *guard = sanitized.clone();
    }
    let _ = app.emit("cockpit://settings-updated", &sanitized);
    state.poll_wake.notify_one();
    poll_once(&app, &state, false).await;
    Ok(sanitized)
}

#[tauri::command]
async fn check_github_release_cmd() -> Result<GitHubReleaseInfo, String> {
    Ok(updater::check_github_release().await)
}

#[tauri::command]
fn open_main_window(app: AppHandle, state: State<'_, Arc<CockpitState>>) {
    tray::show_main_window_internal(&app);
    state.poll_wake.notify_one();
}

#[tauri::command]
fn collapse_to_tray(app: AppHandle) {
    tray::collapse_to_tray_internal(&app);
}

#[tauri::command]
fn hide_popover(app: AppHandle) {
    if let Some(pop) = app.get_webview_window("popover") {
        let _ = pop.hide();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shared_state = Arc::new(CockpitState::new());
    let bg_state = Arc::clone(&shared_state);
    let tray_wake = Arc::clone(&shared_state.poll_wake);

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(shared_state)
        .setup(move |app| {
            tray::setup_tray_and_windows(app, tray_wake)?;

            // Spawn adaptive background telemetry & inbox watcher loop
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                poll_once(&app_handle, &bg_state, false).await;
                loop {
                    let settings = bg_state.settings.read().await.clone();
                    let main_vis = app_handle
                        .get_webview_window("main")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    let pop_vis = app_handle
                        .get_webview_window("popover")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    let sleep_secs = if main_vis || pop_vis {
                        settings.poll_active_secs
                    } else {
                        settings.poll_idle_secs
                    };
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)) => {}
                        _ = bg_state.poll_wake.notified() => {}
                    }
                    poll_once(&app_handle, &bg_state, true).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_nodes,
            get_active_node_id,
            set_active_node,
            upsert_node,
            delete_node,
            get_latest_telemetry,
            trigger_refresh,
            probe_ai_endpoints_cmd,
            run_leak_check_cmd,
            run_ssh_audit_cmd,
            get_settings,
            update_settings,
            check_github_release_cmd,
            open_main_window,
            collapse_to_tray,
            hide_popover
        ])
        .build(tauri::generate_context!())
        .expect("error while building Native ISP Cockpit")
        .run(|app_handle, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::show_main_window_internal(app_handle);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app_handle, event);
        });
}
