use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    time::Instant,
};

pub(crate) const TERMINATED_LIMIT: usize = 512;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetric {
    pub pid: String,
    pub identity: String,
    pub start_time: u64,
    pub active: bool,
    pub name: String,
    pub memory_bytes: u64,
    /// Legacy field names: process I/O, not physical disk traffic on Windows.
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub session_disk_read: u64,
    pub session_disk_write: u64,
    pub uptime_secs: u64,
    pub network_connections: u32,
    pub network_available: bool,
    pub network_down_bps: Option<u64>,
    pub network_up_bps: Option<u64>,
    pub session_network_down: Option<u64>,
    pub session_network_up: Option<u64>,
}

pub(crate) struct ProcessObservation {
    pub pid: String,
    pub start_time: u64,
    pub creation_token: Option<u64>,
    pub name: String,
    pub memory_bytes: u64,
    pub read: u64,
    pub write: u64,
    pub network_connections: u32,
    pub network: Option<[u64; 2]>,
}

impl ProcessObservation {
    pub(crate) fn identity(&self) -> String {
        format!(
            "{}:{}",
            self.pid,
            self.creation_token.unwrap_or(self.start_time)
        )
    }
}

struct TrackedProcess {
    metric: ProcessMetric,
    previous: Option<(u64, u64)>,
    previous_network: Option<[u64; 2]>,
    observed_at: Instant,
    sampled_at: Instant,
}

#[derive(Default)]
pub(crate) struct ProcessTracker {
    active: HashMap<String, TrackedProcess>,
    terminated: VecDeque<ProcessMetric>,
}

impl ProcessTracker {
    pub(crate) fn identities(&self) -> impl Iterator<Item = &String> {
        self.active.keys()
    }

    #[cfg(test)]
    pub(crate) fn update(
        &mut self,
        observations: impl IntoIterator<Item = ProcessObservation>,
        now: Instant,
    ) -> Vec<ProcessMetric> {
        self.update_with_network(
            observations,
            now,
            &crate::process_network::NetworkSnapshot::default(),
        )
    }

    // One transaction reconciles current and terminated identities with their baselines.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn update_with_network(
        &mut self,
        observations: impl IntoIterator<Item = ProcessObservation>,
        now: Instant,
        network: &crate::process_network::NetworkSnapshot,
    ) -> Vec<ProcessMetric> {
        let mut previous_active = std::mem::take(&mut self.active);
        for observation in observations {
            let identity = observation.identity();
            if self.active.contains_key(&identity) {
                continue;
            }
            let tracked = match previous_active.remove(&identity) {
                Some(mut tracked) => {
                    let elapsed = now
                        .checked_duration_since(tracked.sampled_at)
                        .unwrap_or_default()
                        .as_secs_f64();
                    let (read, write) = tracked.previous.map_or((0, 0), |(read, write)| {
                        (
                            observation.read.saturating_sub(read),
                            observation.write.saturating_sub(write),
                        )
                    });
                    tracked.metric.disk_read_bps = rate(read, elapsed);
                    tracked.metric.disk_write_bps = rate(write, elapsed);
                    tracked.metric.session_disk_read =
                        tracked.metric.session_disk_read.saturating_add(read);
                    tracked.metric.session_disk_write =
                        tracked.metric.session_disk_write.saturating_add(write);
                    tracked.metric.name = observation.name;
                    tracked.metric.memory_bytes = observation.memory_bytes;
                    tracked.metric.network_connections = observation.network_connections;
                    update_network(&mut tracked, observation.network, elapsed);
                    tracked.metric.uptime_secs = now
                        .checked_duration_since(tracked.observed_at)
                        .unwrap_or_default()
                        .as_secs();
                    tracked.previous = Some((observation.read, observation.write));
                    tracked.sampled_at = now;
                    tracked
                }
                None => TrackedProcess {
                    metric: ProcessMetric {
                        pid: observation.pid,
                        identity: identity.clone(),
                        start_time: observation.start_time,
                        active: true,
                        name: observation.name,
                        memory_bytes: observation.memory_bytes,
                        disk_read_bps: 0,
                        disk_write_bps: 0,
                        session_disk_read: 0,
                        session_disk_write: 0,
                        uptime_secs: 0,
                        network_connections: observation.network_connections,
                        network_available: observation.network.is_some(),
                        network_down_bps: observation.network.map(|_| 0),
                        network_up_bps: observation.network.map(|_| 0),
                        session_network_down: observation.network.map(|_| 0),
                        session_network_up: observation.network.map(|_| 0),
                    },
                    previous: Some((observation.read, observation.write)),
                    previous_network: observation.network,
                    observed_at: now,
                    sampled_at: now,
                },
            };
            self.active.insert(identity, tracked);
        }
        self.terminated
            .retain(|metric| !self.active.contains_key(&metric.identity));
        for (identity, mut tracked) in previous_active {
            if network.available
                && tracked.previous_network.is_some()
                && tracked
                    .metric
                    .pid
                    .parse::<u32>()
                    .is_ok_and(|pid| !network.unavailable_pids.contains(&pid))
            {
                if let Some(&counters) = network.counters.get(&identity) {
                    let elapsed = now
                        .checked_duration_since(tracked.sampled_at)
                        .unwrap_or_default()
                        .as_secs_f64();
                    update_network(&mut tracked, Some(counters), elapsed);
                }
            }
            let mut metric = tracked.metric;
            metric.active = false;
            metric.memory_bytes = 0;
            metric.disk_read_bps = 0;
            metric.disk_write_bps = 0;
            metric.network_connections = 0;
            metric.network_down_bps = metric.network_available.then_some(0);
            metric.network_up_bps = metric.network_available.then_some(0);
            self.terminated.push_back(metric);
            if self.terminated.len() > TERMINATED_LIMIT {
                self.terminated.pop_front();
            }
        }
        let mut metrics: Vec<_> = self
            .active
            .values()
            .map(|tracked| tracked.metric.clone())
            .chain(self.terminated.iter().cloned())
            .collect();
        metrics.sort_unstable_by(|left, right| {
            right
                .active
                .cmp(&left.active)
                .then_with(|| {
                    right
                        .disk_read_bps
                        .saturating_add(right.disk_write_bps)
                        .cmp(&left.disk_read_bps.saturating_add(left.disk_write_bps))
                })
                .then_with(|| {
                    right
                        .session_disk_read
                        .saturating_add(right.session_disk_write)
                        .cmp(
                            &left
                                .session_disk_read
                                .saturating_add(left.session_disk_write),
                        )
                })
                .then_with(|| left.identity.cmp(&right.identity))
        });
        metrics
    }

    pub(crate) fn reset_session_totals(&mut self) {
        self.terminated.clear();
        for tracked in self.active.values_mut() {
            tracked.metric.session_disk_read = 0;
            tracked.metric.session_disk_write = 0;
            tracked.metric.disk_read_bps = 0;
            tracked.metric.disk_write_bps = 0;
            tracked.previous = None;
            tracked.previous_network = None;
            tracked.metric.session_network_down = tracked.metric.network_available.then_some(0);
            tracked.metric.session_network_up = tracked.metric.network_available.then_some(0);
        }
    }
}

fn update_network(tracked: &mut TrackedProcess, current: Option<[u64; 2]>, elapsed: f64) {
    tracked.metric.network_available = current.is_some();
    if let Some(current) = current {
        let delta = tracked.previous_network.map_or([0, 0], |previous| {
            [
                current[0].saturating_sub(previous[0]),
                current[1].saturating_sub(previous[1]),
            ]
        });
        tracked.metric.network_down_bps = Some(rate(delta[0], elapsed));
        tracked.metric.network_up_bps = Some(rate(delta[1], elapsed));
        tracked.metric.session_network_down = Some(
            tracked
                .metric
                .session_network_down
                .unwrap_or(0)
                .saturating_add(delta[0]),
        );
        tracked.metric.session_network_up = Some(
            tracked
                .metric
                .session_network_up
                .unwrap_or(0)
                .saturating_add(delta[1]),
        );
    } else {
        tracked.metric.network_down_bps = None;
        tracked.metric.network_up_bps = None;
    }
    // Recovering after unavailable intervals starts a fresh baseline, never guesses bytes.
    tracked.previous_network = current;
}

// Only approximate display rates use floating point; cumulative bytes stay integers.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn rate(bytes: u64, elapsed_seconds: f64) -> u64 {
    if elapsed_seconds > 0.0 && elapsed_seconds.is_finite() {
        (bytes as f64 / elapsed_seconds) as u64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{ProcessObservation, ProcessTracker, TERMINATED_LIMIT};
    use std::time::{Duration, Instant};

    fn observation(pid: u64, start_time: u64, read: u64, write: u64) -> ProcessObservation {
        ProcessObservation {
            pid: pid.to_string(),
            start_time,
            creation_token: None,
            name: "worker".into(),
            memory_bytes: 128,
            read,
            write,
            network_connections: 3,
            network: None,
        }
    }

    #[test]
    fn terminated_process_keeps_the_last_captured_network_bytes() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        let mut first = observation(42, 1, 0, 0);
        first.network = Some([100, 100]);
        tracker.update([first], now);
        let snapshot = crate::process_network::NetworkSnapshot {
            available: true,
            counters: std::collections::HashMap::from([("42:1".into(), [900, 500])]),
            ..crate::process_network::NetworkSnapshot::default()
        };
        let ended = tracker.update_with_network([], now + Duration::from_secs(5), &snapshot);
        assert_eq!(ended[0].session_network_down, Some(800));
        assert_eq!(ended[0].session_network_up, Some(400));
        assert_eq!(ended[0].network_down_bps, Some(0));
        assert!(!ended[0].active);
    }

    #[test]
    fn a_terminated_unattributable_pid_does_not_claim_partial_tail_bytes() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        let mut first = observation(42, 1, 0, 0);
        first.network = Some([100, 100]);
        tracker.update([first], now);
        let snapshot = crate::process_network::NetworkSnapshot {
            available: true,
            unavailable_pids: std::collections::HashSet::from([42]),
            counters: std::collections::HashMap::from([("42:1".into(), [900, 500])]),
            ..crate::process_network::NetworkSnapshot::default()
        };
        let ended = tracker.update_with_network([], now + Duration::from_secs(5), &snapshot);
        assert_eq!(ended[0].session_network_down, Some(0));
    }

    #[test]
    fn network_uses_native_identity_observed_deltas_and_actual_elapsed() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        let mut first = observation(42, 1, 100, 100);
        first.creation_token = Some(123_456);
        first.network = Some([9000, 8000]);
        let baseline = tracker.update([first], now);
        assert_eq!(baseline[0].identity, "42:123456");
        assert_eq!(baseline[0].session_network_down, Some(0));
        let mut second = observation(42, 1, 100, 100);
        second.creation_token = Some(123_456);
        second.network = Some([10_000, 8500]);
        let metrics = tracker.update([second], now + Duration::from_secs(5));
        assert_eq!(
            (metrics[0].network_down_bps, metrics[0].network_up_bps),
            (Some(200), Some(100))
        );
        assert_eq!(
            (
                metrics[0].session_network_down,
                metrics[0].session_network_up
            ),
            (Some(1000), Some(500))
        );
        let ended = tracker.update([], now + Duration::from_secs(10));
        assert_eq!(ended[0].network_down_bps, Some(0));
        assert_eq!(ended[0].session_network_down, Some(1000));
    }

    #[test]
    fn network_recovers_with_a_baseline_not_inferred_missing_bytes() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        let mut first = observation(42, 1, 0, 0);
        first.network = Some([100, 100]);
        tracker.update([first], now);
        let absent = tracker.update([observation(42, 1, 0, 0)], now + Duration::from_secs(5));
        assert!(!absent[0].network_available);
        assert_eq!(absent[0].network_down_bps, None);
        let mut recovery = observation(42, 1, 0, 0);
        recovery.network = Some([9999, 9999]);
        let metrics = tracker.update([recovery], now + Duration::from_secs(10));
        assert_eq!(metrics[0].network_down_bps, Some(0));
        assert_eq!(metrics[0].session_network_down, Some(0));
    }

    #[test]
    fn first_observation_has_zero_rates_and_session_bytes() {
        let metrics =
            ProcessTracker::default().update([observation(42, 1, 9_000, 8_000)], Instant::now());
        assert_eq!(metrics[0].identity, "42:1");
        assert_eq!(metrics[0].start_time, 1);
        assert!(metrics[0].active);
        assert_eq!(
            (metrics[0].disk_read_bps, metrics[0].disk_write_bps),
            (0, 0)
        );
        assert_eq!(
            (metrics[0].session_disk_read, metrics[0].session_disk_write),
            (0, 0)
        );
        assert!(!metrics[0].network_available);
        assert_eq!(
            (metrics[0].network_down_bps, metrics[0].network_up_bps),
            (None, None)
        );
    }

    #[test]
    fn rates_use_actual_time_between_process_observations() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        tracker.update([observation(42, 1, 1_000, 2_000)], now);
        let metrics = tracker.update(
            [observation(42, 1, 5_000, 3_000)],
            now + Duration::from_secs(5),
        );
        assert_eq!(
            (metrics[0].disk_read_bps, metrics[0].disk_write_bps),
            (800, 200)
        );
        assert_eq!(
            (metrics[0].session_disk_read, metrics[0].session_disk_write),
            (4_000, 1_000)
        );
        assert_eq!(metrics[0].uptime_secs, 5);
    }

    #[test]
    fn pid_reuse_without_an_empty_scan_starts_a_new_identity() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        tracker.update([observation(42, 1, 1_000, 2_000)], now);
        tracker.update(
            [observation(42, 1, 1_500, 2_100)],
            now + Duration::from_secs(5),
        );
        let metrics = tracker.update(
            [observation(42, 2, 9_000, 9_000)],
            now + Duration::from_secs(10),
        );
        let active = metrics.iter().find(|metric| metric.active).unwrap();
        assert_eq!(active.identity, "42:2");
        assert_eq!(
            (
                active.session_disk_read,
                active.disk_read_bps,
                active.uptime_secs
            ),
            (0, 0, 0)
        );
        let exited = metrics.iter().find(|metric| !metric.active).unwrap();
        assert_eq!(exited.identity, "42:1");
        assert_eq!(exited.session_disk_read, 500);
        assert_eq!(
            (
                exited.memory_bytes,
                exited.disk_read_bps,
                exited.network_connections
            ),
            (0, 0, 0)
        );
        assert_eq!(tracker.active.len(), 1);
    }

    #[test]
    fn counter_reset_does_not_erase_already_observed_session_bytes() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        tracker.update([observation(42, 1, 1_000, 1_000)], now);
        tracker.update(
            [observation(42, 1, 1_200, 1_100)],
            now + Duration::from_secs(5),
        );
        let reset = tracker.update(
            [observation(42, 1, 10, 1_200)],
            now + Duration::from_secs(10),
        );
        assert_eq!(
            (
                reset[0].disk_read_bps,
                reset[0].session_disk_read,
                reset[0].session_disk_write
            ),
            (0, 200, 200)
        );
        let next = tracker.update(
            [observation(42, 1, 30, 1_300)],
            now + Duration::from_secs(15),
        );
        assert_eq!((next[0].disk_read_bps, next[0].session_disk_read), (4, 220));
    }

    #[test]
    fn process_churn_keeps_only_current_baselines_and_bounded_terminated_records() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        for pid in 0..2_000 {
            tracker.update(
                [observation(pid, pid, 1_000, 1_000)],
                now + Duration::from_secs(pid),
            );
            assert_eq!(tracker.active.len(), 1);
            assert!(tracker.terminated.len() <= TERMINATED_LIMIT);
        }
        let metrics = tracker.update([], now + Duration::from_secs(2_000));
        assert!(tracker.active.is_empty());
        assert_eq!(tracker.terminated.len(), TERMINATED_LIMIT);
        assert_eq!(metrics.len(), TERMINATED_LIMIT);
        assert!(metrics.iter().all(|metric| !metric.active));
        assert!(metrics
            .iter()
            .all(|metric| metric.start_time >= 2_000 - TERMINATED_LIMIT as u64));
    }

    #[test]
    fn reset_clears_terminated_history_and_rebaselines_active_processes() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        tracker.update(
            [
                observation(42, 1, 1_000, 1_000),
                observation(43, 1, 100, 100),
            ],
            now,
        );
        tracker.update(
            [observation(42, 1, 2_000, 2_000)],
            now + Duration::from_secs(5),
        );
        tracker.reset_session_totals();
        assert!(tracker.terminated.is_empty());
        let metrics = tracker.update(
            [observation(42, 1, 3_000, 3_000)],
            now + Duration::from_secs(10),
        );
        assert_eq!(
            (metrics[0].session_disk_read, metrics[0].disk_read_bps),
            (0, 0)
        );
        assert_eq!(metrics[0].uptime_secs, 10);
    }

    #[test]
    fn returning_identity_is_not_duplicated_in_terminated_cache() {
        let mut tracker = ProcessTracker::default();
        let now = Instant::now();
        tracker.update([observation(42, 1, 1_000, 1_000)], now);
        tracker.update([], now + Duration::from_secs(5));
        let metrics = tracker.update(
            [observation(42, 1, 2_000, 2_000)],
            now + Duration::from_secs(10),
        );
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].session_disk_read, 0);
        assert!(tracker.terminated.is_empty());
    }
}
