use serde::Serialize;
use std::{collections::HashMap, hash::Hash};

pub(crate) struct CounterAccumulator<K> {
    previous: HashMap<K, (u64, u64)>,
    total: (u64, u64),
}

impl<K> Default for CounterAccumulator<K> {
    fn default() -> Self {
        Self {
            previous: HashMap::new(),
            total: (0, 0),
        }
    }
}

impl<K: Eq + Hash> CounterAccumulator<K> {
    pub(crate) fn update(
        &mut self,
        sources: impl IntoIterator<Item = (K, u64, u64)>,
    ) -> (u64, u64) {
        let mut current = HashMap::new();
        for (key, first, second) in sources {
            let counters = current.entry(key).or_insert((first, second));
            counters.0 = counters.0.max(first);
            counters.1 = counters.1.max(second);
        }
        for (key, &(first, second)) in &current {
            if let Some(&(previous_first, previous_second)) = self.previous.get(key) {
                self.total.0 = self
                    .total
                    .0
                    .saturating_add(first.saturating_sub(previous_first));
                self.total.1 = self
                    .total
                    .1
                    .saturating_add(second.saturating_sub(previous_second));
            }
        }
        // Removed sources lose their baseline; reappearance is a fresh observation.
        self.previous = current;
        self.total
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CounterSample {
    pub network_down: u64,
    pub network_up: u64,
    pub disk_read: u64,
    pub disk_write: u64,
}
impl CounterSample {
    #[must_use]
    pub const fn new(network_down: u64, network_up: u64, disk_read: u64, disk_write: u64) -> Self {
        Self {
            network_down,
            network_up,
            disk_read,
            disk_write,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSnapshot {
    pub network_down_bps: u64,
    pub network_up_bps: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub session_network_down: u64,
    pub session_network_up: u64,
    pub session_disk_read: u64,
    pub session_disk_write: u64,
    pub session_duration_secs: u64,
}

#[derive(Default)]
pub struct MetricsEngine {
    previous: Option<CounterSample>,
    snapshot: MetricsSnapshot,
    session_elapsed_millis: u64,
}
impl MetricsEngine {
    pub fn update(&mut self, current: CounterSample, elapsed_seconds: f64) -> MetricsSnapshot {
        let elapsed =
            (elapsed_seconds.is_finite() && elapsed_seconds > 0.0).then_some(elapsed_seconds);
        if let Some(previous) = self.previous {
            let down = current.network_down.saturating_sub(previous.network_down);
            let up = current.network_up.saturating_sub(previous.network_up);
            let read = current.disk_read.saturating_sub(previous.disk_read);
            let write = current.disk_write.saturating_sub(previous.disk_write);
            self.snapshot.network_down_bps =
                elapsed.map_or(0, |seconds| bytes_per_second(down, seconds));
            self.snapshot.network_up_bps =
                elapsed.map_or(0, |seconds| bytes_per_second(up, seconds));
            self.snapshot.disk_read_bps =
                elapsed.map_or(0, |seconds| bytes_per_second(read, seconds));
            self.snapshot.disk_write_bps =
                elapsed.map_or(0, |seconds| bytes_per_second(write, seconds));
            self.snapshot.session_network_down =
                self.snapshot.session_network_down.saturating_add(down);
            self.snapshot.session_network_up = self.snapshot.session_network_up.saturating_add(up);
            self.snapshot.session_disk_read = self.snapshot.session_disk_read.saturating_add(read);
            self.snapshot.session_disk_write =
                self.snapshot.session_disk_write.saturating_add(write);
            if let Some(elapsed) = elapsed {
                self.session_elapsed_millis = self
                    .session_elapsed_millis
                    .saturating_add(elapsed_millis(elapsed));
            }
        }
        self.snapshot.session_duration_secs = self.session_elapsed_millis / 1_000;
        self.previous = Some(current);
        self.snapshot.clone()
    }
    /// The new epoch begins at the next observation (nominally within one system tick).
    /// The interval spanning the reset is not attributed to either epoch.
    pub fn reset_session_totals(&mut self) {
        self.snapshot = MetricsSnapshot::default();
        self.previous = None;
        self.session_elapsed_millis = 0;
    }
}

// Display rates are approximate; Rust's float cast truncates and saturates at u64::MAX.
// The caller supplies only finite, positive seconds. Byte totals remain exact integers.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn bytes_per_second(bytes: u64, seconds: f64) -> u64 {
    (bytes as f64 / seconds) as u64
}

// Millisecond rounding is intentional; huge positive intervals saturate rather than wrap.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn elapsed_millis(seconds: f64) -> u64 {
    (seconds * 1_000.0).round() as u64
}

#[cfg(test)]
mod tests {
    use super::{CounterAccumulator, CounterSample, MetricsEngine};

    #[test]
    fn source_arrival_and_removal_do_not_change_other_source_deltas() {
        let mut counters = CounterAccumulator::default();
        assert_eq!(
            counters.update([("a", 1_000, 200), ("b", 1_000, 400)]),
            (0, 0)
        );
        assert_eq!(counters.update([("b", 1_100, 450)]), (100, 50));
        assert_eq!(
            counters.update([("b", 1_200, 500), ("c", 9_000, 9_000)]),
            (200, 100)
        );
        assert_eq!(counters.previous.len(), 2);
    }

    #[test]
    fn source_counter_reset_rebaselines_only_that_counter() {
        let mut counters = CounterAccumulator::default();
        counters.update([("a", 1_000, 1_000)]);
        assert_eq!(counters.update([("a", 10, 1_200)]), (0, 200));
        assert_eq!(counters.update([("a", 60, 1_300)]), (50, 300));
    }

    #[test]
    fn duplicate_device_aliases_are_counted_once() {
        let mut counters = CounterAccumulator::default();
        counters.update([("volume", 1_000, 1_000), ("volume", 1_010, 1_020)]);
        assert_eq!(
            counters.update([("volume", 1_100, 1_100), ("volume", 1_110, 1_120)]),
            (100, 100)
        );
    }

    #[test]
    fn removed_source_reappearance_starts_a_new_baseline() {
        let mut counters = CounterAccumulator::default();
        counters.update([("a", 1_000, 1_000)]);
        counters.update([]);
        assert_eq!(counters.update([("a", 9_000, 9_000)]), (0, 0));
        assert_eq!(counters.update([("a", 9_010, 9_020)]), (10, 20));
    }

    #[test]
    fn first_sample_is_a_baseline_not_an_observed_interval() {
        let mut engine = MetricsEngine::default();
        let first = engine.update(CounterSample::new(1_000, 1_000, 1_000, 1_000), 5.0);
        assert_eq!(first.session_duration_secs, 0);
        let second = engine.update(CounterSample::new(2_000, 2_000, 2_000, 2_000), 5.0);
        assert_eq!(second.session_duration_secs, 5);
        assert_eq!(second.network_down_bps, 200);
    }

    #[test]
    fn session_reset_rebaselines_without_carrying_pre_reset_bytes() {
        let mut engine = MetricsEngine::default();
        engine.update(CounterSample::new(1_000, 1_000, 1_000, 1_000), 1.0);
        engine.update(CounterSample::new(2_000, 2_000, 2_000, 2_000), 1.0);
        engine.reset_session_totals();
        let reset = engine.update(CounterSample::new(9_000, 9_000, 9_000, 9_000), 5.0);
        assert_eq!(
            (
                reset.session_network_down,
                reset.session_network_up,
                reset.session_disk_read,
                reset.session_disk_write
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(
            (
                reset.network_down_bps,
                reset.disk_read_bps,
                reset.session_duration_secs
            ),
            (0, 0, 0)
        );
        let next = engine.update(CounterSample::new(10_000, 10_000, 10_000, 10_000), 5.0);
        assert_eq!(
            (
                next.session_network_down,
                next.network_down_bps,
                next.session_duration_secs
            ),
            (1_000, 200, 5)
        );
    }

    #[test]
    fn invalid_elapsed_preserves_bytes_without_rates_or_duration() {
        for elapsed in [f64::NAN, 0.0, -1.0, f64::INFINITY] {
            let mut engine = MetricsEngine::default();
            engine.update(CounterSample::new(1_000, 1_000, 1_000, 1_000), 1.0);
            let invalid = engine.update(CounterSample::new(1_100, 1_100, 1_100, 1_100), elapsed);
            assert_eq!(
                (
                    invalid.network_down_bps,
                    invalid.network_up_bps,
                    invalid.disk_read_bps,
                    invalid.disk_write_bps
                ),
                (0, 0, 0, 0)
            );
            assert_eq!(invalid.session_network_down, 100);
            assert_eq!(invalid.session_duration_secs, 0);
            let next = engine.update(CounterSample::new(1_200, 1_200, 1_200, 1_200), 5.0);
            assert_eq!(
                (
                    next.session_network_down,
                    next.network_down_bps,
                    next.session_duration_secs
                ),
                (200, 20, 5)
            );
        }
    }

    #[test]
    fn session_bytes_and_duration_saturate_instead_of_wrapping() {
        let mut engine = MetricsEngine::default();
        engine.update(CounterSample::default(), 1.0);
        engine.update(
            CounterSample::new(u64::MAX, u64::MAX, u64::MAX, u64::MAX),
            1.0,
        );
        engine.update(CounterSample::default(), 1.0);
        engine.session_elapsed_millis = u64::MAX - 1;
        let snapshot = engine.update(CounterSample::new(1, 1, 1, 1), 1.0);
        assert_eq!(
            (
                snapshot.session_network_down,
                snapshot.session_network_up,
                snapshot.session_disk_read,
                snapshot.session_disk_write
            ),
            (u64::MAX, u64::MAX, u64::MAX, u64::MAX)
        );
        assert_eq!(snapshot.session_duration_secs, u64::MAX / 1_000);
    }

    #[test]
    fn synthetic_totals_saturate_across_sources() {
        let mut counters = CounterAccumulator::default();
        counters.update([("a", 0, 0), ("b", 0, 0)]);
        assert_eq!(
            counters.update([("a", u64::MAX, u64::MAX), ("b", u64::MAX, u64::MAX)]),
            (u64::MAX, u64::MAX)
        );
    }

    #[test]
    fn counter_reset_does_not_create_negative_rate() {
        let mut engine = MetricsEngine::default();
        engine.update(CounterSample::new(1_000, 2_000, 500, 800), 1.0);

        let snapshot = engine.update(CounterSample::new(100, 100, 10, 10), 1.0);

        assert_eq!(snapshot.network_down_bps, 0);
    }

    #[test]
    fn reset_clears_session_totals() {
        let mut engine = MetricsEngine::default();
        engine.update(CounterSample::new(1_000, 2_000, 500, 800), 1.0);
        engine.update(CounterSample::new(1_500, 2_400, 800, 1_000), 1.0);

        engine.reset_session_totals();

        assert_eq!(engine.snapshot.session_network_down, 0);
    }
}
