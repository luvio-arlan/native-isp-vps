use crate::models::LocalShieldStatus;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use tokio::net::TcpStream;
use tokio::sync::watch;

const SLOW_SHIELD_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug)]
pub struct LiveTrafficSample {
    pub updated_at: Instant,
    pub up_bps: u64,
    pub down_bps: u64,
    pub up_total: u64,
    pub down_total: u64,
}

#[derive(Clone)]
struct CachedSlowShield {
    fetched_at: Instant,
    mihomo_source: String,
    mode: String,
    ipv6_disabled: Option<bool>,
    tcp_concurrent: Option<bool>,
    tun_enabled: Option<bool>,
    tun_stack: Option<String>,
    fake_ip_enabled: Option<bool>,
    quic_blocked: Option<bool>,
}

#[derive(Clone)]
struct CachedDnsCheck {
    mtime: Option<SystemTime>,
    fake_ip_enabled: bool,
}

#[derive(Deserialize)]
struct MihomoTrafficFrame {
    #[serde(default)]
    up: u64,
    #[serde(default)]
    down: u64,
    #[serde(default, rename = "upTotal")]
    up_total: u64,
    #[serde(default, rename = "downTotal")]
    down_total: u64,
}

#[derive(Deserialize)]
struct MihomoTunCfg {
    enable: Option<bool>,
    stack: Option<String>,
}

#[derive(Deserialize)]
struct MihomoConfigsResp {
    mode: Option<String>,
    ipv6: Option<bool>,
    #[serde(rename = "tcp-concurrent")]
    tcp_concurrent: Option<bool>,
    tun: Option<MihomoTunCfg>,
}

#[derive(Deserialize)]
struct MihomoConnEntry {
    #[serde(default)]
    chains: Vec<String>,
}

#[derive(Deserialize)]
struct MihomoConnectionsResp {
    #[serde(default, rename = "uploadTotal")]
    upload_total: u64,
    #[serde(default, rename = "downloadTotal")]
    download_total: u64,
    #[serde(default)]
    connections: Vec<MihomoConnEntry>,
}

#[derive(Deserialize)]
struct MihomoRuleEntry {
    #[serde(default)]
    payload: String,
    #[serde(default)]
    proxy: String,
}

#[derive(Deserialize)]
struct MihomoRulesResp {
    #[serde(default)]
    rules: Vec<MihomoRuleEntry>,
}

static PREV_TRAFFIC: Mutex<Option<(Instant, u64, u64)>> = Mutex::new(None);
static LIVE_TRAFFIC_TX: OnceLock<watch::Sender<Option<LiveTrafficSample>>> = OnceLock::new();
static TRAFFIC_STREAM_STARTED: OnceLock<()> = OnceLock::new();
static SLOW_SHIELD_CACHE: OnceLock<Mutex<Option<CachedSlowShield>>> = OnceLock::new();
static DNS_FILE_CACHE: OnceLock<Mutex<Option<CachedDnsCheck>>> = OnceLock::new();
static MIHOMO_HTTP_CLIENT: OnceLock<Client> = OnceLock::new();
#[cfg(unix)]
static CURRENT_UID: OnceLock<String> = OnceLock::new();

fn live_traffic_tx() -> &'static watch::Sender<Option<LiveTrafficSample>> {
    LIVE_TRAFFIC_TX.get_or_init(|| {
        let (tx, _rx) = watch::channel(None);
        tx
    })
}

pub fn subscribe_live_traffic() -> watch::Receiver<Option<LiveTrafficSample>> {
    ensure_traffic_stream();
    live_traffic_tx().subscribe()
}

pub fn latest_live_traffic() -> Option<LiveTrafficSample> {
    (*live_traffic_tx().borrow()).filter(|s| s.updated_at.elapsed() <= Duration::from_secs(4))
}

fn update_live_traffic_line(line: &str) {
    let trimmed = line.trim();
    let Some(start) = trimmed.find('{') else {
        return;
    };
    let Some(end) = trimmed.rfind('}') else {
        return;
    };
    if end <= start {
        return;
    }
    if let Ok(frame) = serde_json::from_str::<MihomoTrafficFrame>(&trimmed[start..=end]) {
        let sample = LiveTrafficSample {
            updated_at: Instant::now(),
            up_bps: frame.up,
            down_bps: frame.down,
            up_total: frame.up_total,
            down_total: frame.down_total,
        };
        let _ = live_traffic_tx().send(Some(sample));
    }
}

#[cfg(unix)]
async fn stream_unix_traffic(sock_path: &PathBuf) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    let mut stream = tokio::time::timeout(Duration::from_secs(2), UnixStream::connect(sock_path))
        .await
        .map_err(|_| "unix traffic connect timeout".to_string())?
        .map_err(|e| e.to_string())?;

    stream
        .write_all(b"GET /traffic HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n")
        .await
        .map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let n = tokio::time::timeout(Duration::from_secs(3), reader.read_line(&mut line))
            .await
            .map_err(|_| "unix traffic header timeout".to_string())?
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("unix traffic eof".to_string());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }

    loop {
        line.clear();
        let n = tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line))
            .await
            .map_err(|_| "unix traffic read timeout".to_string())?
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("unix traffic closed".to_string());
        }
        if line.contains('{') {
            update_live_traffic_line(&line);
        }
    }
}

async fn stream_http_traffic() -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut stream =
        tokio::time::timeout(Duration::from_secs(2), TcpStream::connect("127.0.0.1:9097"))
            .await
            .map_err(|_| "http traffic connect timeout".to_string())?
            .map_err(|e| e.to_string())?;

    stream
        .write_all(
            b"GET /traffic HTTP/1.1\r\nHost: 127.0.0.1:9097\r\nConnection: keep-alive\r\n\r\n",
        )
        .await
        .map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let n = tokio::time::timeout(Duration::from_secs(3), reader.read_line(&mut line))
            .await
            .map_err(|_| "http traffic header timeout".to_string())?
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("http traffic eof".to_string());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }

    loop {
        line.clear();
        let n = tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line))
            .await
            .map_err(|_| "http traffic read timeout".to_string())?
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("http traffic closed".to_string());
        }
        if line.contains('{') {
            update_live_traffic_line(&line);
        }
    }
}

pub fn ensure_traffic_stream() {
    TRAFFIC_STREAM_STARTED.get_or_init(|| {
        let _ = live_traffic_tx();
        tokio::spawn(async move {
            loop {
                #[cfg(unix)]
                {
                    if let Some(sock) = find_mihomo_socket().await {
                        let _ = stream_unix_traffic(&sock).await;
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
                let _ = stream_http_traffic().await;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    });
}

fn mihomo_http_client() -> Result<&'static Client, String> {
    if let Some(c) = MIHOMO_HTTP_CLIENT.get() {
        return Ok(c);
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let _ = MIHOMO_HTTP_CLIENT.set(client);
    MIHOMO_HTTP_CLIENT
        .get()
        .ok_or_else(|| "mihomo http client unavailable".to_string())
}

#[cfg(unix)]
async fn find_mihomo_socket() -> Option<PathBuf> {
    let uid = CURRENT_UID.get_or_init(|| {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "501".to_string())
    });

    let candidates = [
        PathBuf::from(format!(
            "/var/run/clash-verge-service/users/{}/verge-mihomo.sock",
            uid
        )),
        PathBuf::from("/tmp/verge/verge-mihomo.sock"),
    ];
    for p in candidates {
        if tokio::fs::try_exists(&p).await.unwrap_or(false) {
            return Some(p);
        }
    }
    None
}

#[cfg(unix)]
async fn unix_http_get_typed<T: DeserializeOwned>(
    sock_path: &PathBuf,
    endpoint: &str,
) -> Result<T, String> {
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

async fn http_mihomo_get_typed<T: DeserializeOwned>(endpoint: &str) -> Result<T, String> {
    let client = mihomo_http_client()?;
    let url = format!("http://127.0.0.1:9097{}", endpoint);
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    resp.json::<T>().await.map_err(|e| e.to_string())
}

async fn mihomo_get_typed<T: DeserializeOwned>(endpoint: &str) -> Result<(T, String), String> {
    #[cfg(unix)]
    {
        if let Some(sock) = find_mihomo_socket().await {
            if let Ok(val) = unix_http_get_typed(&sock, endpoint).await {
                return Ok((val, "unix-socket".to_string()));
            }
        }
    }
    let val = http_mihomo_get_typed(endpoint).await?;
    Ok((val, "http-api".to_string()))
}

async fn check_fake_ip_config_async() -> Option<bool> {
    let home = dirs::home_dir()?;
    let dns_path = home.join(
        "Library/Application Support/io.github.clash-verge-rev.clash-verge-rev/dns_config.yaml",
    );
    let mtime = tokio::fs::metadata(&dns_path).await.ok()?.modified().ok();

    {
        let cache = DNS_FILE_CACHE.get_or_init(|| Mutex::new(None));
        if let Ok(guard) = cache.lock() {
            if let Some(entry) = guard.as_ref() {
                if entry.mtime == mtime && mtime.is_some() {
                    return Some(entry.fake_ip_enabled);
                }
            }
        }
    }

    let content = tokio::fs::read_to_string(&dns_path).await.ok()?;
    let enabled = content.contains("fake-ip");
    if let Ok(mut guard) = DNS_FILE_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *guard = Some(CachedDnsCheck {
            mtime,
            fake_ip_enabled: enabled,
        });
    }
    Some(enabled)
}

async fn refresh_slow_shield_cache() -> Option<CachedSlowShield> {
    let (cfg_res, rules_res, fake_ip_enabled) = tokio::join!(
        mihomo_get_typed::<MihomoConfigsResp>("/configs"),
        mihomo_get_typed::<MihomoRulesResp>("/rules"),
        check_fake_ip_config_async()
    );

    let (cfg, src) = cfg_res.ok()?;
    let quic_blocked = rules_res.ok().map(|(rules, _)| {
        rules.rules.iter().any(|r| {
            r.proxy.eq_ignore_ascii_case("REJECT")
                && r.payload.to_ascii_uppercase().contains("UDP")
                && r.payload.contains("443")
        })
    });

    let entry = CachedSlowShield {
        fetched_at: Instant::now(),
        mihomo_source: src,
        mode: cfg.mode.unwrap_or_else(|| "rule".to_string()),
        ipv6_disabled: cfg.ipv6.map(|b| !b),
        tcp_concurrent: cfg.tcp_concurrent,
        tun_enabled: cfg.tun.as_ref().and_then(|t| t.enable),
        tun_stack: cfg.tun.and_then(|t| t.stack),
        fake_ip_enabled,
        quic_blocked,
    };

    if let Ok(mut guard) = SLOW_SHIELD_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *guard = Some(entry.clone());
    }
    Some(entry)
}

pub async fn probe_local_shield(force_slow: bool) -> LocalShieldStatus {
    let mut shield = LocalShieldStatus {
        mihomo_source: "offline".to_string(),
        mode: "unknown".to_string(),
        ..Default::default()
    };

    let cached_slow = if !force_slow {
        SLOW_SHIELD_CACHE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .filter(|c| c.fetched_at.elapsed() < SLOW_SHIELD_TTL)
    } else {
        None
    };

    let (slow_opt, conns_res) = if let Some(cached) = cached_slow {
        let conns = mihomo_get_typed::<MihomoConnectionsResp>("/connections").await;
        if conns.is_ok() {
            (Some(cached), conns)
        } else {
            if let Ok(mut g) = SLOW_SHIELD_CACHE.get_or_init(|| Mutex::new(None)).lock() {
                *g = None;
            }
            (None, conns)
        }
    } else {
        tokio::join!(
            refresh_slow_shield_cache(),
            mihomo_get_typed::<MihomoConnectionsResp>("/connections")
        )
    };

    if let Some(slow) = slow_opt {
        shield.mihomo_connected = true;
        shield.mihomo_source = slow.mihomo_source;
        shield.mode = slow.mode;
        shield.ipv6_disabled = slow.ipv6_disabled;
        shield.tcp_concurrent = slow.tcp_concurrent;
        shield.tun_enabled = slow.tun_enabled;
        shield.tun_stack = slow.tun_stack;
        shield.fake_ip_enabled = slow.fake_ip_enabled;
        shield.quic_blocked = slow.quic_blocked;
    }

    if shield.mihomo_connected {
        ensure_traffic_stream();
        let live_sample = latest_live_traffic();

        if let Some(s) = live_sample {
            shield.local_up_bps = s.up_bps;
            shield.local_down_bps = s.down_bps;
            shield.local_up_total = s.up_total;
            shield.local_down_total = s.down_total;
        }

        if let Ok((conns, _)) = conns_res {
            if shield.local_up_total == 0 {
                shield.local_up_total = conns.upload_total;
            }
            if shield.local_down_total == 0 {
                shield.local_down_total = conns.download_total;
            }

            if live_sample.is_none() {
                let now = Instant::now();
                if let Ok(mut guard) = PREV_TRAFFIC.lock() {
                    if let Some((prev_t, prev_up, prev_down)) = *guard {
                        let dt = now.duration_since(prev_t).as_secs_f64();
                        if dt >= 0.25 {
                            shield.local_up_bps =
                                (conns.upload_total.saturating_sub(prev_up) as f64 / dt) as u64;
                            shield.local_down_bps =
                                (conns.download_total.saturating_sub(prev_down) as f64 / dt) as u64;
                        }
                    }
                    *guard = Some((now, conns.upload_total, conns.download_total));
                }
            }

            shield.active_connections = conns.connections.len();
            let mut direct = 0usize;
            let mut proxy = 0usize;
            for c in &conns.connections {
                let is_direct = c
                    .chains
                    .first()
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

    shield
}
