//! Liveness only: a heartbeat does not imply a valid measurement.
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkerKind {
    System,
    Processes,
    Storage,
}

impl WorkerKind {
    fn index(self) -> usize {
        match self {
            Self::System => 0,
            Self::Processes => 1,
            Self::Storage => 2,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Processes => "processes",
            Self::Storage => "storage",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkerStatus {
    NotStarted,
    Running,
    Warning,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerSnapshot {
    pub kind: WorkerKind,
    pub status: WorkerStatus,
    pub heartbeat_age_millis: Option<u64>,
    pub ticks: u64,
    pub last_error: Option<String>,
}

struct State {
    generation: u64,
    status: WorkerStatus,
    heartbeat: Option<Instant>,
    ticks: u64,
    last_error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            generation: 0,
            status: WorkerStatus::NotStarted,
            heartbeat: None,
            ticks: 0,
            last_error: None,
        }
    }
}

#[derive(Default)]
pub struct WorkerHealth {
    states: Mutex<[State; 3]>,
}

impl WorkerHealth {
    fn lock(&self) -> MutexGuard<'_, [State; 3]> {
        self.states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn start(self: &Arc<Self>, kind: WorkerKind) -> WorkerGuard {
        let generation = {
            let mut states = self.lock();
            let state = &mut states[kind.index()];
            let generation = state.generation.wrapping_add(1);
            *state = State {
                generation,
                status: WorkerStatus::Running,
                ..State::default()
            };
            generation
        };
        WorkerGuard {
            registry: Arc::clone(self),
            kind,
            generation,
        }
    }

    pub fn snapshot(&self) -> Vec<WorkerSnapshot> {
        let states = self.lock();
        let now = Instant::now();
        [
            WorkerKind::System,
            WorkerKind::Processes,
            WorkerKind::Storage,
        ]
        .into_iter()
        .map(|kind| {
            let state = &states[kind.index()];
            WorkerSnapshot {
                kind,
                status: state.status,
                heartbeat_age_millis: state.heartbeat.map(|heartbeat| {
                    u64::try_from(now.saturating_duration_since(heartbeat).as_millis())
                        .unwrap_or(u64::MAX)
                }),
                ticks: state.ticks,
                last_error: state.last_error.clone(),
            }
        })
        .collect()
    }
}

pub struct WorkerGuard {
    registry: Arc<WorkerHealth>,
    kind: WorkerKind,
    generation: u64,
}

fn bounded_warning(value: &str) -> String {
    let mut end = value.len().min(512);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

impl WorkerGuard {
    pub fn beat(&self) {
        let mut states = self.registry.lock();
        let state = &mut states[self.kind.index()];
        if state.generation != self.generation {
            return;
        }
        state.heartbeat = Some(Instant::now());
        state.ticks = state.ticks.saturating_add(1);
    }

    /// Logs transitions only; heartbeat calls never produce diagnostics.
    pub fn warn(&self, warning: Option<&str>) {
        let warning = warning.map(bounded_warning);
        {
            let mut states = self.registry.lock();
            let state = &mut states[self.kind.index()];
            if state.generation != self.generation || state.last_error == warning {
                return;
            }
            state.last_error.clone_from(&warning);
            state.status = if warning.is_some() {
                WorkerStatus::Warning
            } else {
                WorkerStatus::Running
            };
        }
        let message = match warning {
            Some(warning) => format!("{}: {warning}", self.kind.name()),
            None => format!("{}: warning cleared", self.kind.name()),
        };
        crate::diagnostics::record("workerWarning", &message);
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        let failed = thread::panicking();
        {
            let mut states = self.registry.lock();
            let state = &mut states[self.kind.index()];
            if state.generation != self.generation {
                return;
            }
            state.status = if failed {
                WorkerStatus::Failed
            } else {
                WorkerStatus::Stopped
            };
            if failed {
                state.last_error = Some("worker thread panicked".to_owned());
            }
        }
        if failed {
            crate::diagnostics::record(
                "workerFailed",
                &format!("{}: worker thread panicked", self.kind.name()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn fixed_registry_defaults_to_not_started_without_heartbeat() {
        let registry = WorkerHealth::default();
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.len(), 3);
        assert_eq!(snapshot[0].kind, WorkerKind::System);
        assert_eq!(snapshot[1].kind, WorkerKind::Processes);
        assert_eq!(snapshot[2].kind, WorkerKind::Storage);
        for item in snapshot {
            assert_eq!(item.status, WorkerStatus::NotStarted);
            assert_eq!(item.heartbeat_age_millis, None);
            assert_eq!(item.ticks, 0);
            assert!(item.last_error.is_none());
        }
    }

    #[test]
    fn heartbeat_is_explicit_monotonic_and_stopped_on_normal_drop() {
        let registry = Arc::new(WorkerHealth::default());
        let guard = registry.start(WorkerKind::System);
        assert_eq!(registry.snapshot()[0].heartbeat_age_millis, None);
        guard.beat();
        thread::sleep(Duration::from_millis(15));
        let item = &registry.snapshot()[0];
        assert_eq!(item.ticks, 1);
        assert!(item.heartbeat_age_millis.unwrap() >= 10);
        guard.beat();
        assert_eq!(registry.snapshot()[0].ticks, 2);
        drop(guard);
        assert_eq!(registry.snapshot()[0].status, WorkerStatus::Stopped);
    }

    #[test]
    fn warnings_change_only_on_new_value_and_survive_beats() {
        let registry = Arc::new(WorkerHealth::default());
        let guard = registry.start(WorkerKind::Storage);
        guard.warn(Some("unavailable"));
        guard.warn(Some("unavailable"));
        guard.beat();
        let item = &registry.snapshot()[2];
        assert_eq!(item.status, WorkerStatus::Warning);
        assert_eq!(item.last_error.as_deref(), Some("unavailable"));
        guard.warn(None);
        assert_eq!(registry.snapshot()[2].status, WorkerStatus::Running);
        assert!(registry.snapshot()[2].last_error.is_none());
    }

    #[test]
    fn panic_is_not_swallowed_and_marks_failed() {
        let registry = Arc::new(WorkerHealth::default());
        let other = Arc::clone(&registry);
        let result = thread::spawn(move || {
            let guard = other.start(WorkerKind::Processes);
            guard.beat();
            panic!("worker test panic");
        })
        .join();
        assert!(result.is_err());
        let item = &registry.snapshot()[1];
        assert_eq!(item.status, WorkerStatus::Failed);
        assert!(item.last_error.is_some());
        assert_eq!(item.ticks, 1);
    }

    #[test]
    fn obsolete_guard_cannot_overwrite_restarted_worker() {
        let registry = Arc::new(WorkerHealth::default());
        let old = registry.start(WorkerKind::System);
        old.beat();
        let new = registry.start(WorkerKind::System);
        old.warn(Some("stale"));
        old.beat();
        drop(old);
        let item = &registry.snapshot()[0];
        assert_eq!(item.status, WorkerStatus::Running);
        assert_eq!(item.ticks, 0);
        assert!(item.last_error.is_none());
        new.beat();
        assert_eq!(registry.snapshot()[0].ticks, 1);
    }

    #[test]
    fn poison_recovery_and_serialization_contract() {
        let registry = Arc::new(WorkerHealth::default());
        let other = Arc::clone(&registry);
        let _ = thread::spawn(move || {
            let _lock = other.states.lock().unwrap();
            panic!("poison test");
        })
        .join();
        let guard = registry.start(WorkerKind::Storage);
        guard.beat();
        let json = serde_json::to_value(registry.snapshot()).unwrap();
        assert_eq!(json[2]["kind"], "storage");
        assert_eq!(json[2]["status"], "running");
        assert_eq!(json[2]["ticks"], 1);
        assert!(json[2].get("heartbeatAgeMillis").is_some());
        assert!(json[2].get("lastError").is_some());
    }
}
