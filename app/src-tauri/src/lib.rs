mod carrier_probe;
mod commands;
mod demo;
mod inbox;
mod local_probe;
mod metrics;
mod mihomo;
mod models;
mod panel_tunnel;
mod poller;
mod ssh;
mod ssh_audit;
mod state;
mod storage;
mod tray;
mod updater;
mod validate;
mod xui;

use state::CockpitState;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, RwLockWriteGuard};

/// Exit-path lock acquisition without `blocking_write` (which panics inside a runtime context):
/// retries `try_write` for up to ~400ms so an in-flight poll tick can release the lock first.
fn write_on_exit<T>(lock: &RwLock<T>) -> Option<RwLockWriteGuard<'_, T>> {
    for _ in 0..40 {
        if let Ok(guard) = lock.try_write() {
            return Some(guard);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

fn persist_on_exit(state: &CockpitState) {
    if let Some(mut hist) = write_on_exit(&state.history) {
        hist.flush_partial_minutes();
        if let Some(buckets) = hist.take_persist_snapshot(true) {
            let _ = storage::save_history_buckets(&buckets);
        }
    }
    if let Some(mut carrier_hist) = write_on_exit(&state.carrier_history) {
        if let Some(map) = carrier_hist.take_persist_snapshot() {
            let _ = storage::save_carrier_history(&map);
        }
    }
    panel_tunnel::shutdown_all();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shared_state = Arc::new(CockpitState::new());
    let bg_state = Arc::clone(&shared_state);
    let tray_state = Arc::clone(&shared_state);
    let event_state = Arc::clone(&shared_state);

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(shared_state)
        .setup(move |app| {
            tray::setup_tray_and_windows(app, tray_state)?;
            poller::spawn_background_loops(app.handle().clone(), bg_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_nodes,
            commands::get_active_node_id,
            commands::set_active_node,
            commands::upsert_node,
            commands::delete_node,
            commands::get_latest_telemetry,
            commands::get_cached_diagnostics,
            commands::get_history_24h,
            commands::trigger_refresh,
            commands::probe_ai_endpoints_cmd,
            commands::probe_carrier_quality_cmd,
            commands::run_leak_check_cmd,
            commands::run_ssh_audit_cmd,
            commands::get_settings,
            commands::update_settings,
            commands::check_github_release_cmd,
            commands::open_main_window,
            commands::collapse_to_tray,
            commands::hide_popover
        ])
        .build(tauri::generate_context!())
        .expect("error while building Native ISP Cockpit")
        .run(move |app_handle, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                tray::show_main_window_internal(app_handle, &event_state);
            }
            tauri::RunEvent::Exit => persist_on_exit(&event_state),
            _ => {
                let _ = app_handle;
            }
        });
}
