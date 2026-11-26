use crate::models::{HandoffPayload, NodeConfig};
use crate::storage;
use std::fs;
use std::path::Path;

/// Overwrites file bytes with zeros before deleting so credentials never linger on disk.
fn shred_file(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let len = meta.len() as usize;
        if len > 0 && len < 1_048_576 {
            let zeros = vec![0u8; len];
            let _ = fs::write(path, zeros);
        }
    }
    let _ = fs::remove_file(path);
}

/// Scans `~/.local/share/native-isp-vps/inbox/` for JSON handoff payloads written by AI Agent Skill.
/// Imports each node, saves secrets into local 0600 storage, and immediately shreds the handoff file.
pub fn drain_inbox() -> Vec<NodeConfig> {
    let _ = storage::ensure_dirs();
    let dir = storage::inbox_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut imported = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(payload) = serde_json::from_str::<HandoffPayload>(&raw) {
                if let Ok(node) = storage::upsert_from_handoff(payload) {
                    shred_file(&path);
                    imported.push(node);
                }
            }
        }
    }
    imported
}
