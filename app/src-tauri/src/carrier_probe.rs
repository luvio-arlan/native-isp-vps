use crate::models::{CarrierLineMetrics, CarrierQualitySnapshot, CarrierSample, NodeConfig};
use crate::ssh::{self, SshMode};
use crate::storage::{self, PersistedCarrierMap};
use chrono::Utc;
use std::collections::HashMap;

pub const MAX_CARRIER_SAMPLES: usize = 24;

#[derive(Debug, Clone, Copy)]
pub struct CarrierTarget {
    pub key: &'static str,
    pub label: &'static str,
    pub host: &'static str,
}

/// Verified direct China 3-carrier backbone endpoints (no overseas CDN).
pub const DEFAULT_CARRIERS: [CarrierTarget; 3] = [
    CarrierTarget {
        key: "ct",
        label: "电信",
        host: "202.96.209.133",
    },
    CarrierTarget {
        key: "cu",
        label: "联通",
        host: "119.6.6.6",
    },
    CarrierTarget {
        key: "cm",
        label: "移动",
        host: "211.136.112.50",
    },
];

/// Zero-dependency POSIX shell script executed over multiplexed SSH.
/// Sends 3 lightweight 64-byte ICMP probes per carrier in parallel (~576 bytes total, ~1s wall time),
/// leaving zero resident processes or open ports on the VPS.
const CARRIER_PROBE_SCRIPT: &str = r#"
probe_one() {
  k="$1"; ip="$2"
  out=$(ping -4 -n -c 3 -i 0.35 -W 2 "$ip" 2>/dev/null)
  loss=$(printf '%s\n' "$out" | awk -F',' '/packet loss/ {for(i=1;i<=NF;i++) if($i ~ /packet loss/) {gsub(/[^0-9.]/,"",$i); print $i; exit}}')
  avg=$(printf '%s\n' "$out" | awk -F'/' '/rtt|round-trip/ {print $5; exit}')
  [ -z "$loss" ] && loss="100"
  [ -z "$avg" ] && avg="NA"
  printf 'CARRIER=%s|LOSS=%s|RTT=%s\n' "$k" "$loss" "$avg"
}
probe_one ct 202.96.209.133 &
probe_one cu 119.6.6.6 &
probe_one cm 211.136.112.50 &
wait
"#;

#[derive(Debug, Default)]
pub struct CarrierHistoryStore {
    by_node: PersistedCarrierMap,
    dirty: bool,
}

impl CarrierHistoryStore {
    pub fn load() -> Self {
        let mut by_node = storage::load_carrier_history();
        for carriers in by_node.values_mut() {
            for samples in carriers.values_mut() {
                while samples.len() > MAX_CARRIER_SAMPLES {
                    samples.pop_front();
                }
            }
        }
        Self {
            by_node,
            dirty: false,
        }
    }

    pub fn record_batch(
        &mut self,
        node_id: &str,
        ts: i64,
        readings: &HashMap<String, (Option<f64>, f64)>,
    ) -> CarrierQualitySnapshot {
        let node_map = self.by_node.entry(node_id.to_string()).or_default();
        for carrier in DEFAULT_CARRIERS {
            let (rtt_ms, loss_pct) = readings
                .get(carrier.key)
                .copied()
                .unwrap_or((None, 100.0));
            let queue = node_map.entry(carrier.key.to_string()).or_default();
            queue.push_back(CarrierSample {
                ts,
                rtt_ms: rtt_ms.map(|v| (v * 10.0).round() / 10.0),
                loss_pct: (loss_pct.clamp(0.0, 100.0) * 10.0).round() / 10.0,
            });
            while queue.len() > MAX_CARRIER_SAMPLES {
                queue.pop_front();
            }
        }
        self.dirty = true;
        self.build_snapshot(node_id, ts, None)
    }

    pub fn build_snapshot(
        &self,
        node_id: &str,
        updated_at: i64,
        error: Option<String>,
    ) -> CarrierQualitySnapshot {
        let node_map = self.by_node.get(node_id);
        let carriers = DEFAULT_CARRIERS
            .iter()
            .map(|c| {
                let samples: Vec<CarrierSample> = node_map
                    .and_then(|m| m.get(c.key))
                    .map(|q| q.iter().cloned().collect())
                    .unwrap_or_default();
                summarize_carrier_line(*c, samples)
            })
            .collect();

        CarrierQualitySnapshot {
            node_id: node_id.to_string(),
            updated_at,
            carriers,
            error,
        }
    }

    pub fn get_cached_snapshot(&self, node_id: &str) -> Option<CarrierQualitySnapshot> {
        let node_map = self.by_node.get(node_id)?;
        let latest_ts = node_map
            .values()
            .filter_map(|q| q.back().map(|s| s.ts))
            .max()?;
        Some(self.build_snapshot(node_id, latest_ts, None))
    }

    pub fn remove_node(&mut self, node_id: &str) {
        if self.by_node.remove(node_id).is_some() {
            self.dirty = true;
        }
    }

    pub fn take_persist_snapshot(&mut self) -> Option<PersistedCarrierMap> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        Some(self.by_node.clone())
    }
}

pub fn summarize_carrier_line(
    target: CarrierTarget,
    samples: Vec<CarrierSample>,
) -> CarrierLineMetrics {
    let latest_rtt_ms = samples.iter().rev().find_map(|s| s.rtt_ms);
    let loss_pct = if samples.is_empty() {
        0.0
    } else {
        let sum: f64 = samples.iter().map(|s| s.loss_pct).sum();
        ((sum / samples.len() as f64) * 10.0).round() / 10.0
    };
    CarrierLineMetrics {
        key: target.key.to_string(),
        label: target.label.to_string(),
        target: target.host.to_string(),
        latest_rtt_ms,
        loss_pct,
        samples,
    }
}

pub fn parse_carrier_probe_output(stdout: &str) -> HashMap<String, (Option<f64>, f64)> {
    let mut out = HashMap::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("CARRIER=") else {
            continue;
        };
        let mut parts = rest.split('|');
        let Some(key) = parts.next().map(str::trim).filter(|s| !s.is_empty()) else {
            continue;
        };
        let mut loss_pct = 100.0;
        let mut rtt_ms = None;
        for part in parts {
            if let Some(v) = part.strip_prefix("LOSS=") {
                if let Ok(parsed) = v.trim().parse::<f64>() {
                    loss_pct = parsed.clamp(0.0, 100.0);
                }
            } else if let Some(v) = part.strip_prefix("RTT=") {
                if let Ok(parsed) = v.trim().parse::<f64>() {
                    if parsed.is_finite() && parsed > 0.0 {
                        rtt_ms = Some((parsed * 10.0).round() / 10.0);
                    }
                }
            }
        }
        out.insert(key.to_string(), (rtt_ms, loss_pct));
    }
    out
}

pub async fn probe_node_carriers(
    node: &NodeConfig,
) -> Result<HashMap<String, (Option<f64>, f64)>, String> {
    let mut cmd = ssh::command(
        node,
        SshMode::Multiplexed { persist_secs: 180 },
        &[],
        CARRIER_PROBE_SCRIPT,
    )?;
    let output = tokio::time::timeout(std::time::Duration::from_secs(10), cmd.output())
        .await
        .map_err(|_| "SSH 三网探测超时".to_string())?
        .map_err(|e| format!("SSH 进程启动失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "SSH 免密认证失败，无法采样三网质量".to_string()
        } else {
            stderr
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = parse_carrier_probe_output(&stdout);
    if parsed.is_empty() {
        return Err("未收到有效的三网延迟回包".to_string());
    }
    Ok(parsed)
}

pub fn build_demo_carrier_quality(tick: u64) -> CarrierQualitySnapshot {
    let now_ms = Utc::now().timestamp_millis();
    let specs: [(CarrierTarget, f64, f64, Option<usize>); 3] = [
        (DEFAULT_CARRIERS[0], 128.0, 4.5, None),
        (DEFAULT_CARRIERS[1], 135.0, 3.8, None),
        (DEFAULT_CARRIERS[2], 131.0, 5.2, Some(22)),
    ];

    let carriers = specs
        .into_iter()
        .map(|(target, base, amp, loss_slot)| {
            let samples: Vec<CarrierSample> = (0..MAX_CARRIER_SAMPLES)
                .map(|idx| {
                    let rev = (MAX_CARRIER_SAMPLES - 1 - idx) as i64;
                    let phase = (idx as f64 + tick as f64 * 0.35) * 0.55;
                    let rtt = ((base + phase.sin() * amp + (phase * 1.6).cos() * 2.1) * 10.0)
                        .round()
                        / 10.0;
                    let loss = if loss_slot == Some(idx) { 33.3 } else { 0.0 };
                    CarrierSample {
                        ts: now_ms - rev * 60_000,
                        rtt_ms: Some(rtt),
                        loss_pct: loss,
                    }
                })
                .collect();
            summarize_carrier_line(target, samples)
        })
        .collect();

    CarrierQualitySnapshot {
        node_id: "demo-la".to_string(),
        updated_at: now_ms,
        carriers,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_carrier_probe_lines() {
        let raw = "\
CARRIER=ct|LOSS=0|RTT=184.23
CARRIER=cu|LOSS=33.3333|RTT=165.4
CARRIER=cm|LOSS=100|RTT=NA
";
        let parsed = parse_carrier_probe_output(raw);
        assert_eq!(parsed.get("ct"), Some(&(Some(184.2), 0.0)));
        assert_eq!(parsed.get("cu"), Some(&(Some(165.4), 33.3333)));
        assert_eq!(parsed.get("cm"), Some(&(None, 100.0)));
    }

    #[test]
    fn carrier_store_caps_window_and_computes_loss_ratio() {
        let mut store = CarrierHistoryStore::default();
        for i in 0..(MAX_CARRIER_SAMPLES + 4) {
            let mut readings = HashMap::new();
            readings.insert("ct".to_string(), (Some(180.0 + i as f64), 0.0));
            readings.insert("cu".to_string(), (Some(165.0), 0.0));
            let cm_loss = if i == MAX_CARRIER_SAMPLES + 2 { 33.3 } else { 0.0 };
            readings.insert("cm".to_string(), (Some(179.0), cm_loss));
            store.record_batch("node-1", 1_700_000_000_000 + i as i64 * 60_000, &readings);
        }

        let snap = store.get_cached_snapshot("node-1").expect("cached snapshot");
        assert_eq!(snap.carriers.len(), 3);
        assert_eq!(snap.carriers[0].samples.len(), MAX_CARRIER_SAMPLES);
        assert_eq!(snap.carriers[0].loss_pct, 0.0);
        // 33.3 / 24 = 1.3875 -> 1.4%
        assert_eq!(snap.carriers[2].loss_pct, 1.4);
    }
}
