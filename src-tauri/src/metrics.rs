use serde::Serialize;

#[derive(Clone, Copy, Debug, Default)]
pub struct CounterSample {
    pub network_down: u64,
    pub network_up: u64,
    pub disk_read: u64,
    pub disk_write: u64,
}
impl CounterSample {
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
        let elapsed = elapsed_seconds.max(f64::EPSILON);
        if let Some(previous) = self.previous {
            let down = current.network_down.saturating_sub(previous.network_down);
            let up = current.network_up.saturating_sub(previous.network_up);
            let read = current.disk_read.saturating_sub(previous.disk_read);
            let write = current.disk_write.saturating_sub(previous.disk_write);
            self.snapshot.network_down_bps = (down as f64 / elapsed) as u64;
            self.snapshot.network_up_bps = (up as f64 / elapsed) as u64;
            self.snapshot.disk_read_bps = (read as f64 / elapsed) as u64;
            self.snapshot.disk_write_bps = (write as f64 / elapsed) as u64;
            self.snapshot.session_network_down += down;
            self.snapshot.session_network_up += up;
            self.snapshot.session_disk_read += read;
            self.snapshot.session_disk_write += write;
        }
        self.session_elapsed_millis += (elapsed * 1_000.0).round() as u64;
        self.snapshot.session_duration_secs = self.session_elapsed_millis / 1_000;
        self.previous = Some(current);
        self.snapshot.clone()
    }
    pub fn reset_session_totals(&mut self) {
        self.snapshot.session_network_down = 0;
        self.snapshot.session_network_up = 0;
        self.snapshot.session_disk_read = 0;
        self.snapshot.session_disk_write = 0;
        self.session_elapsed_millis = 0;
        self.snapshot.session_duration_secs = 0;
    }
    pub fn snapshot(&self) -> MetricsSnapshot {
        self.snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{CounterSample, MetricsEngine};

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

        assert_eq!(engine.snapshot().session_network_down, 0);
    }
}
