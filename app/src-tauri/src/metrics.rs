use crate::models::{HistoryBucket, LocalShieldStatus, TelemetryPoint, XuiStatus};
use crate::storage;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

pub const MAX_FINE_HISTORY: usize = 60;
pub const MAX_24H_BUCKETS: usize = 1440;
const BUCKET_MS: i64 = 60_000;
const FLUSH_INTERVAL: Duration = Duration::from_secs(60);

/// Returns a smoothed RTT (median of the latest up to 3 valid samples) so single-packet spikes
/// do not cause erratic health score jumps. Reports `None` only when the link has no recent reply.
pub fn compute_smoothed_rtt(history: &[TelemetryPoint], latest: Option<f64>) -> Option<f64> {
    let mut recent: Vec<f64> = history
        .iter()
        .rev()
        .take(3)
        .filter_map(|p| p.rtt_ms)
        .collect();
    if recent.is_empty() {
        return latest;
    }
    // If the last two consecutive probes timed out, surface offline immediately
    let last_two_missing = history.len() >= 2
        && history.iter().rev().take(2).all(|p| p.rtt_ms.is_none());
    if last_two_missing {
        return None;
    }
    recent.sort_by(|a, b| a.total_cmp(b));
    Some(recent[recent.len() / 2])
}

pub fn compute_health_score(
    rtt_ms: Option<f64>,
    loss_pct: f64,
    xui: &XuiStatus,
    shield: &LocalShieldStatus,
) -> (u32, String) {
    let mut score: i32 = 100;

    // Smooth piecewise RTT penalty (no abrupt step cliffs around 200ms/280ms)
    if rtt_ms.is_none() {
        score -= 40;
    } else if let Some(r) = rtt_ms {
        if r > 360.0 {
            let extra = (((r - 360.0) / 120.0) * 10.0).min(12.0).round() as i32;
            score -= 16 + extra;
        } else if r > 180.0 {
            let penalty = (((r - 180.0) / 180.0) * 16.0).round() as i32;
            score -= penalty;
        }
    }
    if loss_pct > 5.0 {
        score -= 20;
    } else if loss_pct > 0.0 {
        score -= (loss_pct * 2.0).ceil().clamp(2.0, 10.0) as i32;
    }

    if !xui.connected {
        score -= 18;
    } else {
        if xui.xray_state != "running" {
            score -= 25;
        }
        if xui.cpu_percent > 85.0 {
            score -= 10;
        }
        if xui.mem_total > 0 && (xui.mem_used as f64 / xui.mem_total as f64) > 0.90 {
            score -= 10;
        }
    }

    if shield.ipv6_leak == Some(true) {
        score -= 15;
    }
    if shield.mihomo_connected && shield.quic_blocked == Some(false) {
        score -= 5;
    }

    let clamped = score.clamp(0, 100) as u32;
    let status = if clamped >= 85 {
        "optimal"
    } else if clamped >= 60 {
        "degraded"
    } else {
        "critical"
    };
    (clamped, status.to_string())
}

pub fn compute_rtt_metrics(history: &VecDeque<TelemetryPoint>) -> (f64, f64) {
    if history.is_empty() {
        return (0.0, 0.0);
    }
    let total = history.len() as f64;
    let mut missing = 0.0;
    let mut rtts: Vec<f64> = Vec::with_capacity(history.len());
    for pt in history {
        match pt.rtt_ms {
            Some(v) => rtts.push(v),
            None => missing += 1.0,
        }
    }
    let loss_pct = ((missing / total) * 100.0 * 10.0).round() / 10.0;
    let jitter = if rtts.len() >= 2 {
        let mut diffs = 0.0;
        for w in rtts.windows(2) {
            diffs += (w[1] - w[0]).abs();
        }
        ((diffs / (rtts.len() - 1) as f64) * 10.0).round() / 10.0
    } else {
        0.0
    };
    (jitter, loss_pct)
}

#[derive(Default)]
struct MinuteAccumulator {
    minute_ts: i64,
    rtts: Vec<f64>,
    missing: u32,
    samples: u32,
    sum_local_down: u128,
    sum_vps_down: u128,
}

impl MinuteAccumulator {
    fn new(minute_ts: i64) -> Self {
        Self {
            minute_ts,
            ..Default::default()
        }
    }

    fn push(&mut self, pt: &TelemetryPoint) {
        self.samples += 1;
        match pt.rtt_ms {
            Some(v) => self.rtts.push(v),
            None => self.missing += 1,
        }
        self.sum_local_down += pt.local_down_bps as u128;
        self.sum_vps_down += pt.vps_down_bps as u128;
    }

    fn to_bucket(&self) -> HistoryBucket {
        let samples = self.samples.max(1);
        let (avg_rtt_ms, p95_rtt_ms) = if self.rtts.is_empty() {
            (None, None)
        } else {
            let mut sorted = self.rtts.clone();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let avg = sorted.iter().sum::<f64>() / sorted.len() as f64;
            let p95_idx = (((sorted.len() as f64) * 0.95).ceil() as usize)
                .saturating_sub(1)
                .min(sorted.len() - 1);
            (
                Some((avg * 10.0).round() / 10.0),
                Some((sorted[p95_idx] * 10.0).round() / 10.0),
            )
        };
        let loss_pct = ((self.missing as f64 / samples as f64) * 100.0 * 10.0).round() / 10.0;
        HistoryBucket {
            ts: self.minute_ts,
            avg_rtt_ms,
            p95_rtt_ms,
            loss_pct,
            avg_local_down_bps: (self.sum_local_down / samples as u128) as u64,
            avg_vps_down_bps: (self.sum_vps_down / samples as u128) as u64,
            samples: self.samples,
        }
    }
}

pub struct HistoryStore {
    fine: HashMap<String, VecDeque<TelemetryPoint>>,
    buckets: HashMap<String, VecDeque<HistoryBucket>>,
    accumulators: HashMap<String, MinuteAccumulator>,
    dirty: bool,
    last_flush: Instant,
}

impl HistoryStore {
    pub fn load() -> Self {
        Self {
            fine: HashMap::new(),
            buckets: storage::load_history_buckets(),
            accumulators: HashMap::new(),
            dirty: false,
            last_flush: Instant::now(),
        }
    }

    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            fine: HashMap::new(),
            buckets: HashMap::new(),
            accumulators: HashMap::new(),
            dirty: false,
            last_flush: Instant::now(),
        }
    }

    pub fn push_fine_only(
        &mut self,
        node_id: &str,
        pt: TelemetryPoint,
        seed_if_empty: Option<VecDeque<TelemetryPoint>>,
    ) -> (Vec<TelemetryPoint>, f64, f64) {
        let ring = self.fine.entry(node_id.to_string()).or_insert_with(|| {
            seed_if_empty.unwrap_or_else(|| VecDeque::with_capacity(MAX_FINE_HISTORY))
        });
        if ring.len() >= MAX_FINE_HISTORY {
            ring.pop_front();
        }
        ring.push_back(pt);
        let (j, loss) = compute_rtt_metrics(ring);
        (ring.iter().cloned().collect(), j, loss)
    }

    /// Records a real node telemetry point into both the 2m fine ring and the 24h 1-minute bucket ring.
    pub fn record_real(&mut self, node_id: &str, pt: TelemetryPoint) -> (Vec<TelemetryPoint>, f64, f64) {
        let minute_ts = (pt.ts / BUCKET_MS) * BUCKET_MS;
        let acc = self
            .accumulators
            .entry(node_id.to_string())
            .or_insert_with(|| MinuteAccumulator::new(minute_ts));

        if acc.minute_ts != minute_ts && acc.samples > 0 {
            let finished = acc.to_bucket();
            let ring = self
                .buckets
                .entry(node_id.to_string())
                .or_insert_with(|| VecDeque::with_capacity(MAX_24H_BUCKETS));
            if ring.len() >= MAX_24H_BUCKETS {
                ring.pop_front();
            }
            ring.push_back(finished);
            self.dirty = true;
            *acc = MinuteAccumulator::new(minute_ts);
        }
        acc.push(&pt);

        self.push_fine_only(node_id, pt, None)
    }

    pub fn get_24h_buckets(&self, node_id: &str) -> Vec<HistoryBucket> {
        let mut out: Vec<HistoryBucket> = self
            .buckets
            .get(node_id)
            .map(|q| q.iter().cloned().collect())
            .unwrap_or_default();
        if let Some(acc) = self.accumulators.get(node_id) {
            if acc.samples > 0 {
                out.push(acc.to_bucket());
            }
        }
        out
    }

    pub fn remove_node(&mut self, node_id: &str) {
        self.fine.remove(node_id);
        self.accumulators.remove(node_id);
        if self.buckets.remove(node_id).is_some() {
            self.dirty = true;
        }
    }

    /// Returns a cloned snapshot of buckets to persist asynchronously if the flush interval has elapsed.
    pub fn take_persist_snapshot(&mut self, force: bool) -> Option<HashMap<String, VecDeque<HistoryBucket>>> {
        if !self.dirty {
            return None;
        }
        if !force && self.last_flush.elapsed() < FLUSH_INTERVAL {
            return None;
        }
        self.dirty = false;
        self.last_flush = Instant::now();
        Some(self.buckets.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_score_penalizes_loss_and_leak() {
        let mut xui = XuiStatus {
            connected: true,
            xray_state: "running".into(),
            mem_used: 400,
            mem_total: 1000,
            ..Default::default()
        };
        let mut shield = LocalShieldStatus {
            mihomo_connected: true,
            quic_blocked: Some(true),
            ipv6_leak: Some(false),
            ..Default::default()
        };
        let (score, status) = compute_health_score(Some(140.0), 0.0, &xui, &shield);
        assert_eq!(score, 100);
        assert_eq!(status, "optimal");

        shield.ipv6_leak = Some(true);
        xui.xray_state = "stopped".into();
        let (degraded_score, degraded_status) = compute_health_score(Some(290.0), 6.0, &xui, &shield);
        assert!(degraded_score < 60);
        assert_eq!(degraded_status, "critical");
    }

    #[test]
    fn health_score_rtt_curve_has_no_abrupt_cliffs() {
        let xui = XuiStatus {
            connected: true,
            xray_state: "running".into(),
            mem_used: 350,
            mem_total: 1000,
            ..Default::default()
        };
        let shield = LocalShieldStatus {
            mihomo_connected: true,
            quic_blocked: Some(true),
            ipv6_leak: Some(false),
            ..Default::default()
        };
        let (s275, _) = compute_health_score(Some(275.0), 0.0, &xui, &shield);
        let (s285, _) = compute_health_score(Some(285.0), 0.0, &xui, &shield);
        assert!(s275 >= s285);
        assert!(
            s275 - s285 <= 2,
            "10ms RTT delta must not cause an abrupt cliff (got {} -> {})",
            s275,
            s285
        );
    }

    #[test]
    fn minute_bucket_aggregates_p95_and_caps_24h() {
        let mut store = HistoryStore::empty();
        // Minute 1: 4 samples (100, 120, 140, None)
        for (i, rtt) in [Some(100.0), Some(120.0), Some(140.0), None].into_iter().enumerate() {
            store.record_real(
                "n1",
                TelemetryPoint {
                    ts: 60_000 + (i as i64) * 2000,
                    rtt_ms: rtt,
                    vps_down_bps: 1000,
                    vps_up_bps: 200,
                    local_down_bps: 900,
                    local_up_bps: 100,
                },
            );
        }
        // Minute 2 triggers rollover
        store.record_real(
            "n1",
            TelemetryPoint {
                ts: 120_000,
                rtt_ms: Some(150.0),
                vps_down_bps: 2000,
                vps_up_bps: 300,
                local_down_bps: 1800,
                local_up_bps: 200,
            },
        );
        let buckets = store.get_24h_buckets("n1");
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].avg_rtt_ms, Some(120.0));
        assert_eq!(buckets[0].p95_rtt_ms, Some(140.0));
        assert_eq!(buckets[0].loss_pct, 25.0);
        assert_eq!(buckets[1].avg_rtt_ms, Some(150.0));
    }
}
