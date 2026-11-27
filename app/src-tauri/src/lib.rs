mod carrier_probe;
mod commands;
mod demo;
mod inbox;
mod local_probe;
mod metrics;
mod mihomo;
mod models;
mod poller;
mod ssh_audit;
mod state;
mod storage;
mod tray;
mod updater;
mod xui;

use state::CockpitState;
use std::sync::Arc;

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
            tauri::RunEvent::Exit => {
                if let Ok(mut hist) = event_state.history.try_write() {
                    if let Some(buckets) = hist.take_persist_snapshot(true) {
                        let _ = storage::save_history_buckets(&buckets);
                    }
                }
                if let Ok(mut carrier_hist) = event_state.carrier_history.try_write() {
                    if let Some(map) = carrier_hist.take_persist_snapshot() {
                        let _ = storage::save_carrier_history(&map);
                    }
                }
            }
            _ => {
                let _ = app_handle;
            }
        });
}
