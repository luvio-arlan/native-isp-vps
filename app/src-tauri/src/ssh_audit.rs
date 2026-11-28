use crate::models::{AuditItem, AuditReport, NodeConfig};
use crate::ssh::{self, SshMode};
use chrono::Utc;
use std::collections::HashMap;
use std::time::Duration;

/// Read-only POSIX shell audit executed over the shared SSH ControlMaster (zero VPS agents).
/// The DNS block probes every `/etc/resolv.conf` upstream: a single unreachable server combined
/// with `rotate` stalls ~1/3 of lookups by the resolver timeout, and Xray REALITY resolves its
/// camouflage target on every new connection, so that stall lands on proxied handshakes.
const AUDIT_SCRIPT: &str = r#"
echo "QDISC=$(tc qdisc show 2>/dev/null | head -n 4 | tr '\n' ';')"
echo "QDISC_SVC=$(systemctl is-enabled vps-fq-qdisc.service 2>/dev/null)"
echo "CC=$(sysctl -n net.ipv4.tcp_congestion_control 2>/dev/null)"
echo "TFO=$(sysctl -n net.ipv4.tcp_fastopen 2>/dev/null)"
echo "ECN=$(sysctl -n net.ipv4.tcp_ecn 2>/dev/null)"
echo "KEEPALIVE=$(sysctl -n net.ipv4.tcp_keepalive_time 2>/dev/null)"
echo "XUI=$(systemctl is-active x-ui 2>/dev/null)"
echo "UFW=$(ufw status 2>/dev/null | head -n 1)"
echo "SWAP=$(swapon --show --noheadings 2>/dev/null | wc -l)"
echo "F2B_CUR=$(fail2ban-client status sshd 2>/dev/null | awk -F: '/Currently banned/ {gsub(/ /,"",$2); print $2}')"
echo "F2B_TOT=$(fail2ban-client status sshd 2>/dev/null | awk -F: '/Total banned/ {gsub(/ /,"",$2); print $2}')"
DNS_BAD=""; DNS_TOTAL=0
for ns in $(awk '/^nameserver/ {print $2}' /etc/resolv.conf 2>/dev/null); do
  DNS_TOTAL=$((DNS_TOTAL + 1))
  if command -v python3 >/dev/null 2>&1; then
    python3 -c '
import socket, struct, sys
ns = sys.argv[1]
q = struct.pack(">HHHHHH", 0x4E49, 0x0100, 1, 0, 0, 0) + b"\x03www\x06google\x03com\x00" + struct.pack(">HH", 1, 1)
for _ in range(2):
    s = socket.socket(socket.AF_INET6 if ":" in ns else socket.AF_INET, socket.SOCK_DGRAM)
    s.settimeout(1)
    try:
        s.sendto(q, (ns, 53)); s.recv(512); sys.exit(0)
    except OSError:
        pass
    finally:
        s.close()
sys.exit(1)
' "$ns" >/dev/null 2>&1 || DNS_BAD="$DNS_BAD $ns"
  elif command -v dig >/dev/null 2>&1; then
    dig +time=1 +tries=2 @"$ns" www.google.com A >/dev/null 2>&1 || DNS_BAD="$DNS_BAD $ns"
  else
    DNS_BAD="unknown"; break
  fi
done
echo "DNS_TOTAL=$DNS_TOTAL"
echo "DNS_BAD=$DNS_BAD"
echo "DNS_ROTATE=$(grep -c '^options.*rotate' /etc/resolv.conf 2>/dev/null)"
"#;

fn failed_report(node: &NodeConfig, error: String) -> AuditReport {
    AuditReport {
        node_id: node.id.clone(),
        checked_at: Utc::now().to_rfc3339(),
        score: 0,
        fail2ban_banned_total: None,
        fail2ban_banned_current: None,
        items: Vec::new(),
        error: Some(error),
    }
}

fn item(key: &str, label: &str, passed: bool, actual: String, expected: &str, severity: &str) -> AuditItem {
    AuditItem {
        key: key.into(),
        label: label.into(),
        passed,
        actual,
        expected: expected.into(),
        severity: severity.into(),
    }
}

/// DNS upstream health from the audit's `DNS_*` keys: `(passed, actual)`.
fn dns_health(kv: &HashMap<String, String>) -> (bool, String) {
    let total: u32 = kv.get("DNS_TOTAL").and_then(|s| s.parse().ok()).unwrap_or(0);
    let bad = kv.get("DNS_BAD").map(|s| s.trim()).unwrap_or_default();
    let rotate = kv.get("DNS_ROTATE").is_some_and(|s| s.trim() != "0" && !s.trim().is_empty());
    if bad == "unknown" {
        return (true, "未探测（VPS 缺少 python3 / dig）".into());
    }
    if total == 0 {
        return (false, "resolv.conf 未配置 nameserver".into());
    }
    if bad.is_empty() {
        return (true, format!("{total} 个上游均可达"));
    }
    let rotate_note = if rotate { " · rotate 放大为约 1/3 解析卡顿" } else { "" };
    (false, format!("不可达：{bad}{rotate_note}"))
}

fn build_items(kv: &HashMap<String, String>) -> Vec<AuditItem> {
    let get = |k: &str| kv.get(k).cloned().unwrap_or_default();
    let qdisc = get("QDISC");
    let has_fq = qdisc.contains("qdisc fq ");
    let keepalive: u64 = get("KEEPALIVE").parse().unwrap_or(7200);
    let ufw = get("UFW").to_lowercase();
    let swap_cnt: u32 = get("SWAP").parse().unwrap_or(0);
    let f2b_cur = get("F2B_CUR").parse::<u64>().ok();
    let f2b_tot = get("F2B_TOT").parse::<u64>().ok();
    let (dns_ok, dns_actual) = dns_health(kv);

    vec![
        item(
            "qdisc",
            "网卡队列 FQ (Fair Queue)",
            has_fq,
            match (has_fq, get("QDISC_SVC") == "enabled") {
                (true, true) => "fq (已持久化)".into(),
                (true, false) => "fq".into(),
                _ => qdisc,
            },
            "fq",
            "critical",
        ),
        item("bbr", "TCP 拥塞控制算法", get("CC") == "bbr", get("CC"), "bbr", "critical"),
        item("tfo", "TCP Fast Open (防 SYN 黑洞)", get("TFO") == "0", get("TFO"), "0 (禁用)", "warn"),
        item("ecn", "TCP ECN (防运营商丢包)", get("ECN") == "0", get("ECN"), "0 (禁用)", "warn"),
        item(
            "keepalive",
            "TCP Keepalive 保活周期",
            keepalive <= 180,
            format!("{keepalive}s"),
            "120s",
            "warn",
        ),
        item("xui", "3x-ui / Xray 守护进程", get("XUI") == "active", get("XUI"), "active", "critical"),
        item(
            "ufw",
            "UFW 最小暴露防火墙",
            ufw.contains("active") && !ufw.contains("inactive"),
            get("UFW"),
            "Status: active",
            "critical",
        ),
        item(
            "dns",
            "DNS 上游可达性 (防握手 2s 卡顿)",
            dns_ok,
            dns_actual,
            "仅保留可达上游 · timeout:1",
            "critical",
        ),
        item(
            "swap",
            "小内存 Swap 防 OOM 保护",
            swap_cnt > 0,
            if swap_cnt > 0 { "已启用".into() } else { "未配置".into() },
            ">= 1GB Swap",
            "info",
        ),
        item(
            "fail2ban",
            "Fail2ban SSH 防爆破",
            f2b_tot.is_some(),
            match (f2b_cur, f2b_tot) {
                (Some(c), Some(t)) => format!("运行中（当前封禁 {c} / 累计 {t}）"),
                _ => "未检测到 sshd jail".into(),
            },
            "sshd jail active",
            "warn",
        ),
    ]
}

pub async fn run_ssh_audit(node: &NodeConfig) -> AuditReport {
    let mut cmd = match ssh::command(node, SshMode::Multiplexed { persist_secs: 180 }, &[], AUDIT_SCRIPT) {
        Ok(cmd) => cmd,
        Err(e) => return failed_report(node, e),
    };
    let output = match tokio::time::timeout(Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return failed_report(node, format!("SSH 启动失败: {e}")),
        Err(_) => {
            return failed_report(
                node,
                "SSH 连接超时（请确认本机已配置免密公钥或 ssh-agent）".to_string(),
            )
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return failed_report(
            node,
            if stderr.is_empty() {
                "SSH 免密认证未通过：可在节点设置中指定私钥路径，或让 AI 写入公钥".to_string()
            } else {
                stderr
            },
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let kv: HashMap<String, String> = stdout
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let items = build_items(&kv);
    let passed_cnt = items.iter().filter(|i| i.passed).count() as f64;
    let score = ((passed_cnt / items.len() as f64) * 100.0).round() as u32;

    AuditReport {
        node_id: node.id.clone(),
        checked_at: Utc::now().to_rfc3339(),
        score,
        fail2ban_banned_total: kv.get("F2B_TOT").and_then(|s| s.parse().ok()),
        fail2ban_banned_current: kv.get("F2B_CUR").and_then(|s| s.parse().ok()),
        items,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn dns_health_flags_unreachable_upstream_and_rotate() {
        let (ok, actual) = dns_health(&kv(&[("DNS_TOTAL", "3"), ("DNS_BAD", " 1.0.0.1"), ("DNS_ROTATE", "1")]));
        assert!(!ok);
        assert!(actual.contains("1.0.0.1") && actual.contains("rotate"));

        let (ok, actual) = dns_health(&kv(&[("DNS_TOTAL", "2"), ("DNS_BAD", ""), ("DNS_ROTATE", "0")]));
        assert!(ok);
        assert!(actual.contains('2'));

        assert!(dns_health(&kv(&[("DNS_TOTAL", "1"), ("DNS_BAD", "unknown")])).0);
        assert!(!dns_health(&kv(&[("DNS_TOTAL", "0"), ("DNS_BAD", "")])).0);
    }

    #[test]
    fn audit_items_cover_kernel_firewall_and_dns_baseline() {
        let items = build_items(&kv(&[
            ("QDISC", "qdisc fq 8001: root;"),
            ("QDISC_SVC", "enabled"),
            ("CC", "bbr"),
            ("TFO", "0"),
            ("ECN", "0"),
            ("KEEPALIVE", "120"),
            ("XUI", "active"),
            ("UFW", "Status: active"),
            ("SWAP", "1"),
            ("F2B_CUR", "0"),
            ("F2B_TOT", "12"),
            ("DNS_TOTAL", "2"),
            ("DNS_BAD", ""),
        ]));
        assert_eq!(items.len(), 10);
        assert!(items.iter().all(|i| i.passed), "{items:?}");
        assert_eq!(items[0].actual, "fq (已持久化)");
    }
}
