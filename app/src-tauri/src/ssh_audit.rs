use crate::models::{AuditItem, AuditReport, NodeConfig};
use chrono::Utc;
use std::collections::HashMap;
use tokio::process::Command;

const AUDIT_SCRIPT: &str = r#"
echo "QDISC=$(tc qdisc show 2>/dev/null | head -n 4 | tr '\n' ';')"
echo "CC=$(sysctl -n net.ipv4.tcp_congestion_control 2>/dev/null)"
echo "TFO=$(sysctl -n net.ipv4.tcp_fastopen 2>/dev/null)"
echo "ECN=$(sysctl -n net.ipv4.tcp_ecn 2>/dev/null)"
echo "KEEPALIVE=$(sysctl -n net.ipv4.tcp_keepalive_time 2>/dev/null)"
echo "XUI=$(systemctl is-active x-ui 2>/dev/null)"
echo "UFW=$(ufw status 2>/dev/null | head -n 1)"
echo "SWAP=$(swapon --show --noheadings 2>/dev/null | wc -l)"
echo "F2B_CUR=$(fail2ban-client status sshd 2>/dev/null | awk -F: '/Currently banned/ {gsub(/ /,"",$2); print $2}')"
echo "F2B_TOT=$(fail2ban-client status sshd 2>/dev/null | awk -F: '/Total banned/ {gsub(/ /,"",$2); print $2}')"
"#;

pub async fn run_ssh_audit(node: &NodeConfig) -> AuditReport {
    let mut cmd = Command::new("ssh");
    cmd.arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=7")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-p")
        .arg(node.ssh_port.to_string());

    if let Some(key_path) = &node.ssh_key_path {
        if !key_path.trim().is_empty() {
            cmd.arg("-i").arg(key_path);
        }
    }

    cmd.arg(format!("{}@{}", node.ssh_user, node.host))
        .arg(AUDIT_SCRIPT);

    let output = match tokio::time::timeout(std::time::Duration::from_secs(12), cmd.output()).await
    {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            return AuditReport {
                node_id: node.id.clone(),
                checked_at: Utc::now().to_rfc3339(),
                score: 0,
                fail2ban_banned_total: None,
                fail2ban_banned_current: None,
                items: Vec::new(),
                error: Some(format!("SSH 启动失败: {e}")),
            }
        }
        Err(_) => {
            return AuditReport {
                node_id: node.id.clone(),
                checked_at: Utc::now().to_rfc3339(),
                score: 0,
                fail2ban_banned_total: None,
                fail2ban_banned_current: None,
                items: Vec::new(),
                error: Some("SSH 连接超时（请确认本机已配置免密公钥或 ssh-agent）".to_string()),
            }
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return AuditReport {
            node_id: node.id.clone(),
            checked_at: Utc::now().to_rfc3339(),
            score: 0,
            fail2ban_banned_total: None,
            fail2ban_banned_current: None,
            items: Vec::new(),
            error: Some(if stderr.is_empty() {
                "SSH 免密认证未通过：可在节点设置中指定私钥路径，或让 AI 写入公钥".to_string()
            } else {
                stderr
            }),
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut kv: HashMap<String, String> = HashMap::new();
    for line in stdout.lines() {
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let qdisc = kv.get("QDISC").cloned().unwrap_or_default();
    let cc = kv.get("CC").cloned().unwrap_or_default();
    let tfo = kv.get("TFO").cloned().unwrap_or_default();
    let ecn = kv.get("ECN").cloned().unwrap_or_default();
    let keepalive = kv.get("KEEPALIVE").cloned().unwrap_or_default();
    let xui = kv.get("XUI").cloned().unwrap_or_default();
    let ufw = kv.get("UFW").cloned().unwrap_or_default();
    let swap_cnt: u32 = kv.get("SWAP").and_then(|s| s.parse().ok()).unwrap_or(0);
    let f2b_cur = kv.get("F2B_CUR").and_then(|s| s.parse::<u64>().ok());
    let f2b_tot = kv.get("F2B_TOT").and_then(|s| s.parse::<u64>().ok());

    let keepalive_num: u64 = keepalive.parse().unwrap_or(7200);

    let items = vec![
        AuditItem {
            key: "qdisc".into(),
            label: "网卡队列 FQ (Fair Queue)".into(),
            passed: qdisc.contains("qdisc fq "),
            actual: if qdisc.contains("qdisc fq ") { "fq".into() } else { qdisc },
            expected: "fq".into(),
            severity: "critical".into(),
        },
        AuditItem {
            key: "bbr".into(),
            label: "TCP 拥塞控制算法".into(),
            passed: cc == "bbr",
            actual: cc,
            expected: "bbr".into(),
            severity: "critical".into(),
        },
        AuditItem {
            key: "tfo".into(),
            label: "TCP Fast Open (防 SYN 黑洞)".into(),
            passed: tfo == "0",
            actual: tfo,
            expected: "0 (禁用)".into(),
            severity: "warn".into(),
        },
        AuditItem {
            key: "ecn".into(),
            label: "TCP ECN (防运营商丢包)".into(),
            passed: ecn == "0",
            actual: ecn,
            expected: "0 (禁用)".into(),
            severity: "warn".into(),
        },
        AuditItem {
            key: "keepalive".into(),
            label: "TCP Keepalive 保活周期".into(),
            passed: keepalive_num <= 180,
            actual: format!("{}s", keepalive_num),
            expected: "120s".into(),
            severity: "warn".into(),
        },
        AuditItem {
            key: "xui".into(),
            label: "3x-ui / Xray 守护进程".into(),
            passed: xui == "active",
            actual: xui,
            expected: "active".into(),
            severity: "critical".into(),
        },
        AuditItem {
            key: "ufw".into(),
            label: "UFW 最小暴露防火墙".into(),
            passed: ufw.to_lowercase().contains("active") && !ufw.to_lowercase().contains("inactive"),
            actual: ufw,
            expected: "Status: active".into(),
            severity: "critical".into(),
        },
        AuditItem {
            key: "swap".into(),
            label: "小内存 Swap 防 OOM 保护".into(),
            passed: swap_cnt > 0,
            actual: if swap_cnt > 0 { "已启用".into() } else { "未配置".into() },
            expected: ">= 1GB Swap".into(),
            severity: "info".into(),
        },
        AuditItem {
            key: "fail2ban".into(),
            label: "Fail2ban SSH 防爆破".into(),
            passed: f2b_tot.is_some(),
            actual: match (f2b_cur, f2b_tot) {
                (Some(c), Some(t)) => format!("运行中（当前封禁 {} / 累计 {}）", c, t),
                _ => "未检测到 sshd jail".into(),
            },
            expected: "sshd jail active".into(),
            severity: "warn".into(),
        },
    ];

    let passed_cnt = items.iter().filter(|i| i.passed).count() as f64;
    let score = ((passed_cnt / items.len() as f64) * 100.0).round() as u32;

    AuditReport {
        node_id: node.id.clone(),
        checked_at: Utc::now().to_rfc3339(),
        score,
        fail2ban_banned_total: f2b_tot,
        fail2ban_banned_current: f2b_cur,
        items,
        error: None,
    }
}
