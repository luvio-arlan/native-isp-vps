use crate::models::{AiProbeResult, LeakCheckResult, LocalShieldStatus};
use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

static PREV_TRAFFIC: Mutex<Option<(Instant, u64, u64)>> = Mutex::new(None);

pub async fn tcping(host: &str, port: u16) -> Option<f64> {
    let addr = format!("{}:{}", host, port);
    let start = Instant::now();
    match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await {
        Ok(Ok(_stream)) => Some((start.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0),
        _ => None,
    }
}

#[cfg(unix)]
fn find_mihomo_socket() -> Option<PathBuf> {
    // Try current UID from `id -u`
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "501".to_string());

    let candidates = [
        PathBuf::from(format!(
            "/var/run/clash-verge-service/users/{}/verge-mihomo.sock",
            uid
        )),
        PathBuf::from("/tmp/verge/verge-mihomo.sock"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(unix)]
async fn unix_http_get_json(sock_path: &PathBuf, endpoint: &str) -> Result<Value, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    let mut stream = tokio::time::timeout(Duration::from_secs(2), UnixStream::connect(sock_path))
        .await
        .map_err(|_| "unix socket connect timeout".to_string())?
        .map_err(|e| format!("unix socket connect err: {e}"))?;

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        endpoint
    );
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| e.to_string())?;

    let mut buf = Vec::with_capacity(16384);
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut buf))
        .await
        .map_err(|_| "unix socket read timeout".to_string())?
        .map_err(|e| e.to_string())?;

    let raw = String::from_utf8_lossy(&buf);
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or(raw.as_ref());

    // Handle chunked transfer encoding if present
    let trimmed = body.trim();
    let json_slice = if let Some(idx) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            &trimmed[idx..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    serde_json::from_str(json_slice).map_err(|e| format!("mihomo json parse err: {e}"))
}

async fn http_mihomo_get_json(endpoint: &str) -> Result<Value, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("http://127.0.0.1:9097{}", endpoint);
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    resp.json::<Value>().await.map_err(|e| e.to_string())
}

async fn mihomo_get(endpoint: &str) -> Result<(Value, String), String> {
    #[cfg(unix)]
    {
        if let Some(sock) = find_mihomo_socket() {
            if let Ok(val) = unix_http_get_json(&sock, endpoint).await {
                return Ok((val, "unix-socket".to_string()));
            }
        }
    }
    let val = http_mihomo_get_json(endpoint).await?;
    Ok((val, "http-api".to_string()))
}

pub async fn probe_local_shield(vps_host: Option<&str>) -> LocalShieldStatus {
    let mut shield = LocalShieldStatus {
        mihomo_source: "offline".to_string(),
        mode: "unknown".to_string(),
        ..Default::default()
    };

    if let Ok((cfg, src)) = mihomo_get("/configs").await {
        shield.mihomo_connected = true;
        shield.mihomo_source = src;
        shield.mode = cfg
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("rule")
            .to_string();
        shield.ipv6_disabled = cfg.get("ipv6").and_then(|v| v.as_bool()).map(|b| !b);
        shield.tcp_concurrent = cfg.get("tcp-concurrent").and_then(|v| v.as_bool());
        if let Some(tun) = cfg.get("tun") {
            shield.tun_enabled = tun.get("enable").and_then(|v| v.as_bool());
            shield.tun_stack = tun
                .get("stack")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        // Check local Clash Verge dns_config.yaml for fake-ip confirmation if available
        if let Some(home) = dirs::home_dir() {
            let dns_path = home.join(
                "Library/Application Support/io.github.clash-verge-rev.clash-verge-rev/dns_config.yaml",
            );
            if let Ok(content) = std::fs::read_to_string(dns_path) {
                shield.fake_ip_enabled = Some(content.contains("fake-ip"));
            }
        }
    }

    if shield.mihomo_connected {
        if let Ok((conns, _)) = mihomo_get("/connections").await {
            let up_total = conns
                .get("uploadTotal")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let down_total = conns
                .get("downloadTotal")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let now = Instant::now();
            if let Ok(mut guard) = PREV_TRAFFIC.lock() {
                if let Some((prev_t, prev_up, prev_down)) = *guard {
                    let dt = now.duration_since(prev_t).as_secs_f64();
                    if dt >= 0.25 {
                        shield.local_up_bps =
                            (up_total.saturating_sub(prev_up) as f64 / dt) as u64;
                        shield.local_down_bps =
                            (down_total.saturating_sub(prev_down) as f64 / dt) as u64;
                    }
                }
                *guard = Some((now, up_total, down_total));
            }

            if let Some(arr) = conns.get("connections").and_then(|v| v.as_array()) {
                shield.active_connections = arr.len();
                let mut direct = 0usize;
                let mut proxy = 0usize;
                for c in arr {
                    let chains = c.get("chains").and_then(|v| v.as_array());
                    let is_direct = chains
                        .and_then(|ch| ch.first())
                        .and_then(|s| s.as_str())
                        .is_some_and(|s| s.eq_ignore_ascii_case("DIRECT"));
                    if is_direct {
                        direct += 1;
                    } else {
                        proxy += 1;
                    }
                }
                shield.direct_connections = direct;
                shield.proxy_connections = proxy;
            }
        }

        if let Ok((rules, _)) = mihomo_get("/rules").await {
            if let Some(arr) = rules.get("rules").and_then(|v| v.as_array()) {
                let has_quic_reject = arr.iter().any(|r| {
                    let payload = r.get("payload").and_then(|v| v.as_str()).unwrap_or("");
                    let proxy = r.get("proxy").and_then(|v| v.as_str()).unwrap_or("");
                    proxy.eq_ignore_ascii_case("REJECT")
                        && payload.contains("UDP")
                        && payload.contains("443")
                });
                shield.quic_blocked = Some(has_quic_reject);
            }
        }
    }

    if let Some(host) = vps_host {
        if let Some(exit) = shield.exit_ipv4.as_deref() {
            shield.matches_vps_ip = Some(exit == host);
        }
    }

    shield
}

pub async fn run_leak_check(vps_host: Option<&str>) -> LeakCheckResult {
    let client_v4 = Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .ok();
    let client_v6 = Client::builder()
        .timeout(Duration::from_millis(2500))
        .build()
        .ok();

    let exit_ipv4 = if let Some(c) = client_v4 {
        match c.get("https://api.ipify.org").send().await {
            Ok(resp) if resp.status().is_success() => resp
                .text()
                .await
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            _ => None,
        }
    } else {
        None
    };

    let ipv6_leak = if let Some(c) = client_v6 {
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
    };

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

    let client = match Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("native-isp-cockpit/0.1")
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut handles = Vec::new();
    for (name, host, url) in targets {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            let start = Instant::now();
            match c.get(url).send().await {
                Ok(resp) => {
                    let code = resp.status().as_u16();
                    let ms = (start.elapsed().as_secs_f64() * 1000.0).round();
                    // 2xx, 3xx, 401, 403, 404 all prove clean TLS + API reachability (no geo-block / timeout)
                    let ok = code < 500 && code != 451;
                    AiProbeResult {
                        name: name.to_string(),
                        host: host.to_string(),
                        ok,
                        status_code: Some(code),
                        latency_ms: Some(ms),
                        error: if ok { None } else { Some(format!("HTTP {}", code)) },
                    }
                }
                Err(e) => AiProbeResult {
                    name: name.to_string(),
                    host: host.to_string(),
                    ok: false,
                    status_code: None,
                    latency_ms: None,
                    error: Some(e.to_string()),
                },
            }
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }
    results
}
