use crate::models::{AppSettings, TelemetrySnapshot};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, PhysicalPosition, WindowEvent};
use tokio::sync::Notify;

static LAST_POPOVER_BLUR: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

fn last_blur_slot() -> &'static Mutex<Option<Instant>> {
    LAST_POPOVER_BLUR.get_or_init(|| Mutex::new(None))
}

pub fn record_popover_blur() {
    if let Ok(mut guard) = last_blur_slot().lock() {
        *guard = Some(Instant::now());
    }
}

fn was_just_blurred() -> bool {
    last_blur_slot()
        .lock()
        .ok()
        .and_then(|g| *g)
        .is_some_and(|t| t.elapsed() < Duration::from_millis(240))
}

pub fn format_compact_bps(bps: u64) -> String {
    let mb = bps as f64 / (1024.0 * 1024.0);
    if mb >= 1.0 {
        return format!("{:.1}M", mb);
    }
    let kb = bps as f64 / 1024.0;
    if kb >= 1.0 {
        return format!("{:.0}K", kb);
    }
    format!("{}B", bps)
}

pub fn update_tray_title(app: &AppHandle, snapshot: &TelemetrySnapshot, settings: &AppSettings) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let down = if snapshot.local_shield.mihomo_connected {
            snapshot.local_shield.local_down_bps
        } else {
            snapshot.xui.net_down_bps
        };
        let rtt_str = snapshot
            .rtt_ms
            .map(|r| format!("{:.0}ms", r))
            .unwrap_or_else(|| "OFF".to_string());
        let title = match settings.tray_display.as_str() {
            "dot-only" => String::new(),
            "rtt-only" => format!(" {}", rtt_str),
            _ => format!(" ↓{} · {}", format_compact_bps(down), rtt_str),
        };
        let _ = tray.set_title(Some(&title));
    }
}

pub fn show_main_window_internal(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
    if let Some(pop) = app.get_webview_window("popover") {
        let _ = pop.hide();
    }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

pub fn collapse_to_tray_internal(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    }
}

pub fn toggle_popover_at(
    app: &AppHandle,
    tray_x: f64,
    tray_y: f64,
    tray_w: f64,
    tray_h: f64,
    poll_wake: &Arc<Notify>,
) {
    if let Some(pop) = app.get_webview_window("popover") {
        if pop.is_visible().unwrap_or(false) {
            let _ = pop.hide();
            return;
        }
        // Prevent macOS tray click-to-close reopen race (blur hides popover right before Click Up)
        if was_just_blurred() {
            return;
        }

        // Find the monitor containing the clicked tray icon for accurate multi-monitor clamping
        let target_monitor = app.available_monitors().ok().and_then(|monitors| {
            monitors.into_iter().find(|m| {
                let pos = m.position();
                let size = m.size();
                let x_ok = tray_x >= pos.x as f64 && tray_x < (pos.x + size.width as i32) as f64;
                let y_ok = tray_y >= pos.y as f64 && tray_y < (pos.y + size.height as i32) as f64;
                x_ok && y_ok
            })
        });

        let scale = target_monitor
            .as_ref()
            .map(|m| m.scale_factor())
            .or_else(|| pop.scale_factor().ok())
            .unwrap_or(2.0);
        let pop_w = 420.0 * scale;
        let mut x = tray_x + tray_w / 2.0 - pop_w / 2.0;

        if let Some(monitor) = target_monitor.or_else(|| pop.current_monitor().ok().flatten()) {
            let m_pos = monitor.position();
            let m_size = monitor.size();
            let min_x = m_pos.x as f64;
            let max_x = (m_pos.x + m_size.width as i32) as f64 - pop_w;
            if max_x >= min_x {
                x = x.clamp(min_x, max_x);
            }
        } else {
            x = x.max(0.0);
        }

        let y = (tray_y + tray_h) as i32;
        let _ = pop.set_position(PhysicalPosition::new(x as i32, y));
        let _ = pop.show();
        let _ = pop.set_focus();
        poll_wake.notify_one();
    }
}

pub fn setup_tray_and_windows(app: &App, poll_wake: Arc<Notify>) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "展开完整驾驶舱", true, None::<&str>)?;
    let collapse_item = MenuItem::with_id(app, "collapse", "收缩到状态栏", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 Native ISP Cockpit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &collapse_item, &quit_item])?;

    let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
        .ok()
        .or_else(|| app.default_window_icon().cloned())
        .ok_or_else(|| tauri::Error::AssetNotFound("tray-icon.png".into()))?;

    let wake_for_menu = Arc::clone(&poll_wake);
    let wake_for_tray = Arc::clone(&poll_wake);

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(tray_icon)
        .icon_as_template(true)
        .title(" ···")
        .tooltip("Native ISP Cockpit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => {
                show_main_window_internal(app);
                wake_for_menu.notify_one();
            }
            "collapse" => collapse_to_tray_internal(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                let scale = tray
                    .app_handle()
                    .get_webview_window("popover")
                    .and_then(|w| w.scale_factor().ok())
                    .unwrap_or(2.0);
                let pos = rect.position.to_physical::<f64>(scale);
                let size = rect.size.to_physical::<f64>(scale);
                toggle_popover_at(
                    tray.app_handle(),
                    pos.x,
                    pos.y,
                    size.width,
                    size.height,
                    &wake_for_tray,
                );
            }
        })
        .build(app)?;

    // Main window close -> collapse cleanly to menu bar instead of quitting
    if let Some(main_win) = app.get_webview_window("main") {
        let handle = app.handle().clone();
        main_win.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                collapse_to_tray_internal(&handle);
            }
        });
    }

    // Popover auto-hide when clicking outside + disable native rectangular window shadow
    if let Some(pop_win) = app.get_webview_window("popover") {
        let _ = pop_win.set_shadow(false);
        let pop_clone = pop_win.clone();
        pop_win.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                record_popover_blur();
                let _ = pop_clone.hide();
            }
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::format_compact_bps;

    #[test]
    fn formats_compact_bandwidth() {
        assert_eq!(format_compact_bps(512), "512B");
        assert_eq!(format_compact_bps(2048), "2K");
        assert_eq!(format_compact_bps(6 * 1024 * 1024), "6.0M");
    }
}
