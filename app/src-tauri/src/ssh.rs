//! Single source of truth for spawning OpenSSH (`lint-arch` forbids `Command::new("ssh")` elsewhere).
//!
//! Every invocation is non-interactive (`BatchMode`), re-validates host/user (defence in depth for
//! legacy `nodes.json`) and terminates option parsing with `--`, so a crafted handoff value such as
//! `-oProxyCommand=...` can never be interpreted as an ssh option.

use crate::models::NodeConfig;
use crate::{storage, validate};
use std::path::PathBuf;
use tokio::process::Command;

/// How the ssh client shares connections with other invocations.
pub enum SshMode {
    /// One-shot command multiplexed over the shared ControlMaster (carrier probe, kernel audit).
    Multiplexed { persist_secs: u32 },
    /// Dedicated long-lived session that never touches the shared master (panel API tunnel).
    Dedicated,
}

/// ControlMaster socket path. macOS caps Unix socket paths at 104 bytes and `%C` expands to
/// 40 hex chars, so fall back to `/tmp` for very long home directories.
pub fn control_path() -> PathBuf {
    let preferred = storage::config_dir().join("s-%C");
    if preferred.to_string_lossy().len() + 38 <= 98 {
        preferred
    } else {
        PathBuf::from("/tmp/nisp-%C")
    }
}

/// Expands a leading `~/` to the current user's home directory.
pub fn expand_key_path(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.strip_prefix("~/") {
        Some(rest) => dirs::home_dir()
            .map(|home| home.join(rest).to_string_lossy().to_string())
            .unwrap_or_else(|| trimmed.to_string()),
        None => trimmed.to_string(),
    }
}

/// Builds `ssh [hardened opts] [extra_args] -- user@host remote_cmd` for `node`.
pub fn command(
    node: &NodeConfig,
    mode: SshMode,
    extra_args: &[String],
    remote_cmd: &str,
) -> Result<Command, String> {
    let host = validate::host(&node.host)?;
    let user = validate::ssh_user(&node.ssh_user)?;

    let mut cmd = Command::new("ssh");
    cmd.kill_on_drop(true);
    for opt in [
        "BatchMode=yes",
        "ConnectTimeout=6",
        "StrictHostKeyChecking=accept-new",
        "LogLevel=ERROR",
    ] {
        cmd.arg("-o").arg(opt);
    }
    match mode {
        SshMode::Multiplexed { persist_secs } => {
            cmd.args(["-o", "ServerAliveInterval=5", "-o", "ControlMaster=auto"])
                .arg("-o")
                .arg(format!("ControlPersist={persist_secs}s"))
                .arg("-o")
                .arg(format!("ControlPath={}", control_path().to_string_lossy()));
        }
        SshMode::Dedicated => {
            cmd.args([
                "-o",
                "ControlMaster=no",
                "-o",
                "ControlPath=none",
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                "-o",
                "ExitOnForwardFailure=yes",
            ]);
        }
    }
    cmd.arg("-p").arg(node.ssh_port.to_string());
    if let Some(key) = node
        .ssh_key_path
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        cmd.arg("-i").arg(expand_key_path(key));
    }
    cmd.args(extra_args);
    cmd.arg("--").arg(format!("{user}@{host}")).arg(remote_cmd);
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(user: &str, host: &str) -> NodeConfig {
        NodeConfig {
            id: "n1".into(),
            name: "n".into(),
            flag: "🌐".into(),
            city: "LA".into(),
            isp_tag: "Dual ISP".into(),
            host: host.into(),
            vless_port: 443,
            panel_url: String::new(),
            panel_username: None,
            sub_url: None,
            mihomo_sub_url: None,
            ssh_port: 2222,
            ssh_user: user.into(),
            ssh_key_path: Some("~/.ssh/id_test".into()),
            reality_target: None,
            monthly_total_gb: 1000.0,
            reset_day: 1,
            created_at: String::new(),
            source: "test".into(),
        }
    }

    #[test]
    fn command_terminates_options_before_destination() {
        let cmd = command(&node("root", "1.2.3.4"), SshMode::Dedicated, &[], "true").unwrap();
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        let sep = args.iter().position(|a| a == "--").expect("-- separator");
        assert_eq!(args[sep + 1], "root@1.2.3.4");
        assert_eq!(args[sep + 2], "true");
        assert!(args.contains(&"2222".to_string()));
    }

    #[test]
    fn command_rejects_injected_user_or_host() {
        assert!(command(&node("-oProxyCommand=id", "1.2.3.4"), SshMode::Dedicated, &[], "x").is_err());
        assert!(command(&node("root", "-oProxyCommand=id"), SshMode::Dedicated, &[], "x").is_err());
    }
}
