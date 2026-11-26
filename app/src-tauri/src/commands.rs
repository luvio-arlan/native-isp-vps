use crate::demo;
use crate::local_probe;
use crate::models::{
    AiProbeResult, AppSettings, AuditReport, CachedDiagnostics, GitHubReleaseInfo, HandoffPayload,
    HistoryBucket, LeakCheckResult, NodeConfig, TelemetrySnapshot,
};
use crate::poller;
use crate::ssh_audit;
use crate::state::CockpitState;
use crate::storage;
use crate::tray;
use crate::updater;
use crate::xui;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

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
    poller::poll_once(&app, &state, false, true).await;
    Ok(())
}

#[tauri::command]
pub async fn upsert_node(
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
    poller::poll_once(&app, &state, false, true).await;
    Ok(node)
}

#[tauri::command]
pub async fn delete_node(
    node_id: String,
    app: AppHandle,
    state: State<'_, Arc<CockpitState>>,
) -> Result<Vec<NodeConfig>, String> {
    let nodes = storage::remove_node(&node_id)?;
    xui::evict_client(&node_id);
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
    Ok(store.get_24h_buckets(&id))
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
    storage::save_settings(&sanitized)?;
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
    if let Some(pop) = app.get_webview_window("popover") {
        let _ = pop.hide();
    }
}
