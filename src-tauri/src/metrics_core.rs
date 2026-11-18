//! Deterministic system accounting. The host supplies observations and both clocks.
use crate::{
    metrics::{CounterSample, MetricsEngine, MetricsSnapshot},
    runtime::ObservedSample,
};
use std::time::Instant;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeUsage {
    pub mount_point: String,
    pub name: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub is_removable: bool,
}

#[derive(Default)]
pub struct SystemObservation {
    pub counters: CounterSample,
    pub interfaces: Vec<String>,
    pub total_memory_bytes: u64,
    pub used_memory_bytes: u64,
    pub disk_available: bool,
    pub cpu_usage_percent: f32,
    pub volumes: Vec<VolumeUsage>,
}

#[derive(Clone, Copy)]
pub struct StorageSample {
    counters: CounterSample,
    timestamp: u64,
    elapsed_millis: u64,
}

impl StorageSample {
    #[must_use]
    pub fn observed(self) -> ObservedSample {
        ObservedSample {
            timestamp: self.timestamp,
            elapsed_millis: self.elapsed_millis,
            bytes: [
                self.counters.network_down,
                self.counters.network_up,
                self.counters.disk_read,
                self.counters.disk_write,
            ],
        }
    }
}

pub struct SystemFrame {
    pub metrics: MetricsSnapshot,
    pub interfaces: Vec<String>,
    pub total_memory_bytes: u64,
    pub used_memory_bytes: u64,
    pub disk_available: bool,
    pub cpu_usage_percent: f32,
    pub volumes: Vec<VolumeUsage>,
    pub sequence: u64,
    pub storage: StorageSample,
}

pub struct SystemReducer {
    engine: MetricsEngine,
    origin: Instant,
    previous: Instant,
    previous_wall_secs: Option<u64>,
    sequence: u64,
    generation: u64,
}

impl SystemReducer {
    #[must_use]
    pub fn new(origin: Instant) -> Self {
        Self {
            engine: MetricsEngine::default(),
            origin,
            previous: origin,
            previous_wall_secs: None,
            sequence: 0,
            generation: 0,
        }
    }

    #[must_use]
    pub fn observe(
        &mut self,
        observation: SystemObservation,
        now: Instant,
        timestamp: u64,
        generation: u64,
    ) -> SystemFrame {
        if generation != self.generation {
            self.engine.reset_session_totals();
            self.generation = generation;
        }
        let now = now.max(self.previous);
        let mono_duration = now.duration_since(self.previous);
        let mono_secs = mono_duration.as_secs_f64();
        let wall_delta = self
            .previous_wall_secs
            .map_or(0, |prev| timestamp.saturating_sub(prev));
        // Suppress hibernation/sleep bursts when either monotonic or wall clocks jump.
        let suspend_gap = self.previous_wall_secs.is_some()
            && (mono_secs >= 30.0
                || (wall_delta >= 30 && wall_delta.saturating_sub(15) > mono_duration.as_secs()));
        let metrics = self.engine.update(
            observation.counters,
            if suspend_gap { 0.0 } else { mono_secs },
        );
        self.previous = now;
        self.previous_wall_secs = Some(timestamp);
        self.sequence = self.sequence.saturating_add(1);
        SystemFrame {
            metrics,
            interfaces: observation.interfaces,
            total_memory_bytes: observation.total_memory_bytes,
            used_memory_bytes: observation.used_memory_bytes,
            disk_available: observation.disk_available,
            cpu_usage_percent: observation.cpu_usage_percent,
            volumes: observation.volumes,
            sequence: self.sequence,
            storage: StorageSample {
                counters: observation.counters,
                timestamp,
                elapsed_millis: u64::try_from(now.duration_since(self.origin).as_millis())
                    .unwrap_or(u64::MAX),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SystemObservation, SystemReducer};
    use crate::metrics::CounterSample;
    use std::time::{Duration, Instant};

    fn observation(down: u64) -> SystemObservation {
        SystemObservation {
            counters: CounterSample::new(down, 2, 3, 4),
            interfaces: vec!["Ethernet".to_owned()],
            total_memory_bytes: 16_000,
            used_memory_bytes: 4_000,
            disk_available: true,
            cpu_usage_percent: 12.5,
            volumes: Vec::new(),
        }
    }

    #[test]
    fn first_frame_is_a_baseline_with_observation_metadata_and_storage_order() {
        let origin = Instant::now();
        let mut core = SystemReducer::new(origin);
        let frame = core.observe(observation(1_000), origin + Duration::from_secs(3), 100, 0);
        assert_eq!(frame.sequence, 1);
        assert_eq!(frame.metrics.session_network_down, 0);
        assert_eq!(frame.metrics.session_duration_secs, 0);
        assert_eq!(frame.interfaces, ["Ethernet"]);
        assert_eq!(
            (frame.total_memory_bytes, frame.used_memory_bytes),
            (16_000, 4_000)
        );
        assert!(frame.disk_available);
        let sample = frame.storage.observed();
        assert_eq!((sample.timestamp, sample.elapsed_millis), (100, 3_000));
        assert_eq!(sample.bytes, [1_000, 2, 3, 4]);
    }

    #[test]
    fn rates_use_actual_observed_interval_not_nominal_worker_cadence() {
        let origin = Instant::now();
        let mut core = SystemReducer::new(origin);
        let _ = core.observe(observation(1_000), origin + Duration::from_secs(3), 100, 0);
        let frame = core.observe(observation(2_000), origin + Duration::from_secs(5), 102, 0);
        assert_eq!(frame.metrics.network_down_bps, 500);
        assert_eq!(frame.metrics.session_network_down, 1_000);
        assert_eq!(frame.metrics.session_duration_secs, 2);
        assert_eq!(frame.sequence, 2);
    }

    #[test]
    fn reset_generation_rebaselines_session_but_not_history_counters() {
        let origin = Instant::now();
        let mut core = SystemReducer::new(origin);
        let _ = core.observe(observation(100), origin, 100, 0);
        let _ = core.observe(observation(200), origin + Duration::from_secs(1), 101, 0);
        let reset = core.observe(observation(9_000), origin + Duration::from_secs(2), 102, 7);
        assert_eq!(reset.metrics.session_network_down, 0);
        assert_eq!(reset.metrics.network_down_bps, 0);
        assert_eq!(reset.metrics.session_duration_secs, 0);
        assert_eq!(reset.storage.observed().bytes, [9_000, 2, 3, 4]);
        let next = core.observe(observation(9_100), origin + Duration::from_secs(3), 103, 7);
        assert_eq!(next.metrics.session_network_down, 100);
        assert_eq!(next.metrics.session_duration_secs, 1);
        assert_eq!(next.sequence, 4);
    }

    #[test]
    fn wall_clock_rollback_does_not_change_rate_or_elapsed_measurement() {
        let origin = Instant::now();
        let mut core = SystemReducer::new(origin);
        let _ = core.observe(observation(100), origin, 100, 0);
        let frame = core.observe(observation(300), origin + Duration::from_secs(2), 50, 0);
        assert_eq!(frame.metrics.network_down_bps, 100);
        assert_eq!(frame.metrics.session_duration_secs, 2);
        assert_eq!(frame.storage.observed().timestamp, 50);
        assert_eq!(frame.storage.observed().elapsed_millis, 2_000);
    }

    #[test]
    fn backwards_monotonic_input_never_regresses_storage_elapsed_or_creates_a_rate() {
        let origin = Instant::now();
        let mut core = SystemReducer::new(origin);
        let _ = core.observe(observation(100), origin + Duration::from_secs(3), 100, 0);
        let frame = core.observe(observation(200), origin + Duration::from_secs(2), 101, 0);
        assert_eq!(frame.metrics.network_down_bps, 0);
        assert_eq!(frame.metrics.session_network_down, 100);
        assert_eq!(frame.storage.observed().elapsed_millis, 3_000);
        let next = core.observe(observation(300), origin + Duration::from_secs(5), 102, 0);
        assert_eq!(next.metrics.network_down_bps, 50);
        assert_eq!(next.metrics.session_duration_secs, 2);
    }

    #[test]
    fn sequence_saturates_without_resetting_accounting() {
        let origin = Instant::now();
        let mut core = SystemReducer::new(origin);
        core.sequence = u64::MAX - 1;
        let _ = core.observe(observation(100), origin, 100, 0);
        let frame = core.observe(observation(200), origin + Duration::from_secs(1), 101, 0);
        assert_eq!(frame.sequence, u64::MAX);
        assert_eq!(frame.metrics.session_network_down, 100);
    }

    #[test]
    fn suspend_gap_suppresses_hibernation_rate_spike_without_losing_bytes() {
        let origin = Instant::now();
        let mut core = SystemReducer::new(origin);
        let _ = core.observe(observation(1_000), origin, 100, 0);
        let normal = core.observe(observation(2_000), origin + Duration::from_secs(1), 101, 0);
        assert_eq!(normal.metrics.network_down_bps, 1_000);
        assert_eq!(normal.metrics.session_duration_secs, 1);

        // Simulate wakeup after 1h wall jump while monotonic paused during sleep.
        let wake_wall = core.observe(
            observation(502_000),
            origin + Duration::from_secs(2),
            3_701,
            0,
        );
        assert_eq!(wake_wall.metrics.network_down_bps, 0);
        assert_eq!(wake_wall.metrics.session_duration_secs, 1);
        assert_eq!(wake_wall.metrics.session_network_down, 501_000);

        // Simulate wakeup where monotonic clock jumped >= 30s directly.
        let wake_mono = core.observe(
            observation(902_000),
            origin + Duration::from_secs(47),
            3_746,
            0,
        );
        assert_eq!(wake_mono.metrics.network_down_bps, 0);
        assert_eq!(wake_mono.metrics.session_duration_secs, 1);
        assert_eq!(wake_mono.metrics.session_network_down, 901_000);

        let resumed = core.observe(
            observation(903_000),
            origin + Duration::from_secs(48),
            3_747,
            0,
        );
        assert_eq!(resumed.metrics.network_down_bps, 1_000);
        assert_eq!(resumed.metrics.session_duration_secs, 2);
        assert_eq!(resumed.metrics.session_network_down, 902_000);
    }
}
