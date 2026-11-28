use crate::carrier_probe;
use crate::demo;
use crate::inbox;
use crate::local_probe;
use crate::metrics::{compute_health_score_with_streak, compute_smoothed_rtt};
use crate::mihomo;
use crate::models::{CarrierQualitySnapshot, TelemetryPoint, TelemetrySnapshot};
use crate::state::{self, CockpitState};
use crate::storage;
use crate::tray;
use crate::xui;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

pub async fn process_inbox(app: &AppHandle, state: &CockpitState) -> bool {
    let inbox::InboxDrain { imported, rejected } = inbox::drain_inbox_async().await;
    if !rejected.is_empty() {
        let _ = app
            .notification()
            .builder()
            .title("节点交接文件未能导入")
            .body(format!("已隔离为 .invalid：{}", rejected.join("、")))
            .show();
    }
    if imported.is_empty() {
        return false;
    }

    let fresh_nodes = tokio::task::spawn_blocking(storage::load_nodes)
        .await
        .unwrap_or_default();
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
            let cfg_clone = cfg.clone();
            let _ = tokio::task::spawn_blocking(move || storage::save_settings(&cfg_clone)).await;
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
    true
}

pub async fn poll_once(
    app: &AppHandle,
    state: &CockpitState,
    skip_if_busy: bool,
    force_slow: bool,
) {
    let _poll_guard = if skip_if_busy {
        match state.poll_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return,
        }
    } else {
        state.poll_lock.lock().await
    };

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
        let real_shield = mihomo::probe_local_shield(force_slow).await;
        demo::build_demo_snapshot(state, real_shield).await
    } else if let Some(node) = active_node {
        let secrets = storage::load_secrets(&node.id);
        let (rtt_ms, (xui_status, clients), mut shield) = tokio::join!(
            local_probe::probe_real_rtt(&node),
            xui::fetch_xui_telemetry(&node, &secrets, force_slow),
            mihomo::probe_local_shield(force_slow)
        );

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
        let (history_vec, jitter_ms, packet_loss_pct, health_score, health_status, persist_snap) = {
            let mut store = state.history.write().await;
            let (hist, j, loss) = store.record_real(
                &node.id,
                TelemetryPoint {
                    ts: now_ms,
                    rtt_ms,
                    vps_down_bps: xui_status.net_down_bps,
                    vps_up_bps: xui_status.net_up_bps,
                    local_down_bps: shield.local_down_bps,
                    local_up_bps: shield.local_up_bps,
                },
            );
            let xui_fail_streak = store.record_xui_connectivity(&node.id, xui_status.connected);
            let smoothed_rtt = compute_smoothed_rtt(&hist, rtt_ms);
            let (raw_score, _) = compute_health_score_with_streak(
                smoothed_rtt,
                loss,
                &xui_status,
                &shield,
                xui_fail_streak,
            );
            let fast_react = smoothed_rtt.is_none()
                || xui_fail_streak >= 2
                || (xui_status.connected && xui_status.xray_state != "running")
                || shield.ipv6_leak == Some(true);
            let (score, status) = store.smooth_health_score(&node.id, raw_score, fast_react);
            let persist = store.take_persist_snapshot(false);
            (hist, j, loss, score, status, persist)
        };

        if let Some(buckets) = persist_snap {
            tokio::task::spawn_blocking(move || {
                let _ = storage::save_history_buckets(&buckets);
            });
        }

        TelemetrySnapshot {
            node_id: node.id.clone(),
            timestamp: now_ms,
            health_score,
            health_status,
            rtt_ms,
            jitter_ms,
            packet_loss_pct,
            xui: xui_status,
            clients,
            local_shield: shield,
            history: history_vec,
        }
    } else {
        return;
    };

    // The user may have switched nodes while this tick was in flight: never publish a late
    // snapshot of the previous node under the new node's header (the forced re-poll follows).
    if !settings.demo_mode && snapshot.node_id != "demo-la" {
        let still_active = state
            .active_node_id
            .read()
            .await
            .as_deref()
            .map_or(true, |id| id == snapshot.node_id);
        if !still_active {
            return;
        }
    }

    tray::update_tray_title(app, &snapshot, &settings);

    {
        let mut latest = state.latest_snapshot.write().await;
        *latest = Some(snapshot.clone());
    }
    state::emit_to_visible_windows(app, "cockpit://telemetry", &snapshot);
}

pub async fn poll_carriers_once(
    app: &AppHandle,
    state: &CockpitState,
    skip_if_busy: bool,
) -> Option<CarrierQualitySnapshot> {
    let _guard = if skip_if_busy {
        match state.carrier_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return state.latest_carrier_quality.read().await.clone(),
        }
    } else {
        state.carrier_lock.lock().await
    };

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
        let tick = *state.demo_tick.read().await;
        carrier_probe::build_demo_carrier_quality(tick)
    } else {
        let node = active_node?;
        let now_ms = Utc::now().timestamp_millis();
        match carrier_probe::probe_node_carriers(&node).await {
            Ok(readings) => {
                let (snap, persist_map) = {
                    let mut store = state.carrier_history.write().await;
                    let snap = store.record_batch(&node.id, now_ms, &readings);
                    let persist = store.take_persist_snapshot();
                    (snap, persist)
                };
                if let Some(map) = persist_map {
                    tokio::task::spawn_blocking(move || {
                        let _ = storage::save_carrier_history(&map);
                    });
                }
                snap
            }
            Err(err) => {
                let store = state.carrier_history.read().await;
                store.build_snapshot(&node.id, now_ms, Some(err))
            }
        }
    };

    {
        let mut latest = state.latest_carrier_quality.write().await;
        *latest = Some(snapshot.clone());
    }
    state::emit_to_visible_windows(app, "cockpit://carrier-quality-updated", &snapshot);
    Some(snapshot)
}

pub fn spawn_background_loops(app_handle: AppHandle, state: Arc<CockpitState>) {
    // 1. Dedicated sub-second async inbox watcher (zero blocking I/O, <1s handoff capture even when collapsed)
    let inbox_app = app_handle.clone();
    let inbox_state = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if process_inbox(&inbox_app, &inbox_state).await {
                poll_once(&inbox_app, &inbox_state, false, true).await;
            }
        }
    });

    // 2. 1Hz live traffic stream -> real-time macOS menu bar speed with zero extra VPS polling
    let tray_app = app_handle.clone();
    let tray_state = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        let mut rx = mihomo::subscribe_live_traffic();
        while rx.changed().await.is_ok() {
            let sample = *rx.borrow();
            if let Some(s) = sample {
                let settings = tray_state.settings.read().await.clone();
                if settings.tray_display == "dot-only" || settings.tray_display == "rtt-only" {
                    continue;
                }
                let rtt_opt = {
                    let latest = tray_state.latest_snapshot.read().await;
                    let Some(snap) = latest.as_ref() else {
                        // Avoid flashing "OFF" before the initial poll completes
                        continue;
                    };
                    snap.rtt_ms
                };
                tray::update_tray_title_values(&tray_app, s.down_bps, rtt_opt, &settings);
            }
        }
    });

    // 3. Low-frequency zero-agent 3-carrier quality probe (60s visible / 300s hidden)
    let carrier_app = app_handle.clone();
    let carrier_state = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        loop {
            poll_carriers_once(&carrier_app, &carrier_state, true).await;
            let sleep_secs = if state::any_window_visible(&carrier_app) {
                60
            } else {
                300
            };
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
        }
    });

    // 4. Adaptive tiered telemetry loop (2s active / 15s collapsed)
    tauri::async_runtime::spawn(async move {
        poll_once(&app_handle, &state, false, true).await;
        loop {
            let settings = state.settings.read().await.clone();
            let sleep_secs = if state::any_window_visible(&app_handle) {
                settings.poll_active_secs
            } else {
                settings.poll_idle_secs
            };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(sleep_secs)) => {}
                _ = state.poll_wake.notified() => {}
            }
            poll_once(&app_handle, &state, true, false).await;
        }
    });
}
