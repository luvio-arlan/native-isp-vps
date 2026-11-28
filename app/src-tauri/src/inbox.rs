use crate::models::{HandoffPayload, NodeConfig};
use crate::storage;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static INBOX_DIR_READY: AtomicBool = AtomicBool::new(false);

/// Handoff manifests are tiny; anything bigger is rejected without being parsed.
const MAX_HANDOFF_BYTES: u64 = 64 * 1024;
/// Grace period for writers that do not stage atomically before an unparsable file is quarantined.
const PARSE_GRACE: Duration = Duration::from_secs(20);

#[derive(Default)]
pub struct InboxDrain {
    pub imported: Vec<NodeConfig>,
    /// File names quarantined as `*.invalid` (kept 0600 for the user to inspect or delete).
    pub rejected: Vec<String>,
}

/// Overwrites file bytes with zeros before deleting so credentials do not linger in the inbox.
/// (Best effort: copy-on-write filesystems such as APFS may retain old blocks until reuse.)
async fn shred_file_async(path: &Path) {
    if let Ok(meta) = tokio::fs::metadata(path).await {
        let len = meta.len() as usize;
        if len > 0 && len as u64 <= MAX_HANDOFF_BYTES {
            let _ = tokio::fs::write(path, vec![0u8; len]).await;
        }
    }
    let _ = tokio::fs::remove_file(path).await;
}

async fn quarantine(path: &Path, reason: &str, out: &mut InboxDrain) {
    let target = path.with_extension("json.invalid");
    if tokio::fs::rename(path, &target).await.is_ok() {
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        out.rejected.push(format!("{name}（{reason}）"));
    }
}

/// Async scanner for `~/.local/share/native-isp-vps/inbox/` using non-blocking `tokio::fs`.
pub async fn drain_inbox_async() -> InboxDrain {
    if !INBOX_DIR_READY.swap(true, Ordering::Relaxed) {
        let _ = tokio::task::spawn_blocking(storage::ensure_dirs).await;
    }
    let mut out = InboxDrain::default();
    let Ok(mut entries) = tokio::fs::read_dir(storage::inbox_dir()).await else {
        return out;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if meta.len() > MAX_HANDOFF_BYTES {
            quarantine(&path, "文件过大", &mut out).await;
            continue;
        }
        let settled = meta
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age >= PARSE_GRACE);
        let Ok(raw) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let payload = match serde_json::from_str::<HandoffPayload>(&raw) {
            Ok(p) => p,
            Err(_) if settled => {
                quarantine(&path, "JSON 格式错误", &mut out).await;
                continue;
            }
            Err(_) => continue, // possibly still being written by a non-atomic writer
        };
        match tokio::task::spawn_blocking(move || storage::upsert_from_handoff(payload)).await {
            Ok(Ok(node)) => {
                shred_file_async(&path).await;
                out.imported.push(node);
            }
            Ok(Err(reason)) => quarantine(&path, &reason, &mut out).await,
            Err(_) => {}
        }
    }
    out
}
