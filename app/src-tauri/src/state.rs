use crate::carrier_probe::CarrierHistoryStore;
use crate::metrics::HistoryStore;
use crate::models::{
    AiProbeResult, AppSettings, AuditReport, CachedDiagnostics, CarrierQualitySnapshot,
    LeakCheckResult, NodeConfig, TelemetrySnapshot,
};
use crate::storage;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, EventTarget, Manager};
use tokio::sync::{Mutex as AsyncMutex, Notify, RwLock};

pub const WINDOW_LABELS: [&str; 2] = ["main", "popover"];

pub struct CockpitState {
    pub nodes: RwLock<Vec<NodeConfig>>,
    pub active_node_id: RwLock<Option<String>>,
    pub settings: RwLock<AppSettings>,
    pub history: RwLock<HistoryStore>,
    pub carrier_history: RwLock<CarrierHistoryStore>,
    pub latest_snapshot: RwLock<Option<TelemetrySnapshot>>,
    pub latest_leak: RwLock<Option<LeakCheckResult>>,
    pub latest_ai_probes: RwLock<Vec<AiProbeResult>>,
    pub latest_audit: RwLock<Option<AuditReport>>,
    pub latest_carrier_quality: RwLock<Option<CarrierQualitySnapshot>>,
    pub demo_tick: RwLock<u64>,
    pub poll_lock: AsyncMutex<()>,
    pub carrier_lock: AsyncMutex<()>,
    pub poll_wake: Arc<Notify>,
}

impl CockpitState {
    pub fn new() -> Self {
        let _ = storage::ensure_dirs();
        let nodes = storage::load_nodes();
        let active = nodes.first().map(|n| n.id.clone());
        let mut settings = storage::load_settings();
        if nodes.is_empty() {
            settings.demo_mode = true;
        }
        let carrier_history = CarrierHistoryStore::load();
        let initial_carrier = active
            .as_deref()
            .and_then(|id| carrier_history.get_cached_snapshot(id));
        Self {
            nodes: RwLock::new(nodes),
            active_node_id: RwLock::new(active),
            settings: RwLock::new(settings),
            history: RwLock::new(HistoryStore::load()),
            carrier_history: RwLock::new(carrier_history),
            latest_snapshot: RwLock::new(None),
            latest_leak: RwLock::new(None),
            latest_ai_probes: RwLock::new(Vec::new()),
            latest_audit: RwLock::new(None),
            latest_carrier_quality: RwLock::new(initial_carrier),
            demo_tick: RwLock::new(0),
            poll_lock: AsyncMutex::new(()),
            carrier_lock: AsyncMutex::new(()),
            poll_wake: Arc::new(Notify::new()),
        }
    }

    pub async fn get_cached_diagnostics(&self) -> CachedDiagnostics {
        let (ai_probes, leak_result, audit_report, carrier_quality) = tokio::join!(
            self.latest_ai_probes.read(),
            self.latest_leak.read(),
            self.latest_audit.read(),
            self.latest_carrier_quality.read()
        );
        CachedDiagnostics {
            ai_probes: ai_probes.clone(),
            leak_result: leak_result.clone(),
            audit_report: audit_report.clone(),
            carrier_quality: carrier_quality.clone(),
        }
    }

    pub async fn push_snapshot_to_window(&self, app: &AppHandle, label: &str) {
        if let Some(snap) = self.latest_snapshot.read().await.clone() {
            let _ = app.emit_to(
                EventTarget::webview_window(label),
                "cockpit://telemetry",
                &snap,
            );
        }
    }
}

/// Emits high-frequency telemetry strictly to currently visible WebView windows,
/// eliminating hidden WebView JS wakeups and wasted React diffing while collapsed to tray.
pub fn emit_to_visible_windows<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: &T) {
    for label in WINDOW_LABELS {
        if let Some(win) = app.get_webview_window(label) {
            if win.is_visible().unwrap_or(false) {
                let _ = app.emit_to(EventTarget::webview_window(label), event, payload);
            }
        }
    }
}

pub fn any_window_visible(app: &AppHandle) -> bool {
    WINDOW_LABELS.iter().any(|label| {
        app.get_webview_window(label)
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false)
    })
}
