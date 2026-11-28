use crate::models::{HistoryBucket, LocalShieldStatus, TelemetryPoint, XuiStatus};
use crate::storage;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

pub const MAX_FINE_HISTORY: usize = 60;
pub const MAX_24H_BUCKETS: usize = 1440;
const BUCKET_MS: i64 = 60_000;
const WINDOW_24H_MS: i64 = 24 * 60 * 60 * 1000;
/// 24h chart resolution sent over IPC: 5-minute groups (288 points instead of 1440).
const CHART_GROUP_MS: i64 = 5 * BUCKET_MS;
const FLUSH_INTERVAL: Duration = Duration::from_secs(60);

/// Merges minute buckets into `group_ms`-wide buckets (time-aligned). RTT averages are weighted by
/// successful samples, loss by total samples; P95 keeps the group's worst minute (conservative peak).
pub fn downsample_buckets(buckets: &[HistoryBucket], group_ms: i64) -> Vec<HistoryBucket> {
    #[derive(Default)]
    struct Group {
        ts: i64,
        rtt_weighted_sum: f64,
        rtt_weight: f64,
        missing: f64,
        worst_p95: Option<f64>,
        local_down_sum: u128,
        vps_down_sum: u128,
        samples: u32,
    }
    impl Group {
        fn finish(self) -> HistoryBucket {
            let n = self.samples.max(1);
            HistoryBucket {
                ts: self.ts,
                avg_rtt_ms: (self.rtt_weight > 0.0)
                    .then(|| ((self.rtt_weighted_sum / self.rtt_weight) * 10.0).round() / 10.0),
                p95_rtt_ms: self.worst_p95,
                loss_pct: ((self.missing / n as f64) * 100.0 * 10.0).round() / 10.0,
                avg_local_down_bps: (self.local_down_sum / n as u128) as u64,
                avg_vps_down_bps: (self.vps_down_sum / n as u128) as u64,
                samples: self.samples,
            }
        }
    }

    let mut out = Vec::new();
    let mut current: Option<Group> = None;
    for b in buckets {
        let group_ts = b.ts.div_euclid(group_ms) * group_ms;
        if current.as_ref().is_some_and(|g| g.ts != group_ts) {
            out.extend(current.take().map(Group::finish));
        }
        let g = current.get_or_insert_with(|| Group {
            ts: group_ts,
            ..Default::default()
        });
        let samples = b.samples.max(1);
        let missing = (b.loss_pct / 100.0 * samples as f64).round();
        if let Some(avg) = b.avg_rtt_ms {
            let ok = (samples as f64 - missing).max(1.0);
            g.rtt_weighted_sum += avg * ok;
            g.rtt_weight += ok;
        }
        g.missing += missing;
        g.worst_p95 = match (g.worst_p95, b.p95_rtt_ms) {
            (Some(x), Some(y)) => Some(x.max(y)),
            (x, y) => x.or(y),
        };
        g.local_down_sum += b.avg_local_down_bps as u128 * samples as u128;
        g.vps_down_sum += b.avg_vps_down_bps as u128 * samples as u128;
        g.samples += samples;
    }
    out.extend(current.map(Group::finish));
    out
}

/// Returns a smoothed RTT (median of the latest up to 7 valid samples, ~14s in active mode)
/// so isolated spikes never cause erratic health score jumps. Reports `None` immediately when
/// two consecutive probes time out so real disconnects are never masked.
pub fn compute_smoothed_rtt(history: &[TelemetryPoint], latest: Option<f64>) -> Option<f64> {
    let mut recent: Vec<f64> = history
        .iter()
        .rev()
        .take(7)
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

pub fn health_status_for_score(score: u32) -> String {
    if score >= 85 {
        "optimal".to_string()
    } else if score >= 60 {
        "degraded".to_string()
    } else {
        "critical".to_string()
    }
}

#[cfg(test)]
pub fn compute_health_score(
    rtt_ms: Option<f64>,
    loss_pct: f64,
    xui: &XuiStatus,
    shield: &LocalShieldStatus,
) -> (u32, String) {
    let fail_streak = if xui.connected { 0 } else { 2 };
    compute_health_score_with_streak(rtt_ms, loss_pct, xui, shield, fail_streak)
}

pub fn compute_health_score_with_streak(
    rtt_ms: Option<f64>,
    loss_pct: f64,
    xui: &XuiStatus,
    shield: &LocalShieldStatus,
    xui_fail_streak: u32,
) -> (u32, String) {
    let mut score: i32 = 100;

    // Smooth piecewise RTT penalty calibrated for cross-Pacific Native ISP routes
    // (<=200ms ideal, 200-320ms gentle -0..-8, 320-500ms elevated -8..-22, >500ms capped at -30)
    if rtt_ms.is_none() {
        score -= 40;
    } else if let Some(r) = rtt_ms {
        if r > 500.0 {
            let extra = (((r - 500.0) / 150.0) * 8.0).min(8.0).round() as i32;
            score -= 22 + extra;
        } else if r > 320.0 {
            let extra = (((r - 320.0) / 180.0) * 14.0).round() as i32;
            score -= 8 + extra;
        } else if r > 200.0 {
            let penalty = (((r - 200.0) / 120.0) * 8.0).round() as i32;
            score -= penalty;
        }
    }

    // Smooth linear packet-loss penalty (eliminates abrupt -10 -> -20 cliff at 5%)
    if loss_pct > 0.0 {
        score -= (loss_pct * 2.0).round().clamp(1.0, 25.0) as i32;
    }

    if !xui.connected {
        // Single transient panel hiccup deducts -4; confirmed 2+ consecutive failures deduct -18
        score -= if xui_fail_streak >= 2 { 18 } else { 4 };
    } else {
        if xui.xray_state != "running" {
            score -= 25;
        }
        // Smooth CPU ramp (75%..95% -> 0..-10) instead of a 10-point cliff at 85%
        if xui.cpu_percent > 75.0 {
            let cpu_pen = (((xui.cpu_percent - 75.0) / 20.0) * 10.0)
                .clamp(0.0, 10.0)
                .round() as i32;
            score -= cpu_pen;
        }
        // Smooth RAM ramp (82%..96% -> 0..-10) instead of a 10-point cliff at 90%
        if xui.mem_total > 0 {
            let mem_ratio = xui.mem_used as f64 / xui.mem_total as f64;
            if mem_ratio > 0.82 {
                let mem_pen = (((mem_ratio - 0.82) / 0.14) * 10.0)
                    .clamp(0.0, 10.0)
                    .round() as i32;
                score -= mem_pen;
            }
        }
    }

    if shield.ipv6_leak == Some(true) {
        score -= 15;
    }
    if shield.mihomo_connected && shield.quic_blocked == Some(false) {
        score -= 5;
    }

    let clamped = score.clamp(0, 100) as u32;
    (clamped, health_status_for_score(clamped))
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
    /// Per-node `(ewma_score, displayed_score)` for jitter-free health gauge display.
    score_state: HashMap<String, (f64, u32)>,
    /// Per-node consecutive 3x-ui poll failures so 1-tick panel hiccups don't plunge health.
    xui_fail_streak: HashMap<String, u32>,
    dirty: bool,
    last_flush: Instant,
}

impl HistoryStore {
    pub fn load() -> Self {
        Self {
            fine: HashMap::new(),
            buckets: storage::load_history_buckets(),
            accumulators: HashMap::new(),
            score_state: HashMap::new(),
            xui_fail_streak: HashMap::new(),
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
            score_state: HashMap::new(),
            xui_fail_streak: HashMap::new(),
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
            // Bound by both count and age so app downtime never stretches the "24h" window.
            while ring.len() >= MAX_24H_BUCKETS
                || ring.front().is_some_and(|b| b.ts < pt.ts - WINDOW_24H_MS)
            {
                ring.pop_front();
            }
            ring.push_back(finished);
            self.dirty = true;
            *acc = MinuteAccumulator::new(minute_ts);
        }
        acc.push(&pt);

        self.push_fine_only(node_id, pt, None)
    }

    /// Minute buckets inside `[now - 24h, now]` (incl. the live partial minute), downsampled to
    /// 5-minute chart points.
    pub fn get_24h_buckets(&self, node_id: &str, now_ms: i64) -> Vec<HistoryBucket> {
        let cutoff = now_ms - WINDOW_24H_MS;
        let mut minutes: Vec<HistoryBucket> = self
            .buckets
            .get(node_id)
            .map(|q| q.iter().filter(|b| b.ts >= cutoff).cloned().collect())
            .unwrap_or_default();
        if let Some(acc) = self.accumulators.get(node_id) {
            if acc.samples > 0 {
                minutes.push(acc.to_bucket());
            }
        }
        downsample_buckets(&minutes, CHART_GROUP_MS)
    }

    /// Closes every partial minute into the persisted ring (called once on app exit).
    pub fn flush_partial_minutes(&mut self) {
        for (node_id, acc) in self.accumulators.drain() {
            if acc.samples == 0 {
                continue;
            }
            let ring = self.buckets.entry(node_id).or_default();
            if ring.len() >= MAX_24H_BUCKETS {
                ring.pop_front();
            }
            ring.push_back(acc.to_bucket());
            self.dirty = true;
        }
    }

    pub fn record_xui_connectivity(&mut self, node_id: &str, connected: bool) -> u32 {
        if connected {
            self.xui_fail_streak.insert(node_id.to_string(), 0);
            0
        } else {
            let entry = self
                .xui_fail_streak
                .entry(node_id.to_string())
                .or_insert(0);
            *entry = entry.saturating_add(1);
            *entry
        }
    }

    /// Applies EWMA + hysteresis deadband to raw health scores so healthy links never bounce
    /// between 80s and 90s on normal cross-Pacific variance, while critical outages react fast.
    pub fn smooth_health_score(
        &mut self,
        node_id: &str,
        raw_score: u32,
        fast_react: bool,
    ) -> (u32, String) {
        let clamped_raw = raw_score.min(100);
        let displayed = match self.score_state.get_mut(node_id) {
            None => {
                self.score_state
                    .insert(node_id.to_string(), (clamped_raw as f64, clamped_raw));
                clamped_raw
            }
            Some((ema, shown)) => {
                let alpha = if fast_react { 0.65 } else { 0.25 };
                *ema += alpha * (clamped_raw as f64 - *ema);
                if fast_react || (*ema - *shown as f64).abs() >= 0.75 {
                    *shown = ema.round().clamp(0.0, 100.0) as u32;
                }
                *shown
            }
        };
        (displayed, health_status_for_score(displayed))
    }

    pub fn remove_node(&mut self, node_id: &str) {
        self.fine.remove(node_id);
        self.accumulators.remove(node_id);
        self.score_state.remove(node_id);
        self.xui_fail_streak.remove(node_id);
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
        let minutes = store.buckets.get("n1").expect("minute ring");
        assert_eq!(minutes.len(), 1);
        assert_eq!(minutes[0].avg_rtt_ms, Some(120.0));
        assert_eq!(minutes[0].p95_rtt_ms, Some(140.0));
        assert_eq!(minutes[0].loss_pct, 25.0);

        // Chart view: both minutes fall in the same 5-minute group (finished + live partial minute)
        let chart = store.get_24h_buckets("n1", 125_000);
        assert_eq!(chart.len(), 1);
        assert_eq!(chart[0].samples, 5);
        assert_eq!(chart[0].loss_pct, 20.0);
        assert_eq!(chart[0].p95_rtt_ms, Some(150.0));
        // (100+120+140)*3/3 weighted with the 150ms partial minute → (360 + 150) / 4
        assert_eq!(chart[0].avg_rtt_ms, Some(127.5));
    }

    #[test]
    fn chart_window_excludes_stale_buckets_and_downsamples_to_5min() {
        let mut store = HistoryStore::empty();
        let day = WINDOW_24H_MS;
        let base = 10 * day;
        // One stale sample ~25h ago, then 10 consecutive live minutes
        let stale_ts = base - day - 3_600_000;
        let mut push = |ts: i64, rtt: Option<f64>| {
            store.record_real(
                "n1",
                TelemetryPoint {
                    ts,
                    rtt_ms: rtt,
                    vps_down_bps: 1000,
                    vps_up_bps: 100,
                    local_down_bps: 1000,
                    local_up_bps: 100,
                },
            );
        };
        push(stale_ts, Some(500.0));
        for minute in 0..10 {
            push(base + minute * BUCKET_MS, Some(200.0));
        }
        push(base + 10 * BUCKET_MS, Some(200.0));

        let chart = store.get_24h_buckets("n1", base + 10 * BUCKET_MS);
        assert!(chart.iter().all(|b| b.ts >= base - day), "stale bucket leaked: {chart:?}");
        assert!(chart.iter().all(|b| b.avg_rtt_ms == Some(200.0)));
        assert!(chart.len() <= 3, "10 minutes should collapse into <=3 five-minute groups");
        assert!(chart.windows(2).all(|w| w[1].ts - w[0].ts == CHART_GROUP_MS));
    }

    #[test]
    fn health_score_stays_steady_under_cross_pacific_jitter_and_1tick_xui_hiccup() {
        let mut store = HistoryStore::empty();
        let healthy_xui = XuiStatus {
            connected: true,
            xray_state: "running".into(),
            cpu_percent: 12.0,
            mem_used: 350,
            mem_total: 1000,
            ..Default::default()
        };
        let offline_xui = XuiStatus {
            connected: false,
            xray_state: "unknown".into(),
            ..Default::default()
        };
        let shield = LocalShieldStatus {
            mihomo_connected: true,
            quic_blocked: Some(true),
            ipv6_leak: Some(false),
            ..Default::default()
        };

        // Realistic US residential ISP RTT series with periodic spikes (213ms baseline, 338ms spike)
        let samples = [
            213.0, 224.0, 236.0, 306.0, 232.0, 218.0, 338.0, 226.0, 215.0, 295.0, 220.0,
        ];
        let mut scores = Vec::new();
        for (idx, rtt) in samples.into_iter().enumerate() {
            let (hist, _j, loss) = store.record_real(
                "n1",
                TelemetryPoint {
                    ts: 1_000_000 + (idx as i64) * 2000,
                    rtt_ms: Some(rtt),
                    vps_down_bps: 100_000,
                    vps_up_bps: 20_000,
                    local_down_bps: 90_000,
                    local_up_bps: 15_000,
                },
            );
            // Inject a 1-tick transient 3x-ui hiccup on tick 5
            let xui = if idx == 5 { &offline_xui } else { &healthy_xui };
            let streak = store.record_xui_connectivity("n1", xui.connected);
            let smoothed_rtt = compute_smoothed_rtt(&hist, Some(rtt));
            let (raw, _) = compute_health_score_with_streak(smoothed_rtt, loss, xui, &shield, streak);
            let (score, _) = store.smooth_health_score("n1", raw, false);
            scores.push(score);
        }

        let min_s = *scores.iter().min().unwrap();
        let max_s = *scores.iter().max().unwrap();
        assert!(
            min_s >= 94,
            "healthy cross-Pacific node should stay in mid/high 90s, got {:?}",
            scores
        );
        assert!(
            max_s - min_s <= 4,
            "score spread should stay tight (<=4 pts), got {:?}",
            scores
        );
    }
}
