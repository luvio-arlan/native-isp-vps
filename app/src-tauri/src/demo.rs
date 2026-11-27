use crate::metrics::MAX_FINE_HISTORY;
use crate::models::{
    AuditItem, AuditReport, ClientQuota, HistoryBucket, LocalShieldStatus, TelemetryPoint,
    TelemetrySnapshot, XuiStatus,
};
use crate::state::CockpitState;
use chrono::Utc;
use std::collections::VecDeque;

pub async fn build_demo_snapshot(
    state: &CockpitState,
    real_shield: LocalShieldStatus,
) -> TelemetrySnapshot {
    let tick = {
        let mut t = state.demo_tick.write().await;
        *t += 1;
        *t
    };

    let phase = tick as f64 * 0.45;
    let rtt = (134.0 + phase.sin() * 7.5 + (phase * 1.7).cos() * 3.2).round();
    let vps_down = (4_200_000.0
        + (phase * 0.8).sin() * 2_400_000.0
        + (phase * 2.1).cos() * 900_000.0)
        .max(240_000.0) as u64;
    let vps_up = (780_000.0 + (phase * 0.6).cos() * 360_000.0).max(65_000.0) as u64;

    let local_down = if real_shield.mihomo_connected {
        real_shield.local_down_bps
    } else {
        (vps_down as f64 * 0.88) as u64
    };
    let local_up = if real_shield.mihomo_connected {
        real_shield.local_up_bps
    } else {
        (vps_up as f64 * 0.86) as u64
    };

    let now_ms = Utc::now().timestamp_millis();
    let seed = {
        let mut init = VecDeque::with_capacity(MAX_FINE_HISTORY);
        for i in (1..=36).rev() {
            let p = (36 - i) as f64 * 0.45;
            let d = (3_800_000.0 + (p * 0.8).sin() * 2_200_000.0).max(200_000.0) as u64;
            let u = (700_000.0 + (p * 0.6).cos() * 320_000.0).max(50_000.0) as u64;
            init.push_back(TelemetryPoint {
                ts: now_ms - (i as i64) * 2000,
                rtt_ms: Some((134.0 + p.sin() * 6.5).round()),
                vps_down_bps: d,
                vps_up_bps: u,
                local_down_bps: (d as f64 * 0.9) as u64,
                local_up_bps: (u as f64 * 0.85) as u64,
            });
        }
        init
    };

    let (history_vec, _, _) = {
        let mut store = state.history.write().await;
        store.push_fine_only(
            "demo-la",
            TelemetryPoint {
                ts: now_ms,
                rtt_ms: Some(rtt),
                vps_down_bps: vps_down,
                vps_up_bps: vps_up,
                local_down_bps: local_down,
                local_up_bps: local_up,
            },
            Some(seed),
        )
    };

    let mut shield = real_shield;
    if !shield.mihomo_connected {
        shield.mode = "rule".to_string();
        shield.local_down_bps = local_down;
        shield.local_up_bps = local_up;
        shield.local_up_total = 1_240_000_000;
        shield.local_down_total = 6_820_000_000;
        shield.active_connections = 34;
        shield.proxy_connections = 21;
        shield.direct_connections = 13;
        shield.ipv6_disabled = Some(true);
        shield.tun_enabled = Some(true);
        shield.tun_stack = Some("mixed".to_string());
        shield.fake_ip_enabled = Some(true);
        shield.quic_blocked = Some(true);
        shield.tcp_concurrent = Some(true);
        shield.exit_ipv4 = Some("38.55.142.88".to_string());
        shield.ipv6_leak = Some(false);
        shield.matches_vps_ip = Some(true);
    }

    let xui = XuiStatus {
        connected: true,
        error: None,
        cpu_percent: (9.4 + (phase * 0.5).sin() * 3.8).max(2.1),
        mem_used: 412 * 1024 * 1024,
        mem_total: 1024 * 1024 * 1024,
        swap_used: 68 * 1024 * 1024,
        swap_total: 1024 * 1024 * 1024,
        disk_used: 3_480_000_000,
        disk_total: 20_000_000_000,
        uptime_secs: 18 * 86400 + 7 * 3600 + 42 * 60 + tick * 2,
        loads: [0.14, 0.19, 0.16],
        xray_state: "running".to_string(),
        xray_version: "25.3.6".to_string(),
        net_up_bps: vps_up,
        net_down_bps: vps_down,
        net_sent_total: 198_420_000_000,
        net_recv_total: 184_110_000_000,
        tcp_count: 46,
        udp_count: 6,
    };

    let clients = vec![ClientQuota {
        email: "Shared".to_string(),
        enable: true,
        up_bytes: 35_800_000_000,
        down_bytes: 186_200_000_000,
        total_bytes: 2000 * 1024 * 1024 * 1024,
        reset_day: 25,
        online: true,
        sub_id: Some("demo7a9f2c8e1b4d".to_string()),
        sub_url: Some("http://38.55.142.88:2096/sub/demo7a9f2c8e1b4d".to_string()),
    }];

    TelemetrySnapshot {
        node_id: "demo-la".to_string(),
        timestamp: now_ms,
        health_score: 98,
        health_status: "demo".to_string(),
        rtt_ms: Some(rtt),
        jitter_ms: 3.4,
        packet_loss_pct: 0.0,
        xui,
        clients,
        local_shield: shield,
        history: history_vec,
    }
}

pub fn build_demo_history_24h() -> Vec<HistoryBucket> {
    let now_min = (Utc::now().timestamp_millis() / 60_000) * 60_000;
    // Generate 96 points (every 15 minutes over 24h) showing gentle evening peak bump
    (0..96)
        .rev()
        .map(|i| {
            let ts = now_min - (i as i64) * 15 * 60_000;
            let p = i as f64 * 0.25;
            let peak = if (12..=28).contains(&i) { 14.0 } else { 0.0 };
            let avg = (133.0 + peak + p.sin() * 4.5).round();
            let p95 = (avg + 8.0 + (p * 1.4).cos().abs() * 6.0).round();
            HistoryBucket {
                ts,
                avg_rtt_ms: Some(avg),
                p95_rtt_ms: Some(p95),
                loss_pct: 0.0,
                avg_local_down_bps: (2_600_000.0 + p.cos().abs() * 3_200_000.0) as u64,
                avg_vps_down_bps: (2_900_000.0 + p.cos().abs() * 3_500_000.0) as u64,
                samples: 30,
            }
        })
        .collect()
}

pub fn build_demo_audit_report() -> AuditReport {
    AuditReport {
        node_id: "demo-la".into(),
        checked_at: Utc::now().to_rfc3339(),
        score: 100,
        fail2ban_banned_total: Some(148),
        fail2ban_banned_current: Some(2),
        items: vec![
            AuditItem {
                key: "qdisc".into(),
                label: "网卡队列 FQ (Fair Queue)".into(),
                passed: true,
                actual: "fq (已持久化)".into(),
                expected: "fq".into(),
                severity: "critical".into(),
            },
            AuditItem {
                key: "bbr".into(),
                label: "TCP 拥塞控制算法".into(),
                passed: true,
                actual: "bbr".into(),
                expected: "bbr".into(),
                severity: "critical".into(),
            },
            AuditItem {
                key: "tfo".into(),
                label: "TCP Fast Open (防 SYN 黑洞)".into(),
                passed: true,
                actual: "0".into(),
                expected: "0 (禁用)".into(),
                severity: "warn".into(),
            },
            AuditItem {
                key: "ecn".into(),
                label: "TCP ECN (防运营商丢包)".into(),
                passed: true,
                actual: "0".into(),
                expected: "0 (禁用)".into(),
                severity: "warn".into(),
            },
            AuditItem {
                key: "keepalive".into(),
                label: "TCP Keepalive 保活周期".into(),
                passed: true,
                actual: "120s".into(),
                expected: "120s".into(),
                severity: "warn".into(),
            },
            AuditItem {
                key: "xui".into(),
                label: "3x-ui / Xray 守护进程".into(),
                passed: true,
                actual: "active".into(),
                expected: "active".into(),
                severity: "critical".into(),
            },
            AuditItem {
                key: "ufw".into(),
                label: "UFW 最小暴露防火墙".into(),
                passed: true,
                actual: "Status: active".into(),
                expected: "Status: active".into(),
                severity: "critical".into(),
            },
            AuditItem {
                key: "swap".into(),
                label: "小内存 Swap 防 OOM 保护".into(),
                passed: true,
                actual: "已启用 (1GB)".into(),
                expected: ">= 1GB Swap".into(),
                severity: "info".into(),
            },
            AuditItem {
                key: "fail2ban".into(),
                label: "Fail2ban SSH 防爆破".into(),
                passed: true,
                actual: "运行中（当前封禁 2 / 累计 148）".into(),
                expected: "sshd jail active".into(),
                severity: "warn".into(),
            },
        ],
        error: None,
    }
}
