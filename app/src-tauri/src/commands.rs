use crate::demo;
use crate::local_probe;
use crate::models::{
    AiProbeResult, AppSettings, AuditReport, CachedDiagnostics, CarrierQualitySnapshot,
    GitHubReleaseInfo, HandoffPayload, HistoryBucket, LeakCheckResult, NodeConfig,
    TelemetrySnapshot,
};
use crate::panel_tunnel;
use crate::poller;
use crate::ssh_audit;
use crate::state::{self, CockpitState};
use crate::storage;
use crate::tray;
use crate::updater;
use crate::xui;
use chrono::Utc;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Allowed `AppSettings.tray_display` values (mirrors `SettingsTab.tsx`).
const TRAY_DISPLAY_MODES: [&str; 3] = ["speed-rtt", "rtt-only", "dot-only"];
/// Allowed `AppSettings.theme` values (mirrors `index.css` `[data-theme]` skins).
const THEMES: [&str; 2] = ["titanium", "obsidian"];

/// Writes settings off the async runtime; failures are non-fatal (in-memory state stays authoritative).
async fn persist_settings(cfg: AppSettings) {
    let _ = tokio::task::spawn_blocking(move || storage::save_settings(&cfg)).await;
}

#[tauri::command]
pub async fn list_nodes(state: State<'_, Arc<CockpitState>>) -> Result<Vec<NodeConfig>, String> {
    Ok(state.nodes.read().await.clone())
}

#[tauri::command]
pub async fn get_active_node_id(
    state: State<'_, Arc<CockpitState>>,
) -> Result<Option<String>, String> {
    Ok(state.active_node_id.read().await.clone())
}

#[tauri::command]
pub async fn set_active_node(
    node_id: String,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<(), String> {
    if !state.nodes.read().await.iter().any(|n| n.id == node_id) {
        return Err("未找到目标节点".to_string());
    }
    {
        let mut active = state.active_node_id.write().await;
        *active = Some(node_id.clone());
    }
    let updated_cfg = {
        let mut cfg = state.settings.write().await;
        if cfg.demo_mode {
            cfg.demo_mode = false;
            persist_settings(cfg.clone()).await;
            Some(cfg.clone())
        } else {
            None
        }
    };
    let cached_carrier = {
        let store = state.carrier_history.read().await;
        store.get_cached_snapshot(&node_id)
    };
    {
        let mut latest = state.latest_carrier_quality.write().await;
        *latest = cached_carrier.clone();
    }
    if let Some(snap) = cached_carrier {
        state::emit_to_visible_windows(&app, "cockpit://carrier-quality-updated", &snap);
    }
    let _ = app.emit("cockpit://active-node-updated", Some(node_id));
    if let Some(cfg) = updated_cfg {
        let _ = app.emit("cockpit://settings-updated", &cfg);
    }
    poller::poll_once(&app, &state, false, true).await;
    let bg_app = app.clone();
    let bg_state = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        let _ = poller::poll_carriers_once(&bg_app, &bg_state, true).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn upsert_node(
    payload: HandoffPayload,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<NodeConfig, String> {
    let (node, nodes) = tokio::task::spawn_blocking(move || {
        storage::upsert_from_handoff(payload).map(|node| (node, storage::load_nodes()))
    })
    .await
    .map_err(|e| format!("保存节点失败: {e}"))??;
    // SSH host/user/key may have changed: rebuild the panel tunnel on the next poll.
    panel_tunnel::forget(&node.id).await;
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
        persist_settings(cfg.clone()).await;
        cfg.clone()
    };
    let _ = app.emit("cockpit://nodes-updated", &nodes);
    let _ = app.emit("cockpit://active-node-updated", Some(node.id.clone()));
    let _ = app.emit("cockpit://settings-updated", &updated_cfg);
    poller::poll_once(&app, &state, false, true).await;
    Ok(node)
}

#[tauri::command]
pub async fn delete_node(
    node_id: String,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<Vec<NodeConfig>, String> {
    let id = node_id.clone();
    let nodes = tokio::task::spawn_blocking(move || storage::remove_node(&id))
        .await
        .map_err(|e| format!("移除节点失败: {e}"))??;
    xui::evict_client(&node_id);
    panel_tunnel::forget(&node_id).await;
    {
        let mut hist = state.history.write().await;
        hist.remove_node(&node_id);
        if let Some(buckets) = hist.take_persist_snapshot(true) {
            tokio::task::spawn_blocking(move || {
                let _ = storage::save_history_buckets(&buckets);
            });
        }
    }
    {
        let mut carrier_hist = state.carrier_history.write().await;
        carrier_hist.remove_node(&node_id);
        if let Some(map) = carrier_hist.take_persist_snapshot() {
            tokio::task::spawn_blocking(move || {
                let _ = storage::save_carrier_history(&map);
            });
        }
    }
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
        persist_settings(cfg.clone()).await;
        Some(cfg.clone())
    } else {
        None
    };
    let _ = app.emit("cockpit://nodes-updated", &nodes);
    let _ = app.emit("cockpit://active-node-updated", next_active);
    if let Some(cfg) = updated_cfg {
        let _ = app.emit("cockpit://settings-updated", &cfg);
    }
    poller::poll_once(&app, &state, false, true).await;
    Ok(nodes)
}

#[tauri::command]
pub async fn get_latest_telemetry(
    state: State<'_, Arc<CockpitState>>,
) -> Result<Option<TelemetrySnapshot>, String> {
    Ok(state.latest_snapshot.read().await.clone())
}

#[tauri::command]
pub async fn get_cached_diagnostics(
    state: State<'_, Arc<CockpitState>>,
) -> Result<CachedDiagnostics, String> {
    Ok(state.get_cached_diagnostics().await)
}

#[tauri::command]
pub async fn get_history_24h(
    node_id: Option<String>,
    state: State<'_, Arc<CockpitState>>,
) -> Result<Vec<HistoryBucket>, String> {
    let settings = state.settings.read().await.clone();
    let target_id = match node_id {
        Some(id) if !id.is_empty() => Some(id),
        _ => state.active_node_id.read().await.clone(),
    };
    if settings.demo_mode || target_id.is_none() || target_id.as_deref() == Some("demo-la") {
        return Ok(demo::build_demo_history_24h());
    }
    let id = target_id.unwrap_or_default();
    let store = state.history.read().await;
    Ok(store.get_24h_buckets(&id, Utc::now().timestamp_millis()))
}

#[tauri::command]
pub async fn trigger_refresh(
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<Option<TelemetrySnapshot>, String> {
    poller::poll_once(&app, &state, false, true).await;
    Ok(state.latest_snapshot.read().await.clone())
}

#[tauri::command]
pub async fn probe_ai_endpoints_cmd(
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<Vec<AiProbeResult>, String> {
    let results = local_probe::probe_ai_endpoints().await;
    {
        let mut cache = state.latest_ai_probes.write().await;
        *cache = results.clone();
    }
    let _ = app.emit("cockpit://ai-probes-updated", &results);
    Ok(results)
}

#[tauri::command]
pub async fn run_leak_check_cmd(
    app: AppHandle,
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
    let _ = app.emit("cockpit://leak-updated", &result);
    Ok(result)
}

#[tauri::command]
pub async fn run_ssh_audit_cmd(
    node_id: Option<String>,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<AuditReport, String> {
    let settings = state.settings.read().await.clone();
    let target = {
        let nodes = state.nodes.read().await;
        let active_id = state.active_node_id.read().await;
        let id = node_id.or_else(|| active_id.clone());
        id.and_then(|i| nodes.iter().find(|n| n.id == i).cloned())
    };

    let report = if settings.demo_mode || target.is_none() {
        demo::build_demo_audit_report()
    } else if let Some(node) = target {
        ssh_audit::run_ssh_audit(&node).await
    } else {
        return Err("未找到目标节点".to_string());
    };

    {
        let mut cache = state.latest_audit.write().await;
        *cache = Some(report.clone());
    }
    let _ = app.emit("cockpit://audit-updated", &report);
    Ok(report)
}

#[tauri::command]
pub async fn probe_carrier_quality_cmd(
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<Option<CarrierQualitySnapshot>, String> {
    Ok(poller::poll_carriers_once(&app, &state, false).await)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<CockpitState>>) -> Result<AppSettings, String> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
pub async fn update_settings(
    new_settings: AppSettings,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<AppSettings, String> {
    let mut sanitized = new_settings;
    sanitized.poll_active_secs = sanitized.poll_active_secs.clamp(1, 30);
    sanitized.poll_idle_secs = sanitized.poll_idle_secs.clamp(5, 120);
    sanitized.inbox_dir = storage::inbox_dir().to_string_lossy().to_string();
    if !TRAY_DISPLAY_MODES.contains(&sanitized.tray_display.as_str()) {
        sanitized.tray_display = "speed-rtt".to_string();
    }
    if !THEMES.contains(&sanitized.theme.as_str()) {
        sanitized.theme = "titanium".to_string();
    }
    let to_save = sanitized.clone();
    tokio::task::spawn_blocking(move || storage::save_settings(&to_save))
        .await
        .map_err(|e| format!("保存设置失败: {e}"))??;
    {
        let mut guard = state.settings.write().await;
        *guard = sanitized.clone();
    }
    let _ = app.emit("cockpit://settings-updated", &sanitized);
    state.poll_wake.notify_one();
    poller::poll_once(&app, &state, false, false).await;
    Ok(sanitized)
}

#[tauri::command]
pub async fn check_github_release_cmd() -> Result<GitHubReleaseInfo, String> {
    Ok(updater::check_github_release().await)
}

#[tauri::command]
pub fn open_main_window(app: AppHandle, state: State<'_, Arc<CockpitState>>) {
    tray::show_main_window_internal(&app, &state);
    state.poll_wake.notify_one();
}

#[tauri::command]
pub fn collapse_to_tray(app: AppHandle) {
    tray::collapse_to_tray_internal(&app);
}

#[tauri::command]
pub fn hide_popover(app: AppHandle) {
    tray::hide_popover_animated(&app);
}
