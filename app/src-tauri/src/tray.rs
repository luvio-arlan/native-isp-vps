use crate::models::{AppSettings, TelemetrySnapshot};
use crate::state::CockpitState;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, EventTarget, Manager, PhysicalPosition, WindowEvent};

const POPOVER_EXIT_DURATION_MS: u64 = 215;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PopoverVisibilityPayload {
    open: bool,
    anchor_x: f64,
}

static LAST_POPOVER_BLUR: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
static LAST_TRAY_TITLE: OnceLock<Mutex<String>> = OnceLock::new();
static LAST_TRAY_MODE: OnceLock<Mutex<String>> = OnceLock::new();
static POPOVER_OPEN: AtomicBool = AtomicBool::new(false);
static POPOVER_SEQ: AtomicU64 = AtomicU64::new(0);
/// Anchor horizontal percentage in tenths (500 == 50.0%) so suction-in always targets the tray icon.
static LAST_POPOVER_ANCHOR_X_TENTHS: AtomicU32 = AtomicU32::new(500);

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
        .is_some_and(|t| t.elapsed() < Duration::from_millis(280))
}

/// Fixed 4-character compact throughput string (`0.0K`, ` 48K`, `512K`, `6.4M`, ` 24M`)
/// so the macOS menu bar title never shifts horizontally as speed fluctuates.
pub fn format_compact_bps(bps: u64) -> String {
    let gb = bps as f64 / (1024.0 * 1024.0 * 1024.0);
    if gb >= 0.995 {
        return format!("{:.1}G", gb.min(9.9));
    }
    let mb = bps as f64 / (1024.0 * 1024.0);
    if mb >= 9.95 {
        return format!("{:>3.0}M", mb.min(999.0));
    }
    if mb >= 0.995 {
        return format!("{:.1}M", mb);
    }
    let kb = bps as f64 / 1024.0;
    if kb >= 9.95 {
        return format!("{:>3.0}K", kb.min(999.0));
    }
    format!("{:.1}K", kb)
}

/// Fixed 5-character compact RTT string (` 45ms`, `213ms`, `1.2s `, `  OFF`).
pub fn format_compact_rtt(rtt_ms: Option<f64>) -> String {
    match rtt_ms {
        Some(r) if r >= 999.5 => format!("{:.1}s ", (r / 1000.0).min(9.9)),
        Some(r) => format!("{:>3.0}ms", r.max(0.0)),
        None => "  OFF".to_string(),
    }
}

#[cfg(target_os = "macos")]
fn apply_macos_tray_layout(tray: &tauri::tray::TrayIcon, display_mode: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSFont, NSFontWeightMedium, NSTextAlignment};

    let (fixed_length, alignment) = match display_mode {
        "dot-only" => (28.0_f64, NSTextAlignment::Center),
        "rtt-only" => (84.0_f64, NSTextAlignment::Left),
        _ => (142.0_f64, NSTextAlignment::Left),
    };

    let _ = tray.with_inner_tray_icon(move |inner| {
        if let (Some(mtm), Some(status_item)) = (MainThreadMarker::new(), inner.ns_status_item()) {
            status_item.setLength(fixed_length);
            if let Some(button) = status_item.button(mtm) {
                let weight = unsafe { NSFontWeightMedium };
                let mono = NSFont::monospacedSystemFontOfSize_weight(12.0, weight);
                button.setFont(Some(&mono));
                button.setAlignment(alignment);
            }
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn apply_macos_tray_layout(_tray: &tauri::tray::TrayIcon, _display_mode: &str) {}

pub fn update_tray_title_values(
    app: &AppHandle,
    down_bps: u64,
    rtt_ms: Option<f64>,
    settings: &AppSettings,
) {
    let rtt_str = format_compact_rtt(rtt_ms);
    let title = match settings.tray_display.as_str() {
        "dot-only" => String::new(),
        "rtt-only" => format!(" {}", rtt_str),
        _ => format!(" ↓{} · {}", format_compact_bps(down_bps), rtt_str),
    };

    let mode_changed = {
        let mode_slot = LAST_TRAY_MODE.get_or_init(|| Mutex::new(String::new()));
        if let Ok(mut last_mode) = mode_slot.lock() {
            if *last_mode != settings.tray_display {
                *last_mode = settings.tray_display.clone();
                true
            } else {
                false
            }
        } else {
            false
        }
    };

    // Deduplicate identical titles to avoid redundant macOS NSStatusItem redraws
    let title_changed = {
        let slot = LAST_TRAY_TITLE.get_or_init(|| Mutex::new(String::new()));
        if let Ok(mut last) = slot.lock() {
            if *last == title {
                false
            } else {
                *last = title.clone();
                true
            }
        } else {
            true
        }
    };

    if !mode_changed && !title_changed {
        return;
    }

    if let Some(tray) = app.tray_by_id("main-tray") {
        if mode_changed {
            apply_macos_tray_layout(&tray, &settings.tray_display);
        }
        if title_changed || mode_changed {
            let _ = tray.set_title(Some(&title));
        }
    }
}

pub fn update_tray_title(app: &AppHandle, snapshot: &TelemetrySnapshot, settings: &AppSettings) {
    let down = if snapshot.local_shield.mihomo_connected {
        snapshot.local_shield.local_down_bps
    } else {
        snapshot.xui.net_down_bps
    };
    update_tray_title_values(app, down, snapshot.rtt_ms, settings);
}

pub fn hide_popover_animated(app: &AppHandle) {
    let was_open = POPOVER_OPEN.swap(false, Ordering::SeqCst);
    let seq = POPOVER_SEQ.fetch_add(1, Ordering::SeqCst) + 1;

    if let Some(pop) = app.get_webview_window("popover") {
        let anchor_x = LAST_POPOVER_ANCHOR_X_TENTHS.load(Ordering::SeqCst) as f64 / 10.0;
        let _ = app.emit_to(
            EventTarget::webview_window("popover"),
            "cockpit://popover-visibility",
            PopoverVisibilityPayload {
                open: false,
                anchor_x,
            },
        );
        if !was_open && !pop.is_visible().unwrap_or(false) {
            return;
        }
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(POPOVER_EXIT_DURATION_MS)).await;
            if POPOVER_SEQ.load(Ordering::SeqCst) == seq {
                if let Some(p) = app_clone.get_webview_window("popover") {
                    let _ = p.hide();
                }
            }
        });
    }
}

pub fn show_main_window_internal(app: &AppHandle, state: &Arc<CockpitState>) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
    hide_popover_animated(app);
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        let app_clone = app.clone();
        let state_clone = Arc::clone(state);
        tauri::async_runtime::spawn(async move {
            state_clone.push_snapshot_to_window(&app_clone, "main").await;
        });
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
    state: &Arc<CockpitState>,
) {
    if let Some(pop) = app.get_webview_window("popover") {
        if POPOVER_OPEN.load(Ordering::SeqCst) {
            hide_popover_animated(app);
            return;
        }
        if was_just_blurred() {
            return;
        }

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

        let tray_center_x = tray_x + tray_w / 2.0;
        let anchor_x = (((tray_center_x - x) / pop_w) * 100.0).clamp(14.0, 86.0);
        LAST_POPOVER_ANCHOR_X_TENTHS
            .store((anchor_x * 10.0).round() as u32, Ordering::SeqCst);

        let y = (tray_y + tray_h) as i32;
        POPOVER_SEQ.fetch_add(1, Ordering::SeqCst);
        POPOVER_OPEN.store(true, Ordering::SeqCst);
        let _ = pop.set_position(PhysicalPosition::new(x as i32, y));
        let _ = pop.show();
        let _ = pop.set_focus();
        let _ = app.emit_to(
            EventTarget::webview_window("popover"),
            "cockpit://popover-visibility",
            PopoverVisibilityPayload {
                open: true,
                anchor_x,
            },
        );

        let app_clone = app.clone();
        let state_clone = Arc::clone(state);
        tauri::async_runtime::spawn(async move {
            state_clone.push_snapshot_to_window(&app_clone, "popover").await;
        });
        state.poll_wake.notify_one();
    }
}

pub fn setup_tray_and_windows(app: &App, state: Arc<CockpitState>) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "展开完整驾驶舱", true, None::<&str>)?;
    let collapse_item = MenuItem::with_id(app, "collapse", "收缩到状态栏", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 Native ISP Cockpit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &collapse_item, &quit_item])?;

    let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
        .ok()
        .or_else(|| app.default_window_icon().cloned())
        .ok_or_else(|| tauri::Error::AssetNotFound("tray-icon.png".into()))?;

    let state_for_menu = Arc::clone(&state);
    let state_for_tray = Arc::clone(&state);

    let initial_mode = state
        .settings
        .try_read()
        .map(|s| s.tray_display.clone())
        .unwrap_or_else(|_| "speed-rtt".to_string());
    let initial_title = match initial_mode.as_str() {
        "dot-only" => "",
        "rtt-only" => "  ··· ",
        _ => " ↓0.0K ·  ··· ",
    };

    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(tray_icon)
        .icon_as_template(true)
        .title(initial_title)
        .tooltip("Native ISP Cockpit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => {
                show_main_window_internal(app, &state_for_menu);
                state_for_menu.poll_wake.notify_one();
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
                    &state_for_tray,
                );
            }
        })
        .build(app)?;

    if let Ok(mut last_mode) = LAST_TRAY_MODE.get_or_init(|| Mutex::new(String::new())).lock() {
        *last_mode = initial_mode.clone();
    }
    apply_macos_tray_layout(&tray, &initial_mode);

    if let Some(main_win) = app.get_webview_window("main") {
        let handle = app.handle().clone();
        main_win.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                collapse_to_tray_internal(&handle);
            }
        });
    }

    if let Some(pop_win) = app.get_webview_window("popover") {
        let _ = pop_win.set_shadow(false);
        let handle = app.handle().clone();
        pop_win.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                record_popover_blur();
                hide_popover_animated(&handle);
            }
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{format_compact_bps, format_compact_rtt};

    #[test]
    fn formats_fixed_width_compact_bandwidth_and_rtt() {
        let speeds = [
            (0, "0.0K"),
            (512, "0.5K"),
            (2048, "2.0K"),
            (48 * 1024, " 48K"),
            (512 * 1024, "512K"),
            (6 * 1024 * 1024, "6.0M"),
            (24 * 1024 * 1024, " 24M"),
        ];
        for (bps, expected) in speeds {
            let formatted = format_compact_bps(bps);
            assert_eq!(formatted, expected);
            assert_eq!(formatted.chars().count(), 4);
        }

        let rtts = [
            (Some(45.0), " 45ms"),
            (Some(213.4), "213ms"),
            (Some(1200.0), "1.2s "),
            (None, "  OFF"),
        ];
        for (rtt, expected) in rtts {
            let formatted = format_compact_rtt(rtt);
            assert_eq!(formatted, expected);
            assert_eq!(formatted.chars().count(), 5);
        }
    }
}
