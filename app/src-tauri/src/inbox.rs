use crate::models::{HandoffPayload, NodeConfig};
use crate::storage;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static INBOX_DIR_READY: AtomicBool = AtomicBool::new(false);

/// Overwrites file bytes with zeros asynchronously before deleting so credentials never linger on disk.
async fn shred_file_async(path: &Path) {
    if let Ok(meta) = tokio::fs::metadata(path).await {
        let len = meta.len() as usize;
        if len > 0 && len < 1_048_576 {
            let zeros = vec![0u8; len];
            let _ = tokio::fs::write(path, zeros).await;
        }
    }
    let _ = tokio::fs::remove_file(path).await;
}

/// Async inbox scanner for `~/.local/share/native-isp-vps/inbox/`.
/// Runs with non-blocking `tokio::fs` so Tokio workers never stall on disk I/O.
pub async fn drain_inbox_async() -> Vec<NodeConfig> {
    if !INBOX_DIR_READY.swap(true, Ordering::Relaxed) {
        let _ = storage::ensure_dirs();
    }
    let dir = storage::inbox_dir();
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
        return Vec::new();
    };

    let mut imported = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = tokio::fs::read_to_string(&path).await {
            if let Ok(payload) = serde_json::from_str::<HandoffPayload>(&raw) {
                let res = tokio::task::spawn_blocking(move || storage::upsert_from_handoff(payload))
                    .await
                    .ok()
                    .and_then(|r| r.ok());
                if let Some(node) = res {
                    shred_file_async(&path).await;
                    imported.push(node);
                }
            }
        }
    }
    imported
}
