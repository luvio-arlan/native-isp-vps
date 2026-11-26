use crate::models::{AiProbeResult, LeakCheckResult};
use chrono::Utc;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

const RTT_STRATEGY_TTL: Duration = Duration::from_secs(60);

static RTT_PREFER_HEAD: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
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

    // After idle intervals (>=10s) or cold reconnect spikes, the first HEAD pays TCP+TLS
    // handshake overhead (2-3x real RTT). Fire one immediate keep-alive follow-up HEAD so
    // background polls never pollute jitter or trigger false score drops on open.
    let likely_cold = match prev {
        None => first_ms > 220.0,
        Some((last_at, prev_ms)) => {
            last_at.elapsed() >= Duration::from_secs(10)
                || (first_ms > 240.0 && first_ms > prev_ms * 1.35)
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

    if let Ok(mut m) = LAST_HEAD_PROBE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        m.insert(trimmed_panel.to_string(), (Instant::now(), measured));
    }

    Some(measured)
}

/// Measures true end-to-end RTT to the VPS, adaptively bypassing Clash TUN's local TCP SYN-ACK hijack
/// so active 2s polling never wastes a futile TCP handshake to port 443 on every tick.
pub async fn probe_real_rtt(
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
