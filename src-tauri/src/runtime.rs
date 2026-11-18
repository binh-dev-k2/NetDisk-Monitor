use std::{
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

#[derive(Debug, PartialEq, Eq)]
pub enum ShutdownRequest {
    Ignore,
    Start,
    Wait,
    Finalize,
    Exit,
}

#[must_use]
pub fn should_focus_existing_instance(args: &[String]) -> bool {
    !args.iter().any(|argument| argument == "--background")
}

#[derive(Default)]
pub struct ShutdownGate(Mutex<ExitState>);

#[derive(Default, PartialEq, Eq)]
enum ExitPhase {
    #[default]
    Running,
    Flushing,
    AwaitingAcknowledgement,
    Finalizing,
    Ready,
}

#[derive(Clone, Copy)]
enum ExitIntent {
    Quit,
    Finalize,
}

#[derive(Default)]
struct ExitState {
    phase: ExitPhase,
    intent: Option<ExitIntent>,
    ignored: u64,
}
impl ShutdownGate {
    pub fn authorize_quit(&self) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A delayed ordinary Quit must not erase the internal drain/exit handshake.
        if !matches!(state.intent, Some(ExitIntent::Finalize)) {
            state.intent = Some(ExitIntent::Quit);
        }
    }

    pub fn authorize_finalization(&self) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(
            state.phase,
            ExitPhase::AwaitingAcknowledgement | ExitPhase::Ready
        ) {
            state.intent = Some(ExitIntent::Finalize);
        }
    }

    #[must_use]
    pub fn request(&self) -> ShutdownRequest {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(intent) = state.intent.take() else {
            state.ignored = state.ignored.saturating_add(1);
            return ShutdownRequest::Ignore;
        };
        match (&state.phase, intent) {
            (ExitPhase::Running, ExitIntent::Quit) => {
                state.phase = ExitPhase::Flushing;
                ShutdownRequest::Start
            }
            (ExitPhase::AwaitingAcknowledgement, ExitIntent::Finalize) => {
                state.phase = ExitPhase::Finalizing;
                ShutdownRequest::Finalize
            }
            (ExitPhase::Ready, ExitIntent::Finalize) => ShutdownRequest::Exit,
            _ => ShutdownRequest::Wait,
        }
    }
    pub fn complete(&self) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.phase == ExitPhase::Flushing {
            state.phase = ExitPhase::AwaitingAcknowledgement;
            state.intent = None;
        }
    }

    pub fn finish(&self) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.phase == ExitPhase::Finalizing {
            state.phase = ExitPhase::Ready;
            state.intent = None;
        }
    }

    #[must_use]
    pub fn ignored_requests(&self) -> u64 {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ignored
    }
}

#[derive(Default)]
struct StopState {
    stopped: bool,
    wake_epoch: u64,
}

/// Shared cancellation; waits are interruptible and do not busy-poll.
#[derive(Clone, Default)]
pub struct StopToken(Arc<(Mutex<StopState>, Condvar)>);

impl StopToken {
    pub fn stop(&self) {
        let (state, wake) = &*self.0;
        if let Ok(mut guard) = state.lock() {
            guard.stopped = true;
            wake.notify_all();
        }
    }

    /// Wakes a waiting worker for a one-off work request without stopping it.
    pub fn wake(&self) {
        let (state, wake) = &*self.0;
        if let Ok(mut guard) = state.lock() {
            guard.wake_epoch = guard.wake_epoch.wrapping_add(1);
            wake.notify_all();
        }
    }

    #[must_use]
    pub fn wait_until(&self, deadline: Instant) -> bool {
        let (state, wake) = &*self.0;
        let Ok(mut guard) = state.lock() else {
            return false;
        };
        let initial_epoch = guard.wake_epoch;
        loop {
            if guard.stopped {
                return false;
            }
            if guard.wake_epoch != initial_epoch {
                return true;
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
    fn second_background_launch_does_not_request_dashboard_focus() {
        assert!(super::should_focus_existing_instance(&[]));
        assert!(super::should_focus_existing_instance(&["app.exe".into()]));
        assert!(!super::should_focus_existing_instance(&[
            "app.exe".into(),
            "--background".into()
        ]));
        assert!(super::should_focus_existing_instance(&[
            "--background.exe".into()
        ]));
    }

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
    fn wake_interrupts_a_wait_without_stopping() {
        let stop = StopToken::default();
        let worker_stop = stop.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let started = Instant::now();
        let worker = thread::spawn(move || {
            entered_tx.send(()).unwrap();
            worker_stop.wait_until(Instant::now() + Duration::from_secs(60))
        });
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        thread::sleep(Duration::from_millis(20));
        stop.wake();
        assert!(worker.join().unwrap());
        assert!(started.elapsed() < Duration::from_secs(2));
        stop.stop();
        assert!(!stop.wait_until(Instant::now()));
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
    fn ordinary_quit_cannot_replace_flush_finalization_intent() {
        let gate = super::ShutdownGate::default();
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Start);
        gate.complete();
        gate.authorize_finalization();
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Finalize);
    }

    #[test]
    fn ordinary_quit_cannot_replace_ready_exit_intent() {
        let gate = super::ShutdownGate::default();
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Start);
        gate.complete();
        gate.authorize_finalization();
        assert_eq!(gate.request(), super::ShutdownRequest::Finalize);
        gate.finish();
        gate.authorize_finalization();
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Exit);
    }

    #[test]
    fn quit_queued_before_completion_cannot_acknowledge_a_failed_flush() {
        let gate = super::ShutdownGate::default();
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Start);
        gate.authorize_quit();
        gate.complete();
        assert_eq!(gate.request(), super::ShutdownRequest::Ignore);
    }

    #[test]
    fn implicit_exit_does_not_begin_shutdown() {
        let gate = super::ShutdownGate::default();
        assert_ne!(
            gate.request(),
            super::ShutdownRequest::Start,
            "closing a view must not stop monitoring without explicit quit intent"
        );
    }

    #[test]
    fn repeated_quit_cannot_bypass_an_in_progress_flush() {
        let gate = super::ShutdownGate::default();
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Start);
        assert_eq!(gate.request(), super::ShutdownRequest::Ignore);
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Wait);
        gate.complete();
        assert_eq!(gate.request(), super::ShutdownRequest::Ignore);
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Wait);
        gate.authorize_finalization();
        assert_eq!(gate.request(), super::ShutdownRequest::Finalize);
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Wait);
        gate.finish();
        assert_eq!(gate.request(), super::ShutdownRequest::Ignore);
        gate.authorize_finalization();
        assert_eq!(gate.request(), super::ShutdownRequest::Exit);
        assert_eq!(gate.request(), super::ShutdownRequest::Ignore);
    }

    #[test]
    fn failed_flush_needs_explicit_acknowledgement() {
        let gate = super::ShutdownGate::default();
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Start);
        gate.complete();
        assert_eq!(gate.request(), super::ShutdownRequest::Ignore);
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Wait);
        gate.authorize_finalization();
        assert_eq!(gate.request(), super::ShutdownRequest::Finalize);
        gate.finish();
        gate.authorize_finalization();
        assert_eq!(gate.request(), super::ShutdownRequest::Exit);
    }

    #[test]
    fn concurrent_quit_requests_never_skip_an_in_progress_flush() {
        let gate = std::sync::Arc::new(super::ShutdownGate::default());
        gate.authorize_quit();
        assert_eq!(gate.request(), super::ShutdownRequest::Start);
        let threads: Vec<_> = (0..16)
            .map(|_| {
                let gate = std::sync::Arc::clone(&gate);
                thread::spawn(move || {
                    gate.authorize_quit();
                    assert!(matches!(
                        gate.request(),
                        super::ShutdownRequest::Wait | super::ShutdownRequest::Ignore
                    ));
                })
            })
            .collect();
        for handle in threads {
            handle.join().unwrap();
        }
        gate.complete();
        assert_eq!(gate.request(), super::ShutdownRequest::Ignore);
    }
}
