//! Bounded best-effort diagnostics. Only the worker (or panic hook) touches disk.
use serde::Serialize;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, Once, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const QUEUE_CAPACITY: usize = 128;
const MESSAGE_BYTES: usize = 512;
const FILE_BYTES: u64 = 256 * 1024;
const FATAL_BYTES: usize = 4096;
const EVENT_FILE: &str = "events.jsonl";
const PREVIOUS_FILE: &str = "events.1.jsonl";
const FATAL_FILE: &str = "last-fatal.txt";
/// Maximum combined byte budget for active, rotated, and fatal diagnostics logs.
pub const MAX_DIAGNOSTICS_BYTES: u64 = FILE_BYTES * 2 + FATAL_BYTES as u64;
/// Filenames managed inside the `diagnostics` directory.
pub const DIAGNOSTIC_FILES: [&str; 3] = [EVENT_FILE, PREVIOUS_FILE, FATAL_FILE];

fn recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn bounded_text(value: &str, limit: usize) -> String {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    pub dropped_events: u64,
    pub last_error: Option<String>,
    pub active: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    timestamp_millis: u64,
    kind: String,
    message: String,
}

impl Event {
    fn new(kind: &str, message: &str) -> Self {
        Self {
            timestamp_millis: u64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
            )
            .unwrap_or(u64::MAX),
            kind: bounded_text(kind, MESSAGE_BYTES),
            message: bounded_text(message, MESSAGE_BYTES),
        }
    }
}

struct Shared {
    directory: PathBuf,
    dropped: AtomicU64,
    last_error: Mutex<Option<String>>,
    active: AtomicBool,
    accepting: AtomicBool,
    in_flight: AtomicUsize,
    stop: AtomicBool,
    finished: AtomicBool,
    fatal_lock: Mutex<()>,
    clear_requests: Mutex<Vec<mpsc::SyncSender<Result<(), String>>>>,
}

struct Admission<'a>(&'a Shared);

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Clone)]
pub struct LogSink {
    tx: mpsc::SyncSender<Event>,
    shared: Arc<Shared>,
}

impl LogSink {
    fn new(tx: mpsc::SyncSender<Event>, directory: PathBuf) -> Self {
        Self {
            tx,
            shared: Arc::new(Shared {
                directory,
                dropped: AtomicU64::new(0),
                last_error: Mutex::new(None),
                active: AtomicBool::new(false),
                accepting: AtomicBool::new(true),
                in_flight: AtomicUsize::new(0),
                stop: AtomicBool::new(false),
                finished: AtomicBool::new(false),
                fatal_lock: Mutex::new(()),
                clear_requests: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Never waits for queue space or filesystem work.
    pub fn record(&self, kind: &'static str, message: &str) {
        // Register before rechecking admission so closing cannot miss a sender
        // that has already observed accepting=true. The token also unwinds safely.
        self.shared.in_flight.fetch_add(1, Ordering::SeqCst);
        let _admission = Admission(&self.shared);
        if !self.shared.accepting.load(Ordering::SeqCst)
            || self.tx.try_send(Event::new(kind, message)).is_err()
        {
            self.shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[must_use]
    pub fn status(&self) -> LogStatus {
        LogStatus {
            dropped_events: self.shared.dropped.load(Ordering::Relaxed),
            last_error: recover(&self.shared.last_error).clone(),
            active: self.shared.active.load(Ordering::Acquire),
        }
    }

    fn set_error(&self, message: &str) {
        *recover(&self.shared.last_error) = Some(bounded_text(message, MESSAGE_BYTES));
    }

    /// Safely clears active, rotated, and fatal diagnostics logs through the worker thread.
    ///
    /// # Errors
    /// Returns an error if the worker times out or filesystem cleanup fails.
    pub fn clear(&self) -> Result<(), String> {
        if self.shared.finished.load(Ordering::Acquire) {
            return self.clear_when_stopped();
        }
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        recover(&self.shared.clear_requests).push(done_tx);
        match done_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Disconnected)
                if self.shared.finished.load(Ordering::Acquire) =>
            {
                self.clear_when_stopped()
            }
            Err(_) => Err("Hết thời gian chờ dọn nhật ký chẩn đoán".to_owned()),
        }
    }

    fn clear_when_stopped(&self) -> Result<(), String> {
        clear_directory_files(&self.shared.directory, &self.shared.fatal_lock)
            .map_err(|error| error.to_string())?;
        self.shared.dropped.store(0, Ordering::Relaxed);
        *recover(&self.shared.last_error) = None;
        Ok(())
    }

    fn write_fatal(&self, message: &str) {
        let _lock = recover(&self.shared.fatal_lock);
        let result = (|| {
            fs::create_dir_all(&self.shared.directory)?;
            let mut file = File::create(self.shared.directory.join(FATAL_FILE))?;
            file.write_all(bounded_text(message, FATAL_BYTES).as_bytes())
        })();
        if let Err(error) = result {
            self.set_error(&format!("last-fatal write: {error}"));
        }
    }
}

pub struct LogOwner {
    sink: LogSink,
    completed: mpsc::Receiver<()>,
    stopped: bool,
}

impl LogOwner {
    /// Only thread creation is synchronous; filesystem errors appear in status.
    ///
    /// # Errors
    /// Returns an error if the diagnostics worker thread cannot be spawned.
    pub fn start(directory: PathBuf) -> io::Result<Self> {
        let (tx, rx) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (done_tx, completed) = mpsc::channel();
        let sink = LogSink::new(tx, directory);
        let worker_sink = sink.clone();
        thread::Builder::new()
            .name("diagnostics-writer".to_owned())
            .spawn(move || {
                run_worker(&worker_sink, &rx);
                worker_sink.shared.accepting.store(false, Ordering::SeqCst);
                worker_sink.shared.active.store(false, Ordering::Release);
                // Close all filesystem handles before publishing completion.
                worker_sink.shared.finished.store(true, Ordering::Release);
                let _ = done_tx.send(());
            })?;
        Ok(Self {
            sink,
            completed,
            stopped: false,
        })
    }

    #[must_use]
    pub fn sink(&self) -> LogSink {
        self.sink.clone()
    }

    /// Requests a drain, waits at most `timeout`, and can be called again on timeout.
    pub fn shutdown(&mut self, timeout: Duration) -> bool {
        self.request_stop();
        if self.stopped {
            return true;
        }
        self.stopped = match self.completed.recv_timeout(timeout) {
            Ok(()) => true,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.sink.shared.finished.load(Ordering::Acquire)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => false,
        };
        self.stopped
    }

    fn request_stop(&self) {
        self.sink.shared.accepting.store(false, Ordering::SeqCst);
        self.sink.shared.stop.store(true, Ordering::Release);
    }
}

impl Drop for LogOwner {
    fn drop(&mut self) {
        self.request_stop();
    }
}

fn run_worker(sink: &LogSink, rx: &mpsc::Receiver<Event>) {
    let result = (|| -> io::Result<()> {
        let mut writer = EventWriter::open(&sink.shared.directory)?;
        {
            let _lock = recover(&sink.shared.fatal_lock);
            bound_existing_fatal(&sink.shared.directory)?;
        }
        sink.shared.active.store(true, Ordering::Release);
        loop {
            drain_clear_requests(sink, rx, &mut writer);
            let event = if sink.shared.stop.load(Ordering::Acquire) {
                match next_after_close(sink, rx) {
                    Some(event) => event,
                    None => break,
                }
            } else {
                match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(event) => event,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            };
            if let Err(error) = writer.write(&event) {
                sink.shared.dropped.fetch_add(1, Ordering::Relaxed);
                return Err(error);
            }
        }
        drain_clear_requests(sink, rx, &mut writer);
        Ok(())
    })();
    if let Err(error) = result {
        sink.shared.accepting.store(false, Ordering::SeqCst);
        sink.shared.active.store(false, Ordering::Release);
        sink.set_error(&format!("diagnostics disk: {error}"));
        while next_after_close(sink, rx).is_some() {
            sink.shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
        let waiters = std::mem::take(&mut *recover(&sink.shared.clear_requests));
        for waiter in waiters {
            let _ = waiter.send(Err(error.to_string()));
        }
    }
}

fn drain_clear_requests(sink: &LogSink, rx: &mpsc::Receiver<Event>, writer: &mut EventWriter) {
    let waiters = std::mem::take(&mut *recover(&sink.shared.clear_requests));
    if waiters.is_empty() {
        return;
    }
    while rx.try_recv().is_ok() {}
    let outcome = writer
        .clear(&sink.shared.fatal_lock)
        .map_err(|error| error.to_string());
    if outcome.is_ok() {
        sink.shared.dropped.store(0, Ordering::Relaxed);
        *recover(&sink.shared.last_error) = None;
    }
    for waiter in waiters {
        let _ = waiter.send(outcome.clone());
    }
}

/// Admission must already be closed. Only the disk worker waits here.
fn next_after_close(sink: &LogSink, rx: &mpsc::Receiver<Event>) -> Option<Event> {
    loop {
        // This sample MUST precede the final empty check: an admitted producer
        // can enqueue and release its token between those two observations.
        let quiescent = sink.shared.in_flight.load(Ordering::SeqCst) == 0;
        match rx.try_recv() {
            Ok(event) => return Some(event),
            Err(_) if quiescent => return None,
            Err(_) => {}
        }
        // An admitted producer may be descheduled. Do not spin or impose any
        // wait on producers; the owner's completion wait remains timeout-bound.
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(event) => return Some(event),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn bound_existing_fatal(directory: &Path) -> io::Result<()> {
    let path = directory.join(FATAL_FILE);
    match fs::metadata(&path) {
        Ok(metadata) if metadata.len() > FATAL_BYTES as u64 => {
            let mut bytes = Vec::with_capacity(FATAL_BYTES);
            File::open(&path)?
                .take(FATAL_BYTES as u64)
                .read_to_end(&mut bytes)?;
            let text = String::from_utf8_lossy(&bytes);
            File::create(path)?.write_all(bounded_text(&text, FATAL_BYTES).as_bytes())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

struct EventWriter {
    directory: PathBuf,
    file: Option<File>,
    bytes: u64,
}

impl EventWriter {
    fn open(directory: &Path) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        for name in [EVENT_FILE, PREVIOUS_FILE] {
            let path = directory.join(name);
            match fs::metadata(&path) {
                Ok(metadata) if metadata.len() > FILE_BYTES => {
                    File::create(path)?;
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join(EVENT_FILE))?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            directory: directory.to_owned(),
            file: Some(file),
            bytes,
        })
    }

    fn write(&mut self, event: &Event) -> io::Result<()> {
        let mut line = serde_json::to_vec(event).map_err(io::Error::other)?;
        line.push(b'\n');
        let length = u64::try_from(line.len()).map_err(io::Error::other)?;
        if length > FILE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "event exceeds file bound",
            ));
        }
        if self.bytes + length > FILE_BYTES {
            self.rotate()?;
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("event file closed"))?;
        file.write_all(&line)?;
        self.bytes += length;
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.take();
        let previous = self.directory.join(PREVIOUS_FILE);
        remove_if_present(&previous)?;
        fs::rename(self.directory.join(EVENT_FILE), previous)?;
        self.file = Some(File::create(self.directory.join(EVENT_FILE))?);
        self.bytes = 0;
        Ok(())
    }

    fn clear(&mut self, fatal_lock: &Mutex<()>) -> io::Result<()> {
        self.file.take();
        clear_directory_files(&self.directory, fatal_lock)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.directory.join(EVENT_FILE))?;
        self.file = Some(file);
        self.bytes = 0;
        Ok(())
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn clear_directory_files(directory: &Path, fatal_lock: &Mutex<()>) -> io::Result<()> {
    remove_if_present(&directory.join(PREVIOUS_FILE))?;
    {
        let _lock = recover(fatal_lock);
        remove_if_present(&directory.join(FATAL_FILE))?;
    }
    if directory.exists() {
        File::create(directory.join(EVENT_FILE))?;
    }
    Ok(())
}

static GLOBAL: OnceLock<Mutex<Option<LogSink>>> = OnceLock::new();
static PANIC_HOOK: Once = Once::new();

pub fn install_global(sink: LogSink) {
    *recover(GLOBAL.get_or_init(|| Mutex::new(None))) = Some(sink);
}

fn global_sink() -> Option<LogSink> {
    GLOBAL.get().and_then(|global| recover(global).clone())
}

pub fn record(kind: &'static str, message: &str) {
    if let Some(sink) = global_sink() {
        sink.record(kind, message);
    }
}

/// Installs once, preserves the previous hook, and does not catch the panic.
pub fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Some(sink) = global_sink() {
                let payload = if let Some(value) = info.payload().downcast_ref::<&str>() {
                    *value
                } else if let Some(value) = info.payload().downcast_ref::<String>() {
                    value.as_str()
                } else {
                    "non-string panic payload"
                };
                let mut message = bounded_text(payload, FATAL_BYTES - MESSAGE_BYTES);
                if let Some(location) = info.location() {
                    message.push_str("\nat ");
                    message.push_str(&bounded_text(location.file(), MESSAGE_BYTES / 2));
                    let _ = write!(message, ":{}:{}", location.line(), location.column());
                }
                sink.write_fatal(&message);
                sink.record("panic", &message);
            }
            previous(info);
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "netdisk-diagnostics-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn truncation_preserves_utf8_and_byte_limits() {
        let value = "\u{1f680}".repeat(200);
        let bounded = bounded_text(&value, MESSAGE_BYTES);
        assert_eq!(bounded.len(), 512);
        assert_eq!(bounded.chars().count(), 128);
        assert_eq!(bounded_text("a\u{1f680}", 4), "a");
    }

    #[test]
    fn full_queue_drops_promptly_and_counts_exactly() {
        let (tx, _rx) = mpsc::sync_channel(QUEUE_CAPACITY);
        let sink = LogSink::new(tx, PathBuf::new());
        let started = Instant::now();
        for _ in 0..10_000 {
            sink.record("test", "message");
        }
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(sink.status().dropped_events, 10_000 - QUEUE_CAPACITY as u64);
    }

    #[test]
    fn worker_drains_serialized_json_and_stops_while_sink_survives() {
        let dir = TestDir::new();
        let mut owner = LogOwner::start(dir.0.clone()).unwrap();
        let sink = owner.sink();
        sink.record("test", "line\nquote\"\u{1f680}");
        assert!(owner.shutdown(Duration::from_secs(3)));
        assert!(!sink.status().active);
        let data = std::fs::read_to_string(dir.0.join(EVENT_FILE)).unwrap();
        assert_eq!(data.lines().count(), 1);
        let value: serde_json::Value = serde_json::from_str(data.trim()).unwrap();
        assert_eq!(value["message"], "line\nquote\"\u{1f680}");
        sink.record("after", "stop");
        assert_eq!(sink.status().dropped_events, 1);
        assert!(owner.shutdown(Duration::ZERO));
    }

    #[test]
    fn idle_worker_exits_and_drop_does_not_join() {
        let dir = TestDir::new();
        let owner = LogOwner::start(dir.0.clone()).unwrap();
        let sink = owner.sink();
        let started = Instant::now();
        drop(owner);
        assert!(started.elapsed() < Duration::from_millis(250));
        let deadline = Instant::now() + Duration::from_secs(3);
        while !sink.shared.finished.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(sink.shared.finished.load(Ordering::Acquire));
        assert!(!sink.status().active);
    }

    #[test]
    fn shutdown_timeout_does_not_wait_for_completion() {
        let (tx, _rx) = mpsc::sync_channel(QUEUE_CAPACITY);
        let sink = LogSink::new(tx, PathBuf::new());
        let (done_tx, completed) = mpsc::channel();
        let mut owner = LogOwner {
            sink,
            completed,
            stopped: false,
        };
        let started = Instant::now();
        assert!(!owner.shutdown(Duration::from_millis(10)));
        assert!(started.elapsed() < Duration::from_millis(250));
        done_tx.send(()).unwrap();
        assert!(owner.shutdown(Duration::ZERO));
    }

    #[test]
    fn shutdown_waits_for_admitted_sender_and_writes_its_late_row() {
        let dir = TestDir::new();
        let mut owner = LogOwner::start(dir.0.clone()).unwrap();
        let sink = owner.sink();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !sink.status().active && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(sink.status().active);
        // Simulate a producer paused after admission but before try_send.
        sink.shared.in_flight.fetch_add(1, Ordering::SeqCst);
        let completed_early = owner.shutdown(Duration::from_millis(100));
        let sent = sink
            .tx
            .try_send(Event::new("late", "admitted before close"));
        sink.shared.in_flight.fetch_sub(1, Ordering::SeqCst);
        assert!(
            !completed_early,
            "shutdown completed with an admitted sender"
        );
        assert!(sent.is_ok());
        assert!(owner.shutdown(Duration::from_secs(3)));
        let data = fs::read_to_string(dir.0.join(EVENT_FILE)).unwrap();
        let event: serde_json::Value = serde_json::from_str(data.trim()).unwrap();
        assert_eq!(event["message"], "admitted before close");
        assert_eq!(sink.status().dropped_events, 0);
    }

    #[test]
    fn disk_error_waits_for_admitted_sender_and_counts_queued_and_late_rows() {
        let dir = TestDir::new();
        let path = dir.0.join("not-a-directory");
        fs::write(&path, b"x").unwrap();
        let (tx, rx) = mpsc::sync_channel(QUEUE_CAPACITY);
        let sink = LogSink::new(tx, path);
        sink.tx
            .try_send(Event::new("queued", "before failure"))
            .unwrap();
        sink.shared.in_flight.fetch_add(1, Ordering::SeqCst);
        let worker_sink = sink.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            run_worker(&worker_sink, &rx);
            done_tx.send(()).unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while sink.status().last_error.is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(sink.status().last_error.is_some());
        let completed_early = done_rx.recv_timeout(Duration::from_millis(100));
        let sent = sink
            .tx
            .try_send(Event::new("late", "admitted before failure"));
        sink.shared.in_flight.fetch_sub(1, Ordering::SeqCst);
        assert!(matches!(
            completed_early,
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert!(sent.is_ok());
        done_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        worker.join().unwrap();
        assert_eq!(sink.status().dropped_events, 2);
        sink.record("rejected", "after failure");
        assert_eq!(sink.status().dropped_events, 3);
    }

    #[test]
    fn rotation_includes_escaped_line_bytes_and_keeps_two_bounded_files() {
        let dir = TestDir::new();
        let mut writer = EventWriter::open(&dir.0).unwrap();
        let event = Event::new("escape", &"\u{0001}".repeat(512));
        for _ in 0..250 {
            writer.write(&event).unwrap();
        }
        drop(writer);
        for name in [EVENT_FILE, PREVIOUS_FILE] {
            let path = dir.0.join(name);
            assert!(std::fs::metadata(&path).unwrap().len() <= FILE_BYTES);
            let data = std::fs::read_to_string(path).unwrap();
            assert!(!data.is_empty());
            for line in data.lines() {
                let value: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_eq!(value["message"].as_str().unwrap().len(), 512);
            }
        }
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 2);
    }

    #[test]
    fn startup_bounds_preexisting_files_and_fatal_write_is_utf8() {
        let dir = TestDir::new();
        for name in [EVENT_FILE, PREVIOUS_FILE, FATAL_FILE] {
            std::fs::write(dir.0.join(name), vec![b'x'; 300_000]).unwrap();
        }
        let mut owner = LogOwner::start(dir.0.clone()).unwrap();
        assert!(owner.shutdown(Duration::from_secs(3)));
        let sink = owner.sink();
        sink.write_fatal(&"\u{1f680}".repeat(2000));
        let fatal = std::fs::read_to_string(dir.0.join(FATAL_FILE)).unwrap();
        assert_eq!(fatal.len(), FATAL_BYTES);
        for name in [EVENT_FILE, PREVIOUS_FILE] {
            assert!(std::fs::metadata(dir.0.join(name)).unwrap().len() <= FILE_BYTES);
        }
    }

    #[test]
    fn filesystem_failure_is_visible_without_failing_start() {
        let dir = TestDir::new();
        let file = dir.0.join("not-a-directory");
        std::fs::write(&file, b"x").unwrap();
        let mut owner = LogOwner::start(file).unwrap();
        assert!(owner.shutdown(Duration::from_secs(3)));
        let status = owner.sink().status();
        assert!(!status.active);
        assert!(status.last_error.is_some());
        let json = serde_json::to_value(status).unwrap();
        assert!(json.get("droppedEvents").is_some());
        assert!(json.get("lastError").is_some());
    }

    #[test]
    fn poisoned_status_lock_is_recovered() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let sink = LogSink::new(tx, PathBuf::new());
        let other = sink.clone();
        let _ = thread::spawn(move || {
            let _lock = other.shared.last_error.lock().unwrap();
            panic!("poison test");
        })
        .join();
        sink.set_error("recovered");
        assert_eq!(sink.status().last_error.as_deref(), Some("recovered"));
    }

    #[test]
    fn panic_hook_chains_and_global_health_logging_is_transition_only() {
        let dir = TestDir::new();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }
        let output = command
            .arg("--exact")
            .arg("diagnostics::tests::panic_hook_child")
            .env("NETDISK_DIAGNOSTICS_HOOK_TEST", &dir.0)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let fatal = std::fs::read_to_string(dir.0.join(FATAL_FILE)).unwrap();
        assert!(fatal.len() <= FATAL_BYTES);
        assert!(fatal.starts_with('\u{1f680}'));
        let data = std::fs::read_to_string(dir.0.join(EVENT_FILE)).unwrap();
        let values: Vec<serde_json::Value> = data
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            values
                .iter()
                .filter(|event| event["kind"] == "workerWarning")
                .count(),
            2
        );
        assert_eq!(
            values
                .iter()
                .filter(|event| event["kind"] == "workerFailed")
                .count(),
            1
        );
        assert_eq!(
            values
                .iter()
                .filter(|event| event["kind"] == "panic")
                .count(),
            1
        );
        assert_eq!(values.len(), 4);
    }

    #[test]
    fn panic_hook_child() {
        let Some(path) = std::env::var_os("NETDISK_DIAGNOSTICS_HOOK_TEST") else {
            return;
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let previous_calls = Arc::clone(&calls);
        std::panic::set_hook(Box::new(move |_| {
            previous_calls.fetch_add(1, Ordering::Relaxed);
        }));
        let mut owner = LogOwner::start(PathBuf::from(path)).unwrap();
        install_global(owner.sink());
        install_panic_hook();
        install_panic_hook();
        let registry = Arc::new(crate::health::WorkerHealth::default());
        let guard = registry.start(crate::health::WorkerKind::System);
        guard.warn(Some("transition-test"));
        for _ in 0..100 {
            guard.warn(Some("transition-test"));
            guard.beat();
        }
        guard.warn(None);
        guard.warn(None);
        let result = std::panic::catch_unwind(|| {
            let _guard = registry.start(crate::health::WorkerKind::Processes);
            std::panic::panic_any("\u{1f680}".repeat(2000));
        });
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            registry.snapshot()[1].status,
            crate::health::WorkerStatus::Failed
        );
        assert!(owner.shutdown(Duration::from_secs(3)));
    }

    #[test]
    fn clear_truncates_active_rotated_and_fatal_logs_and_resumes_writing() {
        let dir = TestDir::new();
        fs::write(dir.0.join(PREVIOUS_FILE), b"old-rotated-data").unwrap();
        fs::write(dir.0.join(FATAL_FILE), b"old-fatal-data").unwrap();
        let mut owner = LogOwner::start(dir.0.clone()).unwrap();
        let sink = owner.sink();
        sink.record("before", "old active line");
        let deadline = Instant::now() + Duration::from_secs(3);
        while fs::metadata(dir.0.join(EVENT_FILE))
            .map(|m| m.len())
            .unwrap_or(0)
            == 0
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
        sink.clear().unwrap();
        assert!(!dir.0.join(PREVIOUS_FILE).exists());
        assert!(!dir.0.join(FATAL_FILE).exists());
        assert_eq!(fs::metadata(dir.0.join(EVENT_FILE)).unwrap().len(), 0);

        sink.record("after", "fresh line after clear");
        assert!(owner.shutdown(Duration::from_secs(3)));
        let active = fs::read_to_string(dir.0.join(EVENT_FILE)).unwrap();
        assert!(active.contains("fresh line after clear"));
        assert!(!active.contains("old active line"));
    }
}
