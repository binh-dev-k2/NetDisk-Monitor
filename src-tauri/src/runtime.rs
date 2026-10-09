use std::{
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

#[derive(Debug, PartialEq, Eq)]
pub enum ShutdownRequest {
    Start,
    Wait,
    Exit,
}

#[derive(Default)]
pub struct ShutdownGate(std::sync::atomic::AtomicU8);
impl ShutdownGate {
    pub fn request(&self) -> ShutdownRequest {
        use std::sync::atomic::Ordering;
        match self
            .0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => ShutdownRequest::Start,
            Err(2) => ShutdownRequest::Exit,
            Err(_) => ShutdownRequest::Wait,
        }
    }
    pub fn complete(&self) {
        self.0.store(2, std::sync::atomic::Ordering::Release);
    }
}

/// Shared cancellation; waits are interruptible and do not busy-poll.
#[derive(Clone, Default)]
pub struct StopToken(Arc<(Mutex<bool>, Condvar)>);

impl StopToken {
    pub fn stop(&self) {
        let (stopped, wake) = &*self.0;
        if let Ok(mut stopped) = stopped.lock() {
            *stopped = true;
            wake.notify_all();
        }
    }

    #[must_use]
    pub fn wait_until(&self, deadline: Instant) -> bool {
        let (stopped, wake) = &*self.0;
        let Ok(mut guard) = stopped.lock() else {
            return false;
        };
        loop {
            if *guard {
                return false;
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return true;
            };
            let Ok((next, _)) = wake.wait_timeout(guard, remaining) else {
                return false;
            };
            guard = next;
        }
    }
}

#[derive(Clone, Copy)]
pub struct ObservedSample {
    pub timestamp: u64,
    pub elapsed_millis: u64,
    pub bytes: [u64; 4],
}

pub struct PendingRecord {
    pub timestamp: u64,
    pub elapsed_millis: u64,
    pub bytes: [u64; 4],
    pub clock_adjusted: bool,
}

pub struct PersistenceCursor {
    baseline: ObservedSample,
    wall_watermark: u64,
}

impl PersistenceCursor {
    #[must_use]
    pub fn new(baseline: ObservedSample, recovered_watermark: Option<u64>) -> Self {
        Self {
            baseline,
            wall_watermark: baseline.timestamp.max(recovered_watermark.unwrap_or(0)),
        }
    }
    #[must_use]
    pub fn prepare(&self, current: ObservedSample) -> Option<PendingRecord> {
        let elapsed_millis = current
            .elapsed_millis
            .checked_sub(self.baseline.elapsed_millis)?;
        if elapsed_millis == 0 {
            return None;
        }
        Some(PendingRecord {
            timestamp: current.timestamp.max(self.wall_watermark),
            elapsed_millis,
            bytes: std::array::from_fn(|index| {
                current.bytes[index].saturating_sub(self.baseline.bytes[index])
            }),
            clock_adjusted: current.timestamp < self.wall_watermark,
        })
    }
    pub fn accept(&mut self, current: ObservedSample, record: &PendingRecord) {
        self.baseline = current;
        self.wall_watermark = record.timestamp;
    }
}

#[cfg(test)]
mod tests {
    use super::StopToken;
    use super::{ObservedSample, PersistenceCursor};
    use std::{
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn shutdown_interrupts_a_long_wait() {
        let stop = StopToken::default();
        let worker_stop = stop.clone();
        let worker =
            thread::spawn(move || worker_stop.wait_until(Instant::now() + Duration::from_secs(60)));
        stop.stop();
        assert!(!worker.join().unwrap());
    }

    #[test]
    fn a_deadline_is_reached_without_stopping() {
        let stop = StopToken::default();
        assert!(stop.wait_until(Instant::now()));
        stop.stop();
        assert!(!stop.wait_until(Instant::now()));
    }
    fn sample(timestamp: u64, elapsed_millis: u64, bytes: u64) -> ObservedSample {
        ObservedSample {
            timestamp,
            elapsed_millis,
            bytes: [bytes; 4],
        }
    }
    #[test]
    fn duplicates_cannot_lower_the_accepted_wall_watermark() {
        let mut cursor = PersistenceCursor::new(sample(125, 0, 0), None);
        let pending = cursor.prepare(sample(50, 1000, 10)).unwrap();
        assert_eq!(pending.timestamp, 125);
        cursor.accept(sample(50, 1000, 10), &pending);
        assert!(cursor.prepare(sample(50, 1000, 10)).is_none());
        assert_eq!(cursor.prepare(sample(51, 2000, 20)).unwrap().timestamp, 125);
    }
    #[test]
    fn failed_writes_preserve_cumulative_backlog() {
        let mut cursor = PersistenceCursor::new(sample(100, 0, 0), Some(180));
        let failed = cursor.prepare(sample(101, 1000, 10)).unwrap();
        assert_eq!(failed.timestamp, 180);
        let retry = cursor.prepare(sample(105, 5000, 50)).unwrap();
        assert_eq!(retry.bytes, [50; 4]);
        assert_eq!(retry.elapsed_millis, 5000);
        cursor.accept(sample(105, 5000, 50), &retry);
        assert_eq!(
            cursor.prepare(sample(106, 6000, 60)).unwrap().bytes,
            [10; 4]
        );
    }
    #[test]
    fn repeated_quit_cannot_bypass_an_in_progress_flush() {
        let gate = super::ShutdownGate::default();
        assert_eq!(gate.request(), super::ShutdownRequest::Start);
        assert_eq!(gate.request(), super::ShutdownRequest::Wait);
        gate.complete();
        assert_eq!(gate.request(), super::ShutdownRequest::Exit);
    }
}
