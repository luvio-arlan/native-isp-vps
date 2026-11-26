use crate::models::{ClientQuota, NodeConfig, NodeSecrets, XuiStatus};
use reqwest::Client;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

fn build_client() -> Result<Client, String> {
    Client::builder()
        .cookie_store(true)
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(6))
        .connect_timeout(Duration::from_secs(4))
        .build()
        .map_err(|e| format!("http client error: {e}"))
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

pub async fn fetch_xui_telemetry(
    node: &NodeConfig,
    secrets: &NodeSecrets,
) -> (XuiStatus, Vec<ClientQuota>) {
    let mut status = XuiStatus {
        xray_state: "unknown".to_string(),
        ..Default::default()
    };

    if node.panel_url.trim().is_empty() {
        status.error = Some("未配置 3x-ui 面板地址".to_string());
        return (status, Vec::new());
    }

    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            status.error = Some(e);
            return (status, Vec::new());
        }
    };

    let base = node.panel_url.trim_end_matches('/');
    let status_url = format!("{}/panel/api/server/status", base);
    let inbounds_url = format!("{}/panel/api/inbounds/list", base);
    let onlines_url = format!("{}/panel/api/inbounds/onlines", base);

    let mut active_token = secrets.api_token.as_deref();

    // Try Bearer token first if available
    let status_json = match fetch_authed_json(&client, &status_url, false, active_token).await {
        Ok(v) if v.get("success").and_then(|s| s.as_bool()) == Some(true) => Ok(v),
        first_err => {
            // Fall back to username/password cookie login
            if let (Some(user), Some(pass)) = (
                node.panel_username.as_deref(),
                secrets.panel_password.as_deref(),
            ) {
                if let Err(login_err) = login_with_cookie(&client, base, user, pass).await {
                    Err(login_err)
                } else {
                    active_token = None;
                    fetch_authed_json(&client, &status_url, false, None).await
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
        Ok(val) => {
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
        Err(e) => {
            status.error = Some(e);
            return (status, Vec::new());
        }
    }

    // Fetch online client emails
    let mut online_set: HashSet<String> = HashSet::new();
    if let Ok(online_val) = fetch_authed_json(&client, &onlines_url, true, active_token).await {
        if let Some(arr) = online_val.get("obj").and_then(|v| v.as_array()) {
            for email in arr {
                if let Some(s) = email.as_str() {
                    online_set.insert(s.to_string());
                }
            }
        }
    }

    // Extract base subscription prefix from node.sub_url if available
    let sub_prefix = node.sub_url.as_deref().and_then(|url| {
        url.rsplit_once('/').map(|(prefix, _)| prefix.to_string())
    });

    // Fetch inbounds & per-client quotas
    let mut clients: Vec<ClientQuota> = Vec::new();
    if let Ok(inbounds_val) = fetch_authed_json(&client, &inbounds_url, false, active_token).await {
        if let Some(arr) = inbounds_val.get("obj").and_then(|v| v.as_array()) {
            for inbound in arr {
                // Map email -> (subId, resetDay) from inbound.settings JSON string
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

    (status, clients)
}
