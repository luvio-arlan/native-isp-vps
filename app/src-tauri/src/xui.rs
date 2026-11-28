use crate::models::{ClientQuota, NodeConfig, NodeSecrets, XuiStatus};
use crate::panel_tunnel::{self, PanelTransport};
use reqwest::Client;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CLIENTS_CACHE_TTL: Duration = Duration::from_secs(15);

type ClientsCacheMap = HashMap<String, (Instant, Vec<ClientQuota>)>;

static XUI_CLIENTS: OnceLock<Mutex<HashMap<String, Client>>> = OnceLock::new();
static WARM_SESSIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static COOKIE_ONLY_SESSIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static CLIENTS_CACHE: OnceLock<Mutex<ClientsCacheMap>> = OnceLock::new();

fn build_client() -> Result<Client, String> {
    Client::builder()
        .cookie_store(true)
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(6))
        .connect_timeout(Duration::from_secs(4))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| format!("http client error: {e}"))
}

pub fn evict_client(node_id: &str) {
    let prefix = format!("{}::", node_id);
    if let Some(map) = XUI_CLIENTS.get() {
        let mut guard = map.lock().unwrap_or_else(|p| p.into_inner());
        guard.retain(|k, _| !k.starts_with(&prefix));
    }
    if let Some(set) = WARM_SESSIONS.get() {
        let mut guard = set.lock().unwrap_or_else(|p| p.into_inner());
        guard.retain(|k| !k.starts_with(&prefix));
    }
    if let Some(set) = COOKIE_ONLY_SESSIONS.get() {
        let mut guard = set.lock().unwrap_or_else(|p| p.into_inner());
        guard.retain(|k| !k.starts_with(&prefix));
    }
    if let Some(cache) = CLIENTS_CACHE.get() {
        let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
        guard.remove(node_id);
    }
}

/// Returns the pooled client for `node_id::base`, evicting the node's clients/sessions for any
/// previous base (e.g. a re-established SSH tunnel on a new local port) so they never accumulate.
fn get_or_create_client(node_id: &str, cache_key: &str) -> Result<Client, String> {
    let map = XUI_CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(c) = guard.get(cache_key) {
        return Ok(c.clone());
    }
    let prefix = format!("{node_id}::");
    let is_stale = |k: &String| k.starts_with(&prefix) && k != cache_key;
    guard.retain(|k, _| !is_stale(k));
    for sessions in [&WARM_SESSIONS, &COOKIE_ONLY_SESSIONS] {
        if let Some(set) = sessions.get() {
            set.lock()
                .unwrap_or_else(|p| p.into_inner())
                .retain(|k| !is_stale(k));
        }
    }
    let client = build_client()?;
    guard.insert(cache_key.to_string(), client.clone());
    Ok(client)
}

/// Last known client quotas for `node_id`, regardless of cache age (used while the panel is down).
fn last_known_clients(node_id: &str) -> Vec<ClientQuota> {
    CLIENTS_CACHE
        .get()
        .and_then(|m| m.lock().ok().and_then(|g| g.get(node_id).map(|(_, v)| v.clone())))
        .unwrap_or_default()
}

fn is_session_warm(cache_key: &str) -> bool {
    WARM_SESSIONS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .ok()
        .is_some_and(|s| s.contains(cache_key))
}

fn mark_session_warm(cache_key: &str, warm: bool) {
    if let Ok(mut s) = WARM_SESSIONS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
    {
        if warm {
            s.insert(cache_key.to_string());
        } else {
            s.remove(cache_key);
        }
    }
}

async fn login_with_cookie(
    client: &Client,
    panel_url: &str,
    username: &str,
    password: &str,
) -> Result<(), String> {
    let login_url = format!("{}/login", panel_url.trim_end_matches('/'));
    let resp = client
        .post(&login_url)
        .form(&[("username", username), ("password", password)])
        .send()
        .await
        .map_err(|e| format!("3x-ui login request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("3x-ui login HTTP {}", resp.status()));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("3x-ui login JSON parse failed: {e}"))?;
    if body.get("success").and_then(|v| v.as_bool()) == Some(true) {
        Ok(())
    } else {
        let msg = body
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("invalid credentials");
        Err(format!("3x-ui login rejected: {msg}"))
    }
}

async fn fetch_authed_json(
    client: &Client,
    url: &str,
    method_post: bool,
    token: Option<&str>,
) -> Result<Value, String> {
    let mut req = if method_post {
        client.post(url)
    } else {
        client.get(url)
    };
    if let Some(t) = token {
        if !t.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", t));
        }
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("non-JSON response ({e})"))
}

fn parse_status_obj(status: &mut XuiStatus, val: &Value) {
    if let Some(obj) = val.get("obj") {
        status.connected = true;
        status.cpu_percent = obj.get("cpu").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if let Some(mem) = obj.get("mem") {
            status.mem_used = mem.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
            status.mem_total = mem.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        }
        if let Some(swap) = obj.get("swap") {
            status.swap_used = swap.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
            status.swap_total = swap.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        }
        if let Some(disk) = obj.get("disk") {
            status.disk_used = disk.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
            status.disk_total = disk.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        }
        status.uptime_secs = obj.get("uptime").and_then(|v| v.as_u64()).unwrap_or(0);
        if let Some(loads) = obj.get("loads").and_then(|v| v.as_array()) {
            for (idx, item) in loads.iter().take(3).enumerate() {
                status.loads[idx] = item.as_f64().unwrap_or(0.0);
            }
        }
        if let Some(xray) = obj.get("xray") {
            status.xray_state = xray
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            status.xray_version = xray
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }
        if let Some(net_io) = obj.get("netIO") {
            status.net_up_bps = net_io.get("up").and_then(|v| v.as_u64()).unwrap_or(0);
            status.net_down_bps = net_io.get("down").and_then(|v| v.as_u64()).unwrap_or(0);
        }
        if let Some(net_traffic) = obj.get("netTraffic") {
            status.net_sent_total = net_traffic
                .get("sent")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            status.net_recv_total = net_traffic
                .get("recv")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
        }
        status.tcp_count = obj.get("tcpCount").and_then(|v| v.as_u64()).unwrap_or(0);
        status.udp_count = obj.get("udpCount").and_then(|v| v.as_u64()).unwrap_or(0);
    }
}

fn parse_clients(
    node: &NodeConfig,
    online_res: Result<Value, String>,
    inbounds_res: Result<Value, String>,
) -> Vec<ClientQuota> {
    let mut online_set: HashSet<String> = HashSet::new();
    if let Ok(online_val) = online_res {
        if let Some(arr) = online_val.get("obj").and_then(|v| v.as_array()) {
            for email in arr {
                if let Some(s) = email.as_str() {
                    online_set.insert(s.to_string());
                }
            }
        }
    }

    let sub_prefix = node
        .sub_url
        .as_deref()
        .and_then(|url| url.rsplit_once('/').map(|(prefix, _)| prefix.to_string()));

    let mut clients: Vec<ClientQuota> = Vec::new();
    if let Ok(inbounds_val) = inbounds_res {
        if let Some(arr) = inbounds_val.get("obj").and_then(|v| v.as_array()) {
            for inbound in arr {
                let mut meta_map: HashMap<String, (Option<String>, u32)> = HashMap::new();
                if let Some(settings_str) = inbound.get("settings").and_then(|v| v.as_str()) {
                    if let Ok(settings_json) = serde_json::from_str::<Value>(settings_str) {
                        if let Some(c_arr) = settings_json.get("clients").and_then(|v| v.as_array())
                        {
                            for c in c_arr {
                                if let Some(email) = c.get("email").and_then(|v| v.as_str()) {
                                    let sub_id = c
                                        .get("subId")
                                        .and_then(|v| v.as_str())
                                        .filter(|s| !s.is_empty())
                                        .map(|s| s.to_string());
                                    let r_day = c
                                        .get("reset")
                                        .and_then(|v| v.as_u64())
                                        .map(|v| v as u32)
                                        .filter(|&d| d > 0)
                                        .unwrap_or(node.reset_day);
                                    meta_map.insert(email.to_string(), (sub_id, r_day));
                                }
                            }
                        }
                    }
                }

                if let Some(stats) = inbound.get("clientStats").and_then(|v| v.as_array()) {
                    for stat in stats {
                        let email = stat
                            .get("email")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let (sub_id, reset_day) = meta_map
                            .get(&email)
                            .cloned()
                            .unwrap_or((None, node.reset_day));
                        let sub_url = match (&sub_prefix, &sub_id) {
                            (Some(prefix), Some(sid)) => Some(format!("{}/{}", prefix, sid)),
                            _ => None,
                        };
                        let stat_reset = stat
                            .get("reset")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32)
                            .filter(|&d| d > 0)
                            .unwrap_or(reset_day);

                        clients.push(ClientQuota {
                            online: online_set.contains(&email),
                            email,
                            enable: stat.get("enable").and_then(|v| v.as_bool()).unwrap_or(true),
                            up_bytes: stat.get("up").and_then(|v| v.as_u64()).unwrap_or(0),
                            down_bytes: stat.get("down").and_then(|v| v.as_u64()).unwrap_or(0),
                            total_bytes: stat.get("total").and_then(|v| v.as_u64()).unwrap_or(0),
                            reset_day: stat_reset,
                            sub_id,
                            sub_url,
                        });
                    }
                }
            }
        }
    }
    clients
}

/// Fetches 3x-ui telemetry with tiered caching & 1-RTT concurrency:
/// - Hot ticks (`force_slow == false` with fresh client cache) issue only 1 request (`server/status`),
///   cutting remote VPS panel API load by 66%.
/// - Slow ticks on warm sessions run `status`, `onlines`, and `inbounds/list` concurrently in 1 RTT.
pub async fn fetch_xui_telemetry(
    node: &NodeConfig,
    secrets: &NodeSecrets,
    force_slow: bool,
) -> (XuiStatus, Vec<ClientQuota>) {
    let mut status = XuiStatus {
        xray_state: "unknown".to_string(),
        ..Default::default()
    };

    if node.panel_url.trim().is_empty() {
        status.error = Some("未配置 3x-ui 面板地址".to_string());
        return (status, Vec::new());
    }

    let has_token = secrets
        .api_token
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let has_creds = node
        .panel_username
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
        && secrets
            .panel_password
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());

    if !has_token && !has_creds {
        status.error = Some("缺少 3x-ui 面板密码/Token，请在「接入节点」重新填入密码".to_string());
        return (status, Vec::new());
    }

    let route = panel_tunnel::resolve(node).await;
    status.panel_transport = route.transport.as_str().to_string();
    if route.transport == PanelTransport::Paused {
        status.error =
            Some("SSH 加密隧道重连中：为防面板 Token 明文泄露，已暂停面板 API".to_string());
        return (status, last_known_clients(&node.id));
    }

    let base = route.base.as_str();
    let cache_key = format!("{}::{}", node.id, base);
    let client = match get_or_create_client(&node.id, &cache_key) {
        Ok(c) => c,
        Err(e) => {
            status.error = Some(e);
            return (status, Vec::new());
        }
    };

    let status_url = format!("{}/panel/api/server/status", base);
    let legacy_status_url = format!("{}/server/status", base);
    let inbounds_url = format!("{}/panel/api/inbounds/list", base);
    let onlines_url = format!("{}/panel/api/inbounds/onlines", base);

    let use_cookie_only = COOKIE_ONLY_SESSIONS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .ok()
        .is_some_and(|s| s.contains(&cache_key));

    let mut active_token = if use_cookie_only {
        None
    } else {
        secrets
            .api_token
            .as_deref()
            .filter(|s| !s.trim().is_empty())
    };

    let cached_clients = if !force_slow {
        CLIENTS_CACHE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .ok()
            .and_then(|m| m.get(&node.id).cloned())
            .filter(|(ts, _)| ts.elapsed() < CLIENTS_CACHE_TTL)
            .map(|(_, list)| list)
    } else {
        None
    };

    let need_clients = cached_clients.is_none();
    let warm = is_session_warm(&cache_key);

    // 1-RTT fast path: if session is warm and clients are due, fire all 3 requests concurrently!
    let (first_try, prefetched_clients) = if need_clients && warm {
        let (st, onl, inb) = tokio::join!(
            fetch_authed_json(&client, &status_url, false, active_token),
            fetch_authed_json(&client, &onlines_url, true, active_token),
            fetch_authed_json(&client, &inbounds_url, false, active_token)
        );
        (st, Some((onl, inb)))
    } else {
        (
            fetch_authed_json(&client, &status_url, false, active_token).await,
            None,
        )
    };

    let mut used_fallback_login = false;
    let status_json = match first_try {
        Ok(v) if v.get("success").and_then(|s| s.as_bool()) == Some(true) => {
            mark_session_warm(&cache_key, true);
            Ok(v)
        }
        first_err => {
            mark_session_warm(&cache_key, false);
            if let (Some(user), Some(pass)) = (
                node.panel_username.as_deref(),
                secrets.panel_password.as_deref(),
            ) {
                if let Err(login_err) = login_with_cookie(&client, base, user, pass).await {
                    Err(login_err)
                } else {
                    used_fallback_login = true;
                    active_token = None;
                    let retry = match fetch_authed_json(&client, &status_url, false, None).await {
                        Ok(v) if v.get("success").and_then(|s| s.as_bool()) == Some(true) => Ok(v),
                        _ => fetch_authed_json(&client, &legacy_status_url, true, None).await,
                    };
                    if retry.is_ok() {
                        mark_session_warm(&cache_key, true);
                        if let Ok(mut s) = COOKIE_ONLY_SESSIONS
                            .get_or_init(|| Mutex::new(HashSet::new()))
                            .lock()
                        {
                            s.insert(cache_key.clone());
                        }
                    }
                    retry
                }
            } else {
                match first_err {
                    Ok(v) => Err(v
                        .get("msg")
                        .and_then(|m| m.as_str())
                        .unwrap_or("3x-ui API 鉴权失败")
                        .to_string()),
                    Err(e) => Err(e),
                }
            }
        }
    };

    match status_json {
        Ok(val) => parse_status_obj(&mut status, &val),
        Err(e) => {
            status.error = Some(e);
            let clients = cached_clients.unwrap_or_else(|| last_known_clients(&node.id));
            return (status, clients);
        }
    }

    if let Some(fresh_cached) = cached_clients {
        return (status, fresh_cached);
    }

    let (online_res, inbounds_res) = match (prefetched_clients, used_fallback_login) {
        (Some(pair), false) => pair,
        _ => {
            tokio::join!(
                fetch_authed_json(&client, &onlines_url, true, active_token),
                fetch_authed_json(&client, &inbounds_url, false, active_token)
            )
        }
    };

    let clients = parse_clients(node, online_res, inbounds_res);
    if !clients.is_empty() {
        if let Ok(mut m) = CLIENTS_CACHE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            m.insert(node.id.clone(), (Instant::now(), clients.clone()));
        }
    }

    (status, clients)
}
