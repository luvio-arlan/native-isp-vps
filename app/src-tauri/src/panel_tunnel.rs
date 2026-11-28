//! Encrypted transport for the 3x-ui panel API.
//!
//! Panels deployed by SKILL.md listen on plain `http://`, so the admin Bearer token (and the
//! cookie-login password) would otherwise cross the public internet in cleartext on every poll.
//! When the node has an SSH key we keep one dedicated `ssh -L` child per node and talk to the panel
//! through `127.0.0.1:<local_port>`. Policy:
//! - `https://` panels → direct TLS.
//! - tunnel up → `ssh-tunnel`.
//! - tunnel never worked this session (no key / auth refused) → `plaintext` (UI shows a warning).
//! - tunnel worked before but is down → `paused`: credentials are never downgraded to plaintext.
//!
//! The child's stdin is a pipe owned by this process and the remote command is `cat >/dev/null`:
//! if the app dies, the remote side sees EOF and the tunnel tears itself down (no orphaned ssh,
//! no resident VPS agent).

use crate::models::NodeConfig;
use crate::ssh::{self, SshMode};
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;

const READY_TIMEOUT: Duration = Duration::from_secs(8);
const FIRST_FAILURE_BACKOFF: Duration = Duration::from_secs(120);
const RECONNECT_BACKOFF: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelTransport {
    SshTunnel,
    Tls,
    Plaintext,
    Paused,
}

impl PanelTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SshTunnel => "ssh-tunnel",
            Self::Tls => "tls",
            Self::Plaintext => "plaintext",
            Self::Paused => "paused",
        }
    }
}

pub struct PanelRoute {
    /// Base URL to prefix API paths with (empty when `Paused`).
    pub base: String,
    pub transport: PanelTransport,
}

struct Tunnel {
    child: Child,
    /// Held open for the tunnel's lifetime; dropping it delivers EOF to the remote `cat`.
    _stdin: Option<ChildStdin>,
    local_port: u16,
    fingerprint: String,
}

#[derive(Default)]
struct Registry {
    tunnels: HashMap<String, Tunnel>,
    retry_after: HashMap<String, Instant>,
    /// Nodes whose tunnel came up at least once this session (enables fail-closed `Paused`).
    proven: HashSet<String>,
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

/// Splits `http://host:port/base` into `(panel_port, "/base")`; `None` for non-`http` schemes.
fn split_http_panel(panel_url: &str) -> Option<(u16, String)> {
    let url = reqwest::Url::parse(panel_url).ok()?;
    if url.scheme() != "http" {
        return None;
    }
    let port = url.port_or_known_default()?;
    Some((port, url.path().trim_end_matches('/').to_string()))
}

fn tunneled(local_port: u16, path: &str) -> PanelRoute {
    PanelRoute {
        base: format!("http://127.0.0.1:{local_port}{path}"),
        transport: PanelTransport::SshTunnel,
    }
}

/// Resolves how the panel API for `node` should be reached right now.
pub async fn resolve(node: &NodeConfig) -> PanelRoute {
    let direct = node.panel_url.trim().trim_end_matches('/').to_string();
    if direct.starts_with("https://") {
        return PanelRoute {
            base: direct,
            transport: PanelTransport::Tls,
        };
    }
    let has_key = node
        .ssh_key_path
        .as_deref()
        .is_some_and(|k| !k.trim().is_empty());
    let Some((panel_port, path)) = split_http_panel(&direct).filter(|_| has_key) else {
        return PanelRoute {
            base: direct,
            transport: PanelTransport::Plaintext,
        };
    };
    let fingerprint = format!(
        "{}@{}:{}->{}|{:?}",
        node.ssh_user, node.host, node.ssh_port, panel_port, node.ssh_key_path
    );

    let mut reg = registry().lock().await;
    let reusable = reg.tunnels.get_mut(&node.id).and_then(|t| {
        let alive = t.fingerprint == fingerprint && matches!(t.child.try_wait(), Ok(None));
        alive.then_some(t.local_port)
    });
    if let Some(port) = reusable {
        return tunneled(port, &path);
    }
    reg.tunnels.remove(&node.id);

    let degraded = || PanelRoute {
        base: direct.clone(),
        transport: PanelTransport::Plaintext,
    };
    let proven = reg.proven.contains(&node.id);
    if reg
        .retry_after
        .get(&node.id)
        .is_some_and(|until| Instant::now() < *until)
    {
        return if proven { paused() } else { degraded() };
    }

    match spawn_tunnel(node, panel_port, &path).await {
        Ok(mut tunnel) => {
            let port = tunnel.local_port;
            tunnel.fingerprint = fingerprint;
            reg.retry_after.remove(&node.id);
            reg.proven.insert(node.id.clone());
            reg.tunnels.insert(node.id.clone(), tunnel);
            tunneled(port, &path)
        }
        Err(_) => {
            let backoff = if proven {
                RECONNECT_BACKOFF
            } else {
                FIRST_FAILURE_BACKOFF
            };
            reg.retry_after
                .insert(node.id.clone(), Instant::now() + backoff);
            if proven {
                paused()
            } else {
                degraded()
            }
        }
    }
}

fn paused() -> PanelRoute {
    PanelRoute {
        base: String::new(),
        transport: PanelTransport::Paused,
    }
}

fn pick_free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|e| e.to_string())
}

/// End-to-end readiness: the panel must answer an HTTP `HEAD` through the forward. A bare local
/// accept is not enough (e.g. `AllowTcpForwarding no` refuses every channel after the bind).
async fn panel_answers(mut stream: tokio::net::TcpStream, path: &str) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let request = format!("HEAD {path}/ HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).await.is_err() {
        return false;
    }
    let mut head = [0u8; 5];
    let read = tokio::time::timeout(Duration::from_secs(4), stream.read_exact(&mut head)).await;
    matches!(read, Ok(Ok(_))) && &head == b"HTTP/"
}

async fn spawn_tunnel(node: &NodeConfig, panel_port: u16, path: &str) -> Result<Tunnel, String> {
    let local_port = pick_free_port()?;
    let forward = format!("127.0.0.1:{local_port}:127.0.0.1:{panel_port}");
    let mut cmd = ssh::command(
        node,
        SshMode::Dedicated,
        &["-L".to_string(), forward],
        "cat >/dev/null",
    )?;
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("ssh 隧道启动失败: {e}"))?;
    let stdin = child.stdin.take();

    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("ssh 隧道提前退出: {status}"));
        }
        let probe = tokio::time::timeout(
            Duration::from_millis(300),
            tokio::net::TcpStream::connect(("127.0.0.1", local_port)),
        )
        .await;
        if let Ok(Ok(stream)) = probe {
            // Local bind is up (ssh authenticated); now verify the panel answers end to end.
            return if panel_answers(stream, path).await {
                Ok(Tunnel {
                    child,
                    _stdin: stdin,
                    local_port,
                    fingerprint: String::new(),
                })
            } else {
                Err("面板经 SSH 隧道无响应（sshd 可能禁用了 TCP 转发）".to_string())
            };
        }
        if Instant::now() >= deadline {
            return Err("ssh 隧道握手超时".to_string());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// Drops the node's tunnel (node deleted or its SSH settings changed).
pub async fn forget(node_id: &str) {
    let mut reg = registry().lock().await;
    reg.tunnels.remove(node_id);
    reg.retry_after.remove(node_id);
    reg.proven.remove(node_id);
}

/// Best-effort synchronous teardown for `RunEvent::Exit` (stdin EOF covers the crash path).
pub fn shutdown_all() {
    if let Some(lock) = REGISTRY.get() {
        if let Ok(mut reg) = lock.try_lock() {
            for (_, mut tunnel) in reg.tunnels.drain() {
                let _ = tunnel.child.start_kill();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::split_http_panel;

    #[test]
    fn splits_http_panel_url_into_port_and_base_path() {
        assert_eq!(
            split_http_panel("http://1.2.3.4:2053/AbCd"),
            Some((2053, "/AbCd".to_string()))
        );
        assert_eq!(split_http_panel("http://1.2.3.4/"), Some((80, String::new())));
        assert_eq!(split_http_panel("https://panel.example.com:8443/x"), None);
        assert_eq!(split_http_panel(""), None);
    }
}
