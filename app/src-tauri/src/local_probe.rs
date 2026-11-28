use crate::models::{AiProbeResult, LeakCheckResult, NodeConfig};
use chrono::Utc;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

const RTT_STRATEGY_TTL: Duration = Duration::from_secs(300);

static RTT_PREFER_HEAD: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
/// Tracks `(last_probe_instant, running_baseline_ms)` per panel URL so cold TLS handshakes
/// or brief server-side contention spikes never ratchet up the baseline threshold.
static LAST_HEAD_PROBE: OnceLock<Mutex<HashMap<String, (Instant, f64)>>> = OnceLock::new();
static RTT_HTTP_CLIENT: OnceLock<Client> = OnceLock::new();
static AI_PROBE_CLIENT: OnceLock<Client> = OnceLock::new();
static LEAK_V4_CLIENT: OnceLock<Client> = OnceLock::new();
static LEAK_V6_CLIENT: OnceLock<Client> = OnceLock::new();

pub async fn tcping(host: &str, port: u16) -> Option<f64> {
    let start = Instant::now();
    match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((host, port))).await {
        Ok(Ok(_stream)) => Some((start.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0),
        _ => None,
    }
}

fn rtt_http_client() -> Option<&'static Client> {
    if RTT_HTTP_CLIENT.get().is_none() {
        if let Ok(c) = Client::builder()
            .danger_accept_invalid_certs(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .connect_timeout(Duration::from_secs(3))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
        {
            let _ = RTT_HTTP_CLIENT.set(c);
        }
    }
    RTT_HTTP_CLIENT.get()
}

fn ai_probe_client() -> Option<&'static Client> {
    if AI_PROBE_CLIENT.get().is_none() {
        if let Ok(c) = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(6))
            .local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
            .pool_idle_timeout(Duration::from_secs(60))
            .user_agent("native-isp-cockpit/0.2")
            .build()
        {
            let _ = AI_PROBE_CLIENT.set(c);
        }
    }
    AI_PROBE_CLIENT.get()
}

fn leak_v4_client() -> Option<&'static Client> {
    if LEAK_V4_CLIENT.get().is_none() {
        if let Ok(c) = Client::builder()
            .timeout(Duration::from_secs(4))
            .local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
        {
            let _ = LEAK_V4_CLIENT.set(c);
        }
    }
    LEAK_V4_CLIENT.get()
}

fn leak_v6_client() -> Option<&'static Client> {
    if LEAK_V6_CLIENT.get().is_none() {
        if let Ok(c) = Client::builder()
            .timeout(Duration::from_millis(2500))
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
        {
            let _ = LEAK_V6_CLIENT.set(c);
        }
    }
    LEAK_V6_CLIENT.get()
}

async fn probe_panel_head_rtt(trimmed_panel: &str) -> Option<f64> {
    if trimmed_panel.is_empty() {
        return None;
    }
    let client = rtt_http_client()?;
    let url = format!("{}/", trimmed_panel);

    let prev = LAST_HEAD_PROBE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|m| m.get(trimmed_panel).copied());

    let start = Instant::now();
    if client.head(&url).send().await.is_err() {
        return None;
    }
    let first_ms = (start.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0;

    // After idle intervals (>=8s), on initial cold connect, or when a single probe jumps
    // >=20% & >=25ms above the running baseline (e.g. server closed keep-alive or 3x-ui
    // panel contention), fire one immediate keep-alive follow-up HEAD and keep the faster RTT.
    let likely_cold = match prev {
        None => first_ms > 150.0,
        Some((last_at, baseline_ms)) => {
            last_at.elapsed() >= Duration::from_secs(8)
                || (first_ms > baseline_ms * 1.20 && (first_ms - baseline_ms) >= 25.0)
        }
    };

    let measured = if likely_cold {
        let t_warm = Instant::now();
        if client.head(&url).send().await.is_ok() {
            let warm_ms = (t_warm.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0;
            first_ms.min(warm_ms)
        } else {
            first_ms
        }
    } else {
        first_ms
    };

    let next_baseline = match prev {
        None => measured,
        Some((_, prev_base)) => {
            // Pull baseline down faster than up so occasional congestion cannot ratchet it upward
            if measured < prev_base {
                prev_base * 0.55 + measured * 0.45
            } else {
                prev_base * 0.85 + measured * 0.15
            }
        }
    };

    if let Ok(mut m) = LAST_HEAD_PROBE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        m.insert(trimmed_panel.to_string(), (Instant::now(), next_baseline));
    }

    Some(measured)
}

/// Measures true network RTT to the node.
///
/// macOS first tries a TCP handshake bound to the physical uplink (bypasses the Clash/Mihomo TUN
/// stack, which would otherwise answer the SYN locally in <1ms) against the panel / subscription
/// HTTP ports, so the probe never touches the Reality port (each Reality accept makes Xray dial
/// its camouflage target). Falls back to the adaptive TUN-aware strategy everywhere else.
pub async fn probe_real_rtt(node: &NodeConfig) -> Option<f64> {
    #[cfg(target_os = "macos")]
    if let Ok(ip) = node.host.parse::<std::net::IpAddr>() {
        if let Some(ms) = uplink::probe_candidates(&node.host, ip, &uplink_probe_ports(node)).await {
            return Some(ms);
        }
    }
    probe_rtt_adaptive(&node.host, node.vless_port, node.ssh_port, &node.panel_url).await
}

/// Candidate ports for the uplink-bound handshake: panel HTTP, subscription HTTP, then Reality.
fn uplink_probe_ports(node: &NodeConfig) -> Vec<u16> {
    let http_ports = [Some(node.panel_url.as_str()), node.sub_url.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(|url| reqwest::Url::parse(url.trim()).ok())
        .filter(|url| url.host_str() == Some(node.host.as_str()))
        .filter_map(|url| url.port_or_known_default());
    let mut ports: Vec<u16> = Vec::with_capacity(3);
    for port in http_ports.chain(std::iter::once(node.vless_port)) {
        if !ports.contains(&port) {
            ports.push(port);
        }
    }
    ports
}

#[cfg(target_os = "macos")]
mod uplink {
    //! TCP handshake RTT over the physical uplink (`IP_BOUND_IF`), bypassing any local TUN.
    use std::collections::HashMap;
    use std::net::{IpAddr, SocketAddr};
    use std::num::NonZeroU32;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const UPLINK_TTL: Duration = Duration::from_secs(30);
    static UPLINK: Mutex<Option<(Instant, Option<NonZeroU32>)>> = Mutex::new(None);
    /// Last port that answered per host, so steady-state polling costs exactly one handshake.
    static GOOD_PORT: Mutex<Option<HashMap<String, u16>>> = Mutex::new(None);

    /// Interface index of the default route (e.g. `en0`); `None` when the default is a tunnel.
    async fn uplink_index() -> Option<NonZeroU32> {
        if let Some((at, idx)) = UPLINK.lock().ok().and_then(|g| *g) {
            if at.elapsed() < UPLINK_TTL {
                return idx;
            }
        }
        let output = tokio::process::Command::new("/sbin/route")
            .args(["-n", "get", "default"])
            .output()
            .await
            .ok();
        let iface = output
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|text| {
                text.lines()
                    .find_map(|l| l.trim().strip_prefix("interface:").map(|v| v.trim().to_string()))
            });
        let idx = iface
            .filter(|name| !name.starts_with("utun") && !name.starts_with("lo"))
            .and_then(|name| std::ffi::CString::new(name).ok())
            // SAFETY: `name` is a valid NUL-terminated C string that outlives the call.
            .and_then(|name| NonZeroU32::new(unsafe { libc::if_nametoindex(name.as_ptr()) }));
        if let Ok(mut guard) = UPLINK.lock() {
            *guard = Some((Instant::now(), idx));
        }
        idx
    }

    async fn tcping_bound(ip: IpAddr, port: u16, index: NonZeroU32) -> Option<f64> {
        let IpAddr::V4(v4) = ip else {
            return None;
        };
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )
        .ok()?;
        socket.bind_device_by_index_v4(Some(index)).ok()?;
        socket.set_nonblocking(true).ok()?;
        let tcp = tokio::net::TcpSocket::from_std_stream(socket.into());
        let start = Instant::now();
        let connected = tokio::time::timeout(
            Duration::from_secs(3),
            tcp.connect(SocketAddr::new(IpAddr::V4(v4), port)),
        )
        .await;
        matches!(connected, Ok(Ok(_)))
            .then(|| (start.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0)
    }

    pub async fn probe_candidates(host: &str, ip: IpAddr, ports: &[u16]) -> Option<f64> {
        let index = uplink_index().await?;
        let remembered = GOOD_PORT
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|m| m.get(host).copied()));
        let ordered = remembered
            .into_iter()
            .chain(ports.iter().copied().filter(|p| Some(*p) != remembered));
        for port in ordered {
            if let Some(ms) = tcping_bound(ip, port, index).await {
                if let Ok(mut guard) = GOOD_PORT.lock() {
                    guard.get_or_insert_with(HashMap::new).insert(host.to_string(), port);
                }
                return Some(ms);
            }
        }
        None
    }
}

/// TUN-aware fallback: adaptively bypasses Clash TUN's local SYN-ACK hijack via panel `HEAD`
/// keep-alive probes (or SSH banner timing) when a plain TCP handshake is suspiciously fast.
async fn probe_rtt_adaptive(
    host: &str,
    vless_port: u16,
    ssh_port: u16,
    panel_url: &str,
) -> Option<f64> {
    let trimmed_panel = panel_url.trim().trim_end_matches('/');
    let key = format!("{}:{}", host, vless_port);
    let prefer_head = RTT_PREFER_HEAD
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|m| m.get(&key).copied())
        .is_some_and(|t| t.elapsed() < RTT_STRATEGY_TTL);

    if prefer_head {
        if let Some(ms) = probe_panel_head_rtt(trimmed_panel).await {
            return Some(ms);
        }
    }

    let tcp_rtt = tcping(host, vless_port).await;
    if let Some(ms) = tcp_rtt {
        if ms >= 15.0 {
            if let Ok(mut m) = RTT_PREFER_HEAD
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
            {
                m.remove(&key);
            }
            return Some(ms);
        }
        if !trimmed_panel.is_empty() {
            if let Ok(mut m) = RTT_PREFER_HEAD
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
            {
                m.insert(key, Instant::now());
            }
        }
    }

    if let Some(ms) = probe_panel_head_rtt(trimmed_panel).await {
        return Some(ms);
    }

    if ssh_port > 0 {
        use tokio::io::AsyncReadExt;
        let start = Instant::now();
        if let Ok(Ok(mut stream)) =
            tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((host, ssh_port))).await
        {
            let mut buf = [0u8; 32];
            if let Ok(Ok(n)) =
                tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf)).await
            {
                if n > 0 {
                    let one_rtt =
                        ((start.elapsed().as_secs_f64() * 1000.0 / 2.0) * 10.0).round() / 10.0;
                    return Some(one_rtt);
                }
            }
        }
    }

    tcp_rtt
}

async fn fetch_exit_ipv4(client: &Client) -> Option<String> {
    const PROVIDERS: [&str; 3] = [
        "https://api.ipify.org",
        "https://ipinfo.io/ip",
        "https://ifconfig.me/ip",
    ];
    for url in PROVIDERS {
        if let Ok(resp) = client.get(url).send().await {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() && trimmed.parse::<std::net::Ipv4Addr>().is_ok() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }
    None
}

pub async fn run_leak_check(vps_host: Option<&str>) -> LeakCheckResult {
    let client_v4 = leak_v4_client();
    let client_v6 = leak_v6_client();

    let (exit_ipv4, ipv6_leak) = tokio::join!(
        async {
            match client_v4 {
                Some(c) => fetch_exit_ipv4(c).await,
                None => None,
            }
        },
        async {
            if let Some(c) = client_v6 {
                match c.get("https://api6.ipify.org").send().await {
                    Ok(resp) if resp.status().is_success() => resp
                        .text()
                        .await
                        .ok()
                        .is_some_and(|s| s.contains(':')),
                    _ => false,
                }
            } else {
                false
            }
        }
    );

    let matches_vps_ip = match (exit_ipv4.as_deref(), vps_host) {
        (Some(exit), Some(host)) => exit == host,
        _ => false,
    };

    LeakCheckResult {
        exit_ipv4,
        ipv6_leak,
        matches_vps_ip,
        checked_at: Utc::now().to_rfc3339(),
    }
}

pub async fn probe_ai_endpoints() -> Vec<AiProbeResult> {
    let targets = [
        ("Anthropic API", "api.anthropic.com", "https://api.anthropic.com/"),
        ("OpenAI API", "api.openai.com", "https://api.openai.com/v1/models"),
        (
            "Gemini API",
            "generativelanguage.googleapis.com",
            "https://generativelanguage.googleapis.com/",
        ),
        ("GitHub API", "api.github.com", "https://api.github.com/zen"),
        ("Google 204", "www.google.com", "https://www.google.com/generate_204"),
    ];

    let Some(client) = ai_probe_client() else {
        return Vec::new();
    };

    let mut handles = Vec::with_capacity(targets.len());
    for (name, host, url) in targets {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            // 1) Warmup HEAD opens TCP + proxy tunnel + TLS 1.3 + HTTP/2 connection
            //    so cold handshake overhead does not inflate interactive 1-RTT latency.
            let t_cold = Instant::now();
            match c.head(url).send().await {
                Ok(resp) => {
                    let code = resp.status().as_u16();
                    let cold_ms = (t_cold.elapsed().as_secs_f64() * 1000.0).round().max(1.0);
                    drop(resp);

                    // 2) Two back-to-back warmed HEAD probes over the live keep-alive/H2 stream;
                    //    take the best (min) to suppress single-packet proxy micro-stutters.
                    let mut best_warm_ms: Option<f64> = None;
                    for _ in 0..2 {
                        let t = Instant::now();
                        if let Ok(r) = c.head(url).send().await {
                            let ms = (t.elapsed().as_secs_f64() * 1000.0).round().max(1.0);
                            drop(r);
                            best_warm_ms = Some(match best_warm_ms {
                                Some(prev) => prev.min(ms),
                                None => ms,
                            });
                        }
                    }

                    let ok = code < 500 && code != 451;
                    AiProbeResult {
                        name: name.to_string(),
                        host: host.to_string(),
                        ok,
                        status_code: Some(code),
                        latency_ms: Some(best_warm_ms.unwrap_or(cold_ms)),
                        cold_latency_ms: Some(cold_ms),
                        error: if ok { None } else { Some(format!("HTTP {}", code)) },
                    }
                }
                Err(e) => AiProbeResult {
                    name: name.to_string(),
                    host: host.to_string(),
                    ok: false,
                    status_code: None,
                    latency_ms: None,
                    cold_latency_ms: None,
                    error: Some(e.to_string()),
                },
            }
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uplink_ports_prefer_http_listeners_and_dedupe_reality_port() {
        let node = NodeConfig {
            id: "n1".into(),
            name: "n".into(),
            flag: "🌐".into(),
            city: "LA".into(),
            isp_tag: "Dual ISP".into(),
            host: "1.2.3.4".into(),
            vless_port: 443,
            panel_url: "http://1.2.3.4:2053/base".into(),
            panel_username: None,
            sub_url: Some("http://1.2.3.4:2096/sub/abc".into()),
            mihomo_sub_url: None,
            ssh_port: 22,
            ssh_user: "root".into(),
            ssh_key_path: None,
            reality_target: None,
            monthly_total_gb: 1000.0,
            reset_day: 1,
            created_at: String::new(),
            source: "test".into(),
        };
        assert_eq!(uplink_probe_ports(&node), vec![2053, 2096, 443]);

        let cdn_panel = NodeConfig {
            panel_url: "https://panel.example.com/base".into(),
            sub_url: Some("http://1.2.3.4:443/sub/abc".into()),
            ..node
        };
        assert_eq!(uplink_probe_ports(&cdn_panel), vec![443]);
    }
}
