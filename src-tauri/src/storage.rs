//! Bounded history for a single persistence worker, not the UI thread.
//!
//! Samples are deltas (down, up, disk read, disk write), assigned wholly to
//! UTC buckets by their supplied timestamp. Duration is monotonic elapsed
//! time, not inferred from wall-clock gaps. Reads include pending samples,
//! newest first, omit gaps, and retain 10,080 minutes / 366 UTC days relative
//! to the newest accepted bucket. Rollback into an earlier minute is rejected;
//! reordering within the same minute is harmless and accepted.
//! The parent worker should clamp a backward wall timestamp to its last
//! accepted wall timestamp, keep monotonic elapsed duration unchanged, and
//! expose a rollback warning through `storageError` without stopping sampling.
//! Clamping intentionally attributes those deltas to the last accepted bucket;
//! it does not synthesize elapsed wall time. This API never silently clamps.
//! Persisted rollback protection has bucket (not individual sample) precision.
//! Only the worker opens this store. It can refresh bounded `RuntimeState` caches
//! each minute; history commands read those caches instead of reopening a file.
//!
//! Call `flush` on the owning worker at shutdown. Drop does NOT promise a flush.
//! Normal recording checkpoints the previous batch on a minute transition.
//! At most the current unflushed batch is lost on a crash. Checkpoints use a
//! fixed two-copy redo journal: complete minute/day after-images are made durable
//! before either ring changes. Recovery overwrites, never adds, those images.
//! A completed checkpoint is mirrored to both journal copies. Torn writes retain
//! the preceding durable journal; an interrupted checkpoint is replayed exactly
//! once at both resolutions. This assumes storage honors durable flush barriers.
//!
//! Format v2: the v1 record region plus two 256-byte journal payloads, isolated
//! in separate aligned 4 KiB regions at its tail.
//! v1 histories are migrated in place without rewriting any valid record bytes.
//! The explicit v2 header prevents older code from ignoring the journal.
//! All integers are little endian. Header: magic, version, record size,
//! minute count, day count, minute period, day period, checksum (8 bytes each).
//! Record: sequence, timestamp, duration, four byte totals, checksum (8 each).
//! FNV-1a checksums detect accidental corruption, not malicious modification.
//! Record checksums are bound to their region and logical index. Valid headers
//! with unsupported layouts are rejected. Unrecoverable v2 journals fail closed,
//! never reset history. Oversized files are rejected without truncation. Normal
//! file length is exactly 1,347,584 bytes (v1 region 1,337,152 + fixed tail 10,432).
//! Exclusive ownership uses the standard file lock API (Rust 1.89+).

use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

const FILE_NAME: &str = "history-v1.bin";
const MAGIC: &[u8; 8] = b"NDMHIST\0";
const HEADER_SIZE: usize = 64;
const RECORD_SIZE: usize = 64;
const MINUTE_COUNT: usize = 10_080;
const DAY_COUNT: usize = 366;
const MINUTE_PERIOD: u64 = 60;
const DAY_PERIOD: u64 = 86_400;
const DAY_BASE: usize = HEADER_SIZE + MINUTE_COUNT * 2 * RECORD_SIZE;
const LEGACY_FILE_SIZE: usize = HEADER_SIZE + (MINUTE_COUNT + DAY_COUNT) * 2 * RECORD_SIZE;
const JOURNAL_RECORD_SIZE: usize = 256;
const JOURNAL_STRIDE: usize = 4096;
const JOURNAL_BASE: usize = LEGACY_FILE_SIZE.div_ceil(JOURNAL_STRIDE) * JOURNAL_STRIDE;
const JOURNAL_SIZE: usize = 2 * JOURNAL_STRIDE;
const JOURNAL_MAGIC: &[u8; 8] = b"NDMTXN2\0";
const FILE_SIZE: usize = JOURNAL_BASE + JOURNAL_SIZE;
/// Exact bounded size of the v2 history file, including its aligned redo journal.
pub const HISTORY_FILE_BYTES: u64 = FILE_SIZE as u64;
const MAX_FILE_SIZE: u64 = HISTORY_FILE_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub timestamp_secs: u64,
    pub duration_millis: u64,
    pub bytes: [u64; 4],
}

pub struct HistoryStore {
    file: File,
    minutes: Ring,
    days: Ring,
    high_water: Option<u64>,
    journal: Journal,
    in_flight: Option<JournalRecord>,
}

impl HistoryStore {
    #[must_use]
    pub fn latest_timestamp_secs(&self) -> Option<u64> {
        self.high_water
    }
    /// Open or recover history. A second owner receives `WouldBlock`.
    ///
    /// # Errors
    /// Returns filesystem/lock errors or `InvalidData` for a checksum-valid
    /// unsupported header/layout, an unrecoverable v2 journal, or an oversized
    /// file. v1 records are retained during explicit in-place migration.
    pub fn open(directory: &Path) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join(FILE_NAME))?;
        file.try_lock().map_err(io::Error::from)?;
        let length = file.metadata()?.len();
        if length > MAX_FILE_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "History exceeds the v2 size bound",
            ));
        }
        let length_usize = usize::try_from(length).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "History length is not addressable",
            )
        })?;
        let mut raw = vec![0; FILE_SIZE];
        file.read_exact(&mut raw[..length_usize])?;
        let journal = recover_or_migrate(&mut file, &mut raw, length)?;
        // Recover again after redo; neither resolution may observe pre-replay data.
        let minutes = Ring::recover(&raw, MINUTE_COUNT, MINUTE_PERIOD, HEADER_SIZE);
        let days = Ring::recover(&raw, DAY_COUNT, DAY_PERIOD, DAY_BASE);
        let high_water = minutes.latest().into_iter().chain(days.latest()).max();
        #[cfg(unix)]
        if length == 0 {
            // fsync the new filename before an accepted checkpoint can be durable.
            File::open(directory)?.sync_all()?;
        }
        Ok(Self {
            file,
            minutes,
            days,
            high_water,
            journal,
            in_flight: None,
        })
    }

    /// Accumulate delta bytes. Zero duration, overflow and older minutes are
    /// rejected before either aggregate changes. On an I/O error retry this
    /// sample: the incoming sample has not been accepted.
    ///
    /// # Errors
    /// Returns `InvalidInput` for zero duration, aggregate overflow or an older
    /// minute; checkpoint I/O errors and sequence exhaustion are propagated.
    pub fn record(
        &mut self,
        timestamp_secs: u64,
        elapsed_millis: u64,
        bytes: [u64; 4],
    ) -> io::Result<()> {
        let minute = timestamp_secs / MINUTE_PERIOD * MINUTE_PERIOD;
        if elapsed_millis == 0 || self.high_water.is_some_and(|latest| minute < latest) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Zero duration or clock rollback into an older minute",
            ));
        }
        // Finish a failed checkpoint before accepting further data into its images.
        if self.in_flight.is_some() {
            self.flush()?;
        }
        let minute_record = self
            .minutes
            .aggregate(timestamp_secs, elapsed_millis, bytes)?;
        let day_record = self.days.aggregate(timestamp_secs, elapsed_millis, bytes)?;
        if self.high_water.is_some_and(|latest| minute > latest) {
            self.flush()?;
        }
        self.minutes.accept(minute_record);
        self.days.accept(day_record);
        self.high_water = Some(minute);
        Ok(())
    }

    /// Return up to `limit` occupied minute buckets, newest first, including
    /// unsynchronized in-memory data. This performs no file I/O.
    ///
    /// # Errors
    /// Currently infallible; `io::Result` preserves the history API contract.
    pub fn read_minutes(&self, limit: usize) -> io::Result<Vec<HistoryRecord>> {
        Ok(self.minutes.read(limit, self.high_water))
    }

    /// Like `read_minutes`, at UTC-day resolution.
    ///
    /// # Errors
    /// Currently infallible; `io::Result` preserves the history API contract.
    pub fn read_days(&self, limit: usize) -> io::Result<Vec<HistoryRecord>> {
        Ok(self.days.read(limit, self.high_water))
    }

    /// Durably journal the pending pair, apply it, then mirror the journal.
    /// Failed flushes retain the exact transaction for an idempotent retry.
    ///
    /// # Errors
    /// Returns write/seek/synchronization errors or `InvalidData` if a sequence
    /// is exhausted. Retrying does not double-count previously accepted data.
    pub fn flush(&mut self) -> io::Result<()> {
        if self.in_flight.is_none() {
            let minute = self.minutes.pending()?;
            let day = self.days.pending()?;
            let (Some(mut minute), Some(mut day)) = (minute, day) else {
                if self.minutes.dirty.is_some() || self.days.dirty.is_some() {
                    return Err(invalid_data("Incomplete checkpoint pair"));
                }
                return Ok(());
            };
            let generation = self
                .journal
                .record
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid_data("History transaction sequence exhausted"))?;
            minute.resequence(generation, MINUTE_PERIOD);
            day.resequence(generation, DAY_PERIOD);
            self.in_flight = Some(JournalRecord {
                generation,
                pending: Some([minute, day]),
            });
        }
        let transaction = self
            .in_flight
            .as_ref()
            .ok_or_else(|| invalid_data("Missing planned checkpoint"))?;
        checkpoint(&mut self.file, transaction, 1 - self.journal.copy)?;
        let transaction = self
            .in_flight
            .take()
            .ok_or_else(|| invalid_data("Missing durable checkpoint"))?;
        let [minute, day] = transaction
            .pending
            .as_ref()
            .ok_or_else(|| invalid_data("Missing durable checkpoint pair"))?;
        self.minutes.committed(minute);
        self.days.committed(day);
        self.journal = Journal {
            record: transaction,
            copy: 1 - self.journal.copy,
        };
        Ok(())
    }
}

#[derive(Clone)]
struct Slot {
    record: Option<HistoryRecord>,
    sequence: u64,
    copy: u8,
}

struct Ring {
    slots: Vec<Slot>,
    period: u64,
    base: usize,
    dirty: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    index: usize,
    sequence: u64,
    copy: u8,
    encoded: [u8; RECORD_SIZE],
}

impl Pending {
    fn address(&self, base: usize) -> usize {
        base + (self.index * 2 + usize::from(self.copy)) * RECORD_SIZE
    }

    fn offset(&self, base: usize) -> u64 {
        self.address(base) as u64
    }

    fn resequence(&mut self, sequence: u64, period: u64) {
        self.sequence = sequence;
        put(&mut self.encoded, 0, sequence);
        let hash = record_checksum(&self.encoded[..56], period, self.index);
        put(&mut self.encoded, 7, hash);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JournalRecord {
    generation: u64,
    pending: Option<[Pending; 2]>,
}

struct Journal {
    record: JournalRecord,
    copy: u8,
}

impl JournalRecord {
    fn encode(&self, copy: u8) -> [u8; JOURNAL_RECORD_SIZE] {
        let mut bytes = [0; JOURNAL_RECORD_SIZE];
        bytes[..8].copy_from_slice(JOURNAL_MAGIC);
        put(&mut bytes, 1, 2);
        put(&mut bytes, 2, self.generation);
        if let Some([minute, day]) = &self.pending {
            put(&mut bytes, 3, 3);
            for (item, base, offset_word, start) in
                [(minute, HEADER_SIZE, 4, 48), (day, DAY_BASE, 5, 112)]
            {
                put(&mut bytes, offset_word, item.offset(base));
                bytes[start..start + RECORD_SIZE].copy_from_slice(&item.encoded);
            }
        }
        let hash = journal_checksum(&bytes[..248], copy);
        put(&mut bytes, 31, hash);
        bytes
    }

    fn decode(bytes: &[u8], copy: u8) -> io::Result<Option<Self>> {
        if word(bytes, 31) != journal_checksum(&bytes[..248], copy) {
            return Ok(None);
        }
        if bytes[..8] != *JOURNAL_MAGIC || word(bytes, 1) != 2 {
            return Err(invalid_data("Unsupported checksum-valid journal format"));
        }
        let generation = word(bytes, 2);
        if generation == 0 || bytes[176..248].iter().any(|&b| b != 0) {
            return Err(invalid_data("Invalid journal generation or reserved bytes"));
        }
        let pending = match word(bytes, 3) {
            0 if bytes[32..176].iter().all(|&b| b == 0) => None,
            3 => {
                let decode = |base: usize, count: usize, period: u64, offset_word, start: usize| {
                    let offset = word(bytes, offset_word);
                    let relative = offset
                        .checked_sub(base as u64)
                        .ok_or_else(|| invalid_data("Journal offset outside record region"))?;
                    if !relative.is_multiple_of(RECORD_SIZE as u64)
                        || relative >= (count * 2 * RECORD_SIZE) as u64
                    {
                        return Err(invalid_data("Journal offset outside record region"));
                    }
                    let relative = usize::try_from(relative)
                        .map_err(|_| invalid_data("Journal offset is not addressable"))?;
                    let index = relative / RECORD_SIZE / 2;
                    let copy = u8::from(!(relative / RECORD_SIZE).is_multiple_of(2));
                    let encoded: [u8; RECORD_SIZE] = bytes[start..start + RECORD_SIZE]
                        .try_into()
                        .expect("Fixed journal payload");
                    if word(&encoded, 0) != generation
                        || word(&encoded, 2) == 0
                        || !word(&encoded, 1).is_multiple_of(period)
                        || word(&encoded, 1) / period % count as u64 != index as u64
                        || word(&encoded, 7) != record_checksum(&encoded[..56], period, index)
                    {
                        return Err(invalid_data("Invalid journal after-image"));
                    }
                    Ok(Pending {
                        index,
                        copy,
                        sequence: generation,
                        encoded,
                    })
                };
                let minute = decode(HEADER_SIZE, MINUTE_COUNT, MINUTE_PERIOD, 4, 48)?;
                let day = decode(DAY_BASE, DAY_COUNT, DAY_PERIOD, 5, 112)?;
                if word(&minute.encoded, 1) / DAY_PERIOD * DAY_PERIOD != word(&day.encoded, 1) {
                    return Err(invalid_data("Journal pair belongs to different UTC days"));
                }
                Some([minute, day])
            }
            _ => return Err(invalid_data("Invalid journal checkpoint flags")),
        };
        Ok(Some(Self {
            generation,
            pending,
        }))
    }
}

impl Journal {
    fn recover(raw: &[u8]) -> io::Result<Option<Self>> {
        let mut latest: Option<Self> = None;
        for copy in 0..2 {
            let offset = journal_address(copy);
            let Some(record) =
                JournalRecord::decode(&raw[offset..offset + JOURNAL_RECORD_SIZE], copy)?
            else {
                continue;
            };
            if let Some(previous) = &latest {
                if record.generation == previous.record.generation && record != previous.record {
                    return Err(invalid_data(
                        "Conflicting journal copies with equal generations",
                    ));
                }
                if record.generation <= previous.record.generation {
                    continue;
                }
            }
            latest = Some(Self { record, copy });
        }
        Ok(latest)
    }
}

fn journal_offset(copy: u8) -> u64 {
    journal_address(copy) as u64
}

fn journal_address(copy: u8) -> usize {
    JOURNAL_BASE + usize::from(copy) * JOURNAL_STRIDE
}

fn journal_checksum(bytes: &[u8], copy: u8) -> u64 {
    checksum(bytes) ^ checksum(b"history-v2-redo") ^ checksum(&[copy])
}

fn invalid_data(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn recover_or_migrate(file: &mut File, raw: &mut [u8], length: u64) -> io::Result<Journal> {
    let mut actual = [0; HEADER_SIZE];
    actual.copy_from_slice(&raw[..HEADER_SIZE]);
    let expected = header();
    let legacy = header_version(1);
    let valid_checksum = word(&actual, 7) == checksum(&actual[..56]);
    if valid_checksum && actual != expected && actual != legacy {
        return Err(invalid_data("Unsupported history header/layout"));
    }
    let minutes = Ring::recover(raw, MINUTE_COUNT, MINUTE_PERIOD, HEADER_SIZE);
    let days = Ring::recover(raw, DAY_COUNT, DAY_PERIOD, DAY_BASE);
    let max_sequence = minutes
        .slots
        .iter()
        .chain(&days.slots)
        .map(|s| s.sequence)
        .max()
        .unwrap_or(0);
    let Some(journal) = Journal::recover(raw)? else {
        return initialize_legacy(file, raw, length, &actual, max_sequence);
    };
    if actual == legacy && journal.record.pending.is_some() {
        return Err(invalid_data(
            "v1 header with v2 transactions; refusing unsafe downgrade",
        ));
    }
    if max_sequence > journal.record.generation {
        return Err(invalid_data(
            "History is newer than every valid journal copy",
        ));
    }
    file.set_len(MAX_FILE_SIZE)?;
    replay_checkpoint(file, &journal.record)?;
    mirror_checkpoint(file, &journal.record, 1 - journal.copy)?;
    if let Some([minute, day]) = &journal.record.pending {
        for (base, item) in [(HEADER_SIZE, minute), (DAY_BASE, day)] {
            let offset = item.address(base);
            raw[offset..offset + RECORD_SIZE].copy_from_slice(&item.encoded);
        }
    }
    if actual != expected {
        file.write_at(0, &expected)?;
        file.durable_sync()?;
    }
    Ok(journal)
}

fn initialize_legacy(
    file: &mut File,
    raw: &[u8],
    length: u64,
    actual: &[u8; HEADER_SIZE],
    max_sequence: u64,
) -> io::Result<Journal> {
    // v2 without recovery data must not be silently reinterpreted as legacy.
    if *actual == header() || (actual[..8] == *MAGIC && word(actual, 1) == 2) {
        return Err(invalid_data("No valid v2 journal; history was not reset"));
    }
    if length > LEGACY_FILE_SIZE as u64
        && *actual != header_version(1)
        && !raw[..LEGACY_FILE_SIZE].iter().all(|&b| b == 0)
    {
        return Err(invalid_data(
            "Unrecognized history tail; history was not reset",
        ));
    }
    let journal = Journal {
        record: JournalRecord {
            generation: max_sequence.max(1),
            pending: None,
        },
        copy: 0,
    };
    if length > LEGACY_FILE_SIZE as u64 {
        if raw[LEGACY_FILE_SIZE..JOURNAL_BASE].iter().any(|&b| b != 0) {
            return Err(invalid_data("Unrecognized legacy alignment padding"));
        }
        for copy in 0..2 {
            let offset = journal_address(copy);
            let tail = &raw[offset..offset + JOURNAL_RECORD_SIZE];
            let planned = journal.record.encode(copy);
            let interrupted_initialization = (0..=JOURNAL_RECORD_SIZE)
                .any(|cut| tail[..cut] == planned[..cut] && tail[cut..].iter().all(|&b| b == 0));
            if !interrupted_initialization {
                return Err(invalid_data(
                    "Unrecognized legacy tail; history was not reset",
                ));
            }
            if raw[offset + JOURNAL_RECORD_SIZE..offset + JOURNAL_STRIDE]
                .iter()
                .any(|&b| b != 0)
            {
                return Err(invalid_data("Unrecognized legacy journal padding"));
            }
        }
    }
    migrate_v1(file, &journal.record)?;
    Ok(journal)
}

// One small I/O boundary allows deterministic torn-write/barrier fault injection.
trait CheckpointIo {
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()>;
    fn durable_sync(&mut self) -> io::Result<()>;
    fn resize(&mut self, length: u64) -> io::Result<()>;
}

impl CheckpointIo for File {
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        self.seek(SeekFrom::Start(offset))?;
        self.write_all(bytes)
    }

    fn durable_sync(&mut self) -> io::Result<()> {
        self.sync_all()?;
        #[cfg(target_os = "macos")]
        {
            use std::os::fd::AsRawFd;
            extern "C" {
                fn fcntl(fd: i32, command: i32, ...) -> i32;
            }
            // Apple fcntl.h: F_FULLFSYNC = 51 (flush device write cache too).
            if unsafe { fcntl(self.as_raw_fd(), 51) } == -1 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    fn resize(&mut self, length: u64) -> io::Result<()> {
        self.set_len(length)
    }
}

fn migrate_v1(io: &mut impl CheckpointIo, bootstrap: &JournalRecord) -> io::Result<()> {
    io.resize(MAX_FILE_SIZE)?;
    for copy in 0..2 {
        io.write_at(journal_offset(copy), &bootstrap.encode(copy))?;
    }
    io.durable_sync()?;
    io.write_at(0, &header())?;
    io.durable_sync()
}

fn replay_checkpoint(io: &mut impl CheckpointIo, record: &JournalRecord) -> io::Result<()> {
    if let Some([minute, day]) = &record.pending {
        for (base, item) in [(HEADER_SIZE, minute), (DAY_BASE, day)] {
            io.write_at(item.offset(base), &item.encoded)?;
        }
        io.durable_sync()?;
    }
    Ok(())
}

fn mirror_checkpoint(
    io: &mut impl CheckpointIo,
    record: &JournalRecord,
    copy: u8,
) -> io::Result<()> {
    io.write_at(journal_offset(copy), &record.encode(copy))?;
    io.durable_sync()
}

fn checkpoint(io: &mut impl CheckpointIo, record: &JournalRecord, copy: u8) -> io::Result<()> {
    // Commit point: one checksum-valid after-image pair crosses this first barrier.
    // The other journal still holds the old transaction until the rings are durable.
    mirror_checkpoint(io, record, copy)?;
    replay_checkpoint(io, record)?;
    mirror_checkpoint(io, record, 1 - copy)
}

impl Ring {
    fn recover(raw: &[u8], count: usize, period: u64, base: usize) -> Self {
        let mut ring = Self {
            slots: vec![
                Slot {
                    record: None,
                    sequence: 0,
                    copy: 1
                };
                count
            ],
            period,
            base,
            dirty: None,
        };
        for index in 0..count {
            for copy in 0..2 {
                let offset = ring.base + (index * 2 + usize::from(copy)) * RECORD_SIZE;
                let encoded = &raw[offset..offset + RECORD_SIZE];
                let sequence = word(encoded, 0);
                let timestamp_secs = word(encoded, 1);
                let duration_millis = word(encoded, 2);
                if sequence == 0
                    || duration_millis == 0
                    || !timestamp_secs.is_multiple_of(period)
                    || timestamp_secs / period % count as u64 != index as u64
                    || word(encoded, 7) != record_checksum(&encoded[..56], period, index)
                    || sequence <= ring.slots[index].sequence
                {
                    continue;
                }
                ring.slots[index] = Slot {
                    record: Some(HistoryRecord {
                        timestamp_secs,
                        duration_millis,
                        bytes: [
                            word(encoded, 3),
                            word(encoded, 4),
                            word(encoded, 5),
                            word(encoded, 6),
                        ],
                    }),
                    sequence,
                    copy,
                };
            }
        }
        ring
    }

    fn index(&self, timestamp: u64) -> usize {
        usize::try_from(timestamp / self.period % self.slots.len() as u64)
            .expect("Modulo bounds the index to the allocated ring")
    }

    fn latest(&self) -> Option<u64> {
        self.slots
            .iter()
            .filter_map(|slot| slot.record.as_ref().map(|r| r.timestamp_secs))
            .max()
    }

    fn aggregate(
        &self,
        timestamp: u64,
        elapsed: u64,
        bytes: [u64; 4],
    ) -> io::Result<HistoryRecord> {
        let timestamp_secs = timestamp / self.period * self.period;
        let existing = self.slots[self.index(timestamp)]
            .record
            .as_ref()
            .filter(|record| record.timestamp_secs == timestamp_secs);
        let mut result = existing.cloned().unwrap_or(HistoryRecord {
            timestamp_secs,
            duration_millis: 0,
            bytes: [0; 4],
        });
        let overflow = || io::Error::new(io::ErrorKind::InvalidInput, "History aggregate overflow");
        result.duration_millis = result
            .duration_millis
            .checked_add(elapsed)
            .ok_or_else(overflow)?;
        for (total, delta) in result.bytes.iter_mut().zip(bytes) {
            *total = total.checked_add(delta).ok_or_else(overflow)?;
        }
        Ok(result)
    }

    fn accept(&mut self, record: HistoryRecord) {
        let index = self.index(record.timestamp_secs);
        self.slots[index].record = Some(record);
        self.dirty = Some(index);
    }

    fn read(&self, limit: usize, high_water: Option<u64>) -> Vec<HistoryRecord> {
        let Some(latest) = high_water else {
            return Vec::new();
        };
        if limit == 0 {
            return Vec::new();
        }
        let latest_bucket = latest / self.period;
        let mut records: Vec<_> = self
            .slots
            .iter()
            .filter_map(|slot| slot.record.as_ref())
            .filter(|record| {
                let bucket = record.timestamp_secs / self.period;
                bucket <= latest_bucket && latest_bucket - bucket < self.slots.len() as u64
            })
            .cloned()
            .collect();
        records.sort_unstable_by_key(|record| std::cmp::Reverse(record.timestamp_secs));
        records.truncate(limit);
        records
    }

    fn pending(&self) -> io::Result<Option<Pending>> {
        let Some(index) = self.dirty else {
            return Ok(None);
        };
        let slot = &self.slots[index];
        let sequence = slot.sequence.checked_add(1).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "History sequence exhausted")
        })?;
        let record = slot.record.as_ref().expect("Dirty slot has an aggregate");
        let mut encoded = [0; RECORD_SIZE];
        put(&mut encoded, 0, sequence);
        put(&mut encoded, 1, record.timestamp_secs);
        put(&mut encoded, 2, record.duration_millis);
        for (index, value) in record.bytes.iter().enumerate() {
            put(&mut encoded, index + 3, *value);
        }
        let checksum = record_checksum(&encoded[..56], self.period, index);
        put(&mut encoded, 7, checksum);
        Ok(Some(Pending {
            index,
            sequence,
            copy: 1 - slot.copy,
            encoded,
        }))
    }

    fn committed(&mut self, pending: &Pending) {
        self.slots[pending.index].sequence = pending.sequence;
        self.slots[pending.index].copy = pending.copy;
        self.dirty = None;
    }
}

fn word(bytes: &[u8], index: usize) -> u64 {
    u64::from_le_bytes(
        bytes[index * 8..index * 8 + 8]
            .try_into()
            .expect("Fixed-size word"),
    )
}

fn put(bytes: &mut [u8], index: usize, value: u64) {
    bytes[index * 8..index * 8 + 8].copy_from_slice(&value.to_le_bytes());
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn record_checksum(bytes: &[u8], period: u64, index: usize) -> u64 {
    checksum(bytes) ^ checksum(&period.to_le_bytes()) ^ checksum(&(index as u64).to_le_bytes())
}

fn header() -> [u8; HEADER_SIZE] {
    header_version(2)
}

fn header_version(version: u64) -> [u8; HEADER_SIZE] {
    let mut bytes = [0; HEADER_SIZE];
    bytes[..8].copy_from_slice(MAGIC);
    for (index, value) in [
        version,
        RECORD_SIZE as u64,
        MINUTE_COUNT as u64,
        DAY_COUNT as u64,
        MINUTE_PERIOD,
        DAY_PERIOD,
    ]
    .into_iter()
    .enumerate()
    {
        put(&mut bytes, index + 1, value);
    }
    let hash = checksum(&bytes[..56]);
    put(&mut bytes, 7, hash);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, OpenOptions},
        io::{Read, Seek, SeekFrom, Write},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    const MINUTES: usize = 10_080;
    const DAYS: usize = 366;
    const HEADER: u64 = 64;
    const RECORD: u64 = 64;
    const SIZE: u64 = HEADER + (MINUTES + DAYS) as u64 * 2 * RECORD + 10_432;
    const FILE: &str = "history-v1.bin";
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "netdisk-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn file(&self) -> PathBuf {
            self.0.join(FILE)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn minutes(store: &HistoryStore) -> Vec<HistoryRecord> {
        store.read_minutes(usize::MAX).unwrap()
    }
    fn days(store: &HistoryStore) -> Vec<HistoryRecord> {
        store.read_days(usize::MAX).unwrap()
    }
    fn disk_image(store: &HistoryStore) -> Vec<u8> {
        // Windows locks forbid reads through a separately opened handle.
        let mut file = store.file.try_clone().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }
    fn position(day: bool, bucket: u64, copy: u64) -> u64 {
        let count = if day { DAYS } else { MINUTES } as u64;
        let base = if day { MINUTES as u64 * 2 * RECORD } else { 0 };
        HEADER + base + (bucket % count * 2 + copy) * RECORD
    }
    fn damage(path: &Path, offset: u64) {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        let mut byte = [0];
        file.read_exact(&mut byte).unwrap();
        byte[0] ^= 0x80;
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(&byte).unwrap();
        file.sync_all().unwrap();
    }

    #[test]
    fn aggregates_and_batches_until_minute_transition_or_flush() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        let empty = disk_image(&store);
        store.record(120, 900, [1, 2, 3, 4]).unwrap();
        store.record(179, 1_100, [5, 6, 7, 8]).unwrap();
        assert_eq!(disk_image(&store), empty);
        assert_eq!(
            minutes(&store),
            vec![HistoryRecord {
                timestamp_secs: 120,
                duration_millis: 2_000,
                bytes: [6, 8, 10, 12]
            }]
        );
        assert_eq!(days(&store)[0].bytes, [6, 8, 10, 12]);
        store.record(180, 1_000, [2; 4]).unwrap();
        assert_ne!(disk_image(&store), empty);
        store.flush().unwrap();
        let checkpoint = disk_image(&store);
        store.flush().unwrap();
        assert_eq!(disk_image(&store), checkpoint);
        drop(store);
        let reopened = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&reopened)[0].timestamp_secs, 180);
        assert_eq!(days(&reopened)[0].bytes, [8, 10, 12, 14]);
    }

    #[test]
    fn restart_continues_existing_minute_and_day() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(86_401, 1_000, [10; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(86_459, 2_000, [20; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [30; 4]);
        assert_eq!(minutes(&store)[0].duration_millis, 3_000);
        assert_eq!(days(&store)[0].timestamp_secs, 86_400);
        assert_eq!(days(&store)[0].bytes, [30; 4]);
    }

    #[test]
    fn accelerated_minute_wrap_and_exact_size_bound() {
        assert_eq!(HISTORY_FILE_BYTES, SIZE);
        assert_eq!(HISTORY_FILE_BYTES, 1_347_584);
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        for bucket in 0..MINUTES as u64 + 5 {
            store.record(bucket * 60, 1, [bucket; 4]).unwrap();
        }
        store.flush().unwrap();
        assert_eq!(fs::metadata(dir.file()).unwrap().len(), SIZE);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
        drop(store);
        let store = HistoryStore::open(&dir.0).unwrap();
        let records = minutes(&store);
        assert_eq!(records.len(), MINUTES);
        assert_eq!(records[0].timestamp_secs, (MINUTES as u64 + 4) * 60);
        assert_eq!(records.last().unwrap().timestamp_secs, 5 * 60);
        assert_eq!(store.read_minutes(2).unwrap(), records[..2]);
        assert_eq!(store.read_minutes(0).unwrap(), Vec::<HistoryRecord>::new());
    }

    #[test]
    fn accelerated_daily_wrap_and_gap_expiration() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        for day in 0..DAYS as u64 + 3 {
            store.record(day * 86_400, 1, [day; 4]).unwrap();
        }
        store.flush().unwrap();
        drop(store);
        let mut store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(days(&store).len(), DAYS);
        assert_eq!(days(&store).last().unwrap().timestamp_secs, 3 * 86_400);
        assert_eq!(store.read_days(1).unwrap().len(), 1);
        assert_eq!(store.read_days(0).unwrap(), Vec::<HistoryRecord>::new());
        store.record(800 * 86_400, 1, [1; 4]).unwrap();
        assert_eq!(minutes(&store).len(), 1);
        assert_eq!(days(&store).len(), 1);
        store.flush().unwrap();
        assert_eq!(fs::metadata(dir.file()).unwrap().len(), SIZE);
    }

    #[test]
    fn rollback_is_rejected_atomically_even_after_restart() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(120, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        let mut store = HistoryStore::open(&dir.0).unwrap();
        let before = disk_image(&store);
        assert_eq!(
            store.record(119, 1, [99; 4]).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(minutes(&store)[0].bytes, [1; 4]);
        assert_eq!(days(&store)[0].bytes, [1; 4]);
        store.flush().unwrap();
        assert_eq!(disk_image(&store), before);
        store.record(121, 1, [2; 4]).unwrap();
        store.record(120, 1, [3; 4]).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [6; 4]);
    }

    #[test]
    fn newest_corrupt_copy_falls_back_without_losing_other_buckets() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 10, [1; 4]).unwrap();
        store.flush().unwrap();
        store.record(0, 20, [2; 4]).unwrap();
        store.flush().unwrap();
        store.record(60, 30, [4; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        damage(&dir.file(), position(false, 0, 1) + 24);
        damage(&dir.file(), position(true, 0, 0) + 24);
        let mut store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store).len(), 2);
        assert_eq!(minutes(&store)[1].bytes, [1; 4]);
        assert_eq!(minutes(&store)[0].bytes, [4; 4]);
        // Latest journal restores the damaged latest day after-image (7 bytes).
        assert_eq!(days(&store)[0].bytes, [7; 4]);
        store.record(60, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [5; 4]);
    }

    #[test]
    fn legacy_torn_tail_preserves_previous_copy_and_other_regions() {
        let dir = Temp::new();
        let timestamp = (DAYS as u64 - 1) * 86_400;
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(timestamp, 10, [1; 4]).unwrap();
        store.flush().unwrap();
        store.record(timestamp, 20, [2; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        let mut legacy = fs::read(dir.file()).unwrap();
        legacy.truncate(LEGACY_FILE_SIZE);
        legacy[..HEADER_SIZE].copy_from_slice(&header_version(1));
        fs::write(dir.file(), legacy).unwrap();
        OpenOptions::new()
            .write(true)
            .open(dir.file())
            .unwrap()
            .set_len(LEGACY_FILE_SIZE as u64 - 17)
            .unwrap();
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(days(&store)[0].bytes, [1; 4]);
        assert_eq!(minutes(&store)[0].bytes, [3; 4]);
        assert_eq!(fs::metadata(dir.file()).unwrap().len(), SIZE);
    }

    #[test]
    fn interrupted_checkpoint_recovers_both_resolutions_exactly_once() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1_000, [10; 4]).unwrap();
        store.flush().unwrap();
        store.record(0, 1_000, [20; 4]).unwrap();
        // Restore the old day-copy bytes after preparing a complete checkpoint.
        // This is the disk image of a crash after minute but before day application.
        let pending = store.days.pending().unwrap().unwrap();
        let offset = pending.offset(store.days.base);
        let before = disk_image(&store);
        store.flush().unwrap();
        store.file.seek(SeekFrom::Start(offset)).unwrap();
        let offset = usize::try_from(offset).unwrap();
        store
            .file
            .write_all(&before[offset..offset + RECORD_SIZE])
            .unwrap();
        store.file.sync_data().unwrap();
        drop(store);
        let mut store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [30; 4]);
        assert_eq!(days(&store)[0].bytes, [30; 4]);
        store.record(60, 1_000, [5; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [5; 4]);
        assert_eq!(days(&store)[0].bytes, [35; 4]);
    }

    #[test]
    fn corrupt_header_is_repaired_without_discarding_slots() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(60, 1, [42; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        damage(&dir.file(), 0);
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [42; 4]);
        assert_eq!(days(&store)[0].bytes, [42; 4]);
    }

    #[test]
    fn corrupt_both_copies_only_removes_that_bucket() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        store.record(0, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        store.record(60, 1, [3; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        damage(&dir.file(), position(false, 0, 0) + 8);
        damage(&dir.file(), position(false, 0, 1) + 8);
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store).len(), 1);
        assert_eq!(minutes(&store)[0].bytes, [3; 4]);
        assert_eq!(days(&store)[0].bytes, [5; 4]);
    }

    #[test]
    fn unflushed_samples_are_not_promised_durable() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        store.record(0, 1, [9; 4]).unwrap();
        drop(store);
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [1; 4]);
    }

    #[test]
    fn zero_duration_and_overflow_do_not_partially_mutate_aggregates() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        assert!(store.record(0, 0, [1; 4]).is_err());
        assert_eq!(minutes(&store), Vec::<HistoryRecord>::new());
        store.record(0, u64::MAX, [u64::MAX; 4]).unwrap();
        assert!(store.record(0, 1, [0; 4]).is_err());
        assert!(store.record(60, 1, [1; 4]).is_err());
        assert_eq!(minutes(&store).len(), 1);
        assert_eq!(days(&store)[0].duration_millis, u64::MAX);
        store.flush().unwrap();
    }

    #[test]
    fn oversized_file_is_rejected_without_truncating_it() {
        let dir = Temp::new();
        let store = HistoryStore::open(&dir.0).unwrap();
        drop(store);
        OpenOptions::new()
            .write(true)
            .open(dir.file())
            .unwrap()
            .set_len(SIZE + 1)
            .unwrap();
        assert_eq!(
            HistoryStore::open(&dir.0).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::metadata(dir.file()).unwrap().len(), SIZE + 1);
    }

    #[test]
    fn only_one_writer_may_own_the_file() {
        let dir = Temp::new();
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(
            HistoryStore::open(&dir.0).err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(store);
        assert!(HistoryStore::open(&dir.0).is_ok());
    }

    #[test]
    fn parent_clamping_preserves_monotonic_duration_during_rollback() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        let last_wall = 125;
        store.record(last_wall, 1_000, [10; 4]).unwrap();
        let rolled_back_wall = 50;
        store
            .record(rolled_back_wall.max(last_wall), 1_500, [20; 4])
            .unwrap();
        assert_eq!(minutes(&store)[0].timestamp_secs, 120);
        assert_eq!(minutes(&store)[0].duration_millis, 2_500);
        assert_eq!(minutes(&store)[0].bytes, [30; 4]);
        store.flush().unwrap();
    }

    #[test]
    fn byte_overflow_rejects_both_resolutions_before_flushing() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1, [0, u64::MAX, 0, 0]).unwrap();
        let before = disk_image(&store);
        assert!(store.record(60, 1, [0, 1, 0, 0]).is_err());
        assert_eq!(minutes(&store).len(), 1);
        assert_eq!(days(&store)[0].duration_millis, 1);
        assert_eq!(disk_image(&store), before);
        store.record(86_400, 1, [1; 4]).unwrap();
        assert_eq!(days(&store).len(), 2);
    }

    #[test]
    fn failed_transition_does_not_accept_sample_and_flush_is_retryable() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1, [1; 4]).unwrap();
        let owner = store.file.try_clone().unwrap();
        store.file = File::open(dir.file()).unwrap();
        assert!(store.record(60, 1, [2; 4]).is_err());
        assert_eq!(minutes(&store).len(), 1);
        assert_eq!(days(&store)[0].bytes, [1; 4]);
        store.file = owner;
        store.flush().unwrap();
        store.record(60, 1, [2; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [2; 4]);
        assert_eq!(days(&store)[0].bytes, [3; 4]);
    }

    #[test]
    fn header_and_records_are_explicitly_little_endian() {
        fn serializable<T: serde::Serialize>(_: &T) {}
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        let value = 0x0102_0304_0506_0708;
        store.record(60, value, [value; 4]).unwrap();
        store.flush().unwrap();
        let image = disk_image(&store);
        assert_eq!(&image[..8], b"NDMHIST\0");
        assert_eq!(&image[8..16], &[2, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&image[16..24], &[64, 0, 0, 0, 0, 0, 0, 0]);
        let offset = usize::try_from(position(false, 1, 0)).unwrap();
        assert_eq!(&image[offset + 8..offset + 16], &[60, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&image[offset + 16..offset + 24], &[8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!(&image[offset + 24..offset + 32], &[8, 7, 6, 5, 4, 3, 2, 1]);
        serializable(&minutes(&store)[0]);
    }

    #[test]
    fn checksum_valid_unknown_version_is_rejected_without_changes() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        let mut image = disk_image(&store);
        drop(store);
        put(&mut image, 1, 3);
        let hash = checksum(&image[..56]);
        put(&mut image, 7, hash);
        fs::write(dir.file(), &image).unwrap();
        assert_eq!(
            HistoryStore::open(&dir.0).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(dir.file()).unwrap(), image);
    }

    #[test]
    fn every_record_byte_is_checksummed_and_previous_copy_survives() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        store.record(0, 2, [2; 4]).unwrap();
        store.flush().unwrap();
        let image = disk_image(&store);
        let offset = usize::try_from(position(false, 0, 1)).unwrap();
        for byte in 0..RECORD_SIZE {
            let mut damaged = image.clone();
            damaged[offset + byte] ^= 1;
            let ring = Ring::recover(&damaged, MINUTE_COUNT, MINUTE_PERIOD, HEADER_SIZE);
            let recovered = ring.read(1, Some(0));
            assert_eq!(recovered[0].bytes, [1; 4], "byte {byte}");
            assert_eq!(recovered[0].duration_millis, 1, "byte {byte}");
        }
    }

    #[test]
    fn sequence_exhaustion_does_not_overwrite_previous_copy() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        let before = disk_image(&store);
        store.minutes.slots[0].sequence = u64::MAX;
        store.record(0, 1, [1; 4]).unwrap();
        assert_eq!(
            store.flush().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(disk_image(&store), before);
        assert!(store.minutes.dirty.is_some());
    }

    #[test]
    fn maximum_timestamp_and_new_directory_are_supported() {
        let dir = Temp::new();
        let nested = dir.0.join("nested");
        let mut store = HistoryStore::open(&nested).unwrap();
        assert_eq!(minutes(&store), Vec::<HistoryRecord>::new());
        store.record(u64::MAX, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        let store = HistoryStore::open(&nested).unwrap();
        assert_eq!(minutes(&store)[0].timestamp_secs, u64::MAX / 60 * 60);
        assert_eq!(days(&store)[0].timestamp_secs, u64::MAX / 86_400 * 86_400);
    }

    #[derive(Clone, Debug)]
    enum Operation {
        Write(usize),
        Sync,
        Resize,
    }

    struct CrashIo {
        image: Vec<u8>,
        durable: Vec<u8>,
        operations: Vec<Operation>,
        fail: Option<(usize, usize)>,
    }

    impl CrashIo {
        fn new(image: Vec<u8>, fail: Option<(usize, usize)>) -> Self {
            Self {
                durable: image.clone(),
                image,
                operations: Vec::new(),
                fail,
            }
        }

        fn failure(&self) -> Option<usize> {
            self.fail
                .filter(|&(step, _)| step == self.operations.len() - 1)
                .map(|(_, cut)| cut)
        }
    }

    impl CheckpointIo for CrashIo {
        fn write_at(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()> {
            self.operations.push(Operation::Write(bytes.len()));
            let failure = self.failure();
            let written = failure.unwrap_or(bytes.len()).min(bytes.len());
            let start = usize::try_from(offset)
                .map_err(|_| invalid_data("Fault-test offset is not addressable"))?;
            self.image[start..start + written].copy_from_slice(&bytes[..written]);
            if failure.is_some() {
                return Err(io::Error::other("injected torn write"));
            }
            Ok(())
        }

        fn durable_sync(&mut self) -> io::Result<()> {
            self.operations.push(Operation::Sync);
            let failure = self.failure();
            if failure != Some(0) {
                self.durable.clone_from(&self.image);
            }
            if failure.is_some() {
                return Err(io::Error::other("injected barrier failure"));
            }
            Ok(())
        }

        fn resize(&mut self, length: u64) -> io::Result<()> {
            self.operations.push(Operation::Resize);
            let failure = self.failure();
            if failure != Some(0) {
                let length = usize::try_from(length)
                    .map_err(|_| invalid_data("Fault-test length is not addressable"))?;
                self.image.resize(length, 0);
            }
            if failure.is_some() {
                return Err(io::Error::other("injected resize failure"));
            }
            Ok(())
        }
    }

    fn fault_cuts(operation: &Operation) -> Vec<usize> {
        match operation {
            Operation::Write(size) => (0..=*size).collect(),
            Operation::Sync | Operation::Resize => vec![0, 1],
        }
    }

    fn checkpoint_fixture() -> (Vec<u8>, JournalRecord, u8) {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1_000, [10, 11, 12, 13]).unwrap();
        store.flush().unwrap();
        let image = disk_image(&store);
        store.record(1, 1_500, [20, 21, 22, 23]).unwrap();
        let generation = store.journal.record.generation + 1;
        let mut minute = store.minutes.pending().unwrap().unwrap();
        let mut day = store.days.pending().unwrap().unwrap();
        minute.resequence(generation, MINUTE_PERIOD);
        day.resequence(generation, DAY_PERIOD);
        (
            image,
            JournalRecord {
                generation,
                pending: Some([minute, day]),
            },
            1 - store.journal.copy,
        )
    }

    fn assert_replayed_pair(image: Vec<u8>, transaction: &JournalRecord, must_be_new: bool) {
        assert_eq!(image.len(), FILE_SIZE);
        let journal = Journal::recover(&image).unwrap().unwrap();
        let new = journal.record.generation == transaction.generation;
        assert!(!must_be_new || new, "durable journal transaction was lost");
        let expected_bytes = if new {
            [30, 32, 34, 36]
        } else {
            [10, 11, 12, 13]
        };
        let expected_duration = if new { 2_500 } else { 1_000 };
        let mut io = CrashIo::new(image, None);
        replay_checkpoint(&mut io, &journal.record).unwrap();
        mirror_checkpoint(&mut io, &journal.record, 1 - journal.copy).unwrap();
        let minutes =
            Ring::recover(&io.image, MINUTE_COUNT, MINUTE_PERIOD, HEADER_SIZE).read(1, Some(0));
        let days = Ring::recover(&io.image, DAY_COUNT, DAY_PERIOD, DAY_BASE).read(1, Some(0));
        for record in minutes.iter().chain(&days) {
            assert_eq!(record.bytes, expected_bytes);
            assert_eq!(record.duration_millis, expected_duration);
        }
        assert_eq!(minutes.len(), 1);
        assert_eq!(days.len(), 1);
        let once = io.image.clone();
        replay_checkpoint(&mut io, &journal.record).unwrap();
        mirror_checkpoint(&mut io, &journal.record, 1 - journal.copy).unwrap();
        assert_eq!(io.image, once, "recovery added the same contribution twice");
    }

    #[test]
    fn every_checkpoint_write_byte_and_sync_stage_recovers_one_complete_pair() {
        let (before, transaction, copy) = checkpoint_fixture();
        let mut complete = CrashIo::new(before.clone(), None);
        checkpoint(&mut complete, &transaction, copy).unwrap();
        assert_eq!(complete.operations.len(), 7);
        let mut cuts = 0;
        for (stage, operation) in complete.operations.iter().enumerate() {
            for cut in fault_cuts(operation) {
                let mut crash = CrashIo::new(before.clone(), Some((stage, cut)));
                assert!(checkpoint(&mut crash, &transaction, copy).is_err());
                // Unsynchronized writes may survive or be lost. After barrier #1,
                // the complete new transaction must survive either outcome.
                assert_replayed_pair(crash.image, &transaction, stage > 1);
                assert_replayed_pair(crash.durable, &transaction, stage > 1);
                cuts += 1;
            }
        }
        assert_eq!(cuts, 650);
        assert_replayed_pair(complete.image, &transaction, true);
    }

    #[test]
    fn crash_during_every_recovery_write_and_barrier_remains_replayable() {
        let (mut before, transaction, copy) = checkpoint_fixture();
        let offset = journal_address(copy);
        before[offset..offset + JOURNAL_RECORD_SIZE].copy_from_slice(&transaction.encode(copy));
        let mut complete = CrashIo::new(before.clone(), None);
        replay_checkpoint(&mut complete, &transaction).unwrap();
        mirror_checkpoint(&mut complete, &transaction, 1 - copy).unwrap();
        assert_eq!(complete.operations.len(), 5);
        for (stage, operation) in complete.operations.iter().enumerate() {
            for cut in fault_cuts(operation) {
                let mut crash = CrashIo::new(before.clone(), Some((stage, cut)));
                let result = replay_checkpoint(&mut crash, &transaction)
                    .and_then(|()| mirror_checkpoint(&mut crash, &transaction, 1 - copy));
                assert!(result.is_err());
                assert_replayed_pair(crash.image, &transaction, true);
                assert_replayed_pair(crash.durable, &transaction, true);
            }
        }
    }

    #[test]
    fn batched_checkpoint_has_four_bounded_writes_and_three_barriers() {
        let (before, transaction, copy) = checkpoint_fixture();
        let mut io = CrashIo::new(before, None);
        checkpoint(&mut io, &transaction, copy).unwrap();
        let sizes: Vec<_> = io
            .operations
            .iter()
            .filter_map(|op| match op {
                Operation::Write(size) => Some(*size),
                _ => None,
            })
            .collect();
        assert_eq!(sizes, [256, 64, 64, 256]);
        assert_eq!(sizes.iter().sum::<usize>(), 640);
        assert_eq!(
            io.operations
                .iter()
                .filter(|op| matches!(op, Operation::Sync))
                .count(),
            3
        );
        assert_eq!(io.image.len(), FILE_SIZE);
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        let before = disk_image(&store);
        for second in 0..60 {
            store.record(second, 1_000, [1; 4]).unwrap();
        }
        assert_eq!(
            disk_image(&store),
            before,
            "per-second samples must not write or sync"
        );
    }

    fn legacy_fixture() -> Vec<u8> {
        let (mut image, _, _) = checkpoint_fixture();
        image.truncate(LEGACY_FILE_SIZE);
        image[..HEADER_SIZE].copy_from_slice(&header_version(1));
        image
    }

    #[test]
    fn v1_migration_preserves_all_record_bytes_and_can_continue_the_bucket() {
        let dir = Temp::new();
        let legacy = legacy_fixture();
        fs::write(dir.file(), &legacy).unwrap();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(
            &disk_image(&store)[HEADER_SIZE..LEGACY_FILE_SIZE],
            &legacy[HEADER_SIZE..]
        );
        assert_eq!(minutes(&store)[0].bytes, [10, 11, 12, 13]);
        assert_eq!(days(&store)[0].bytes, [10, 11, 12, 13]);
        assert_eq!(fs::metadata(dir.file()).unwrap().len(), SIZE);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
        store.record(1, 1_500, [20, 21, 22, 23]).unwrap();
        store.flush().unwrap();
        drop(store);
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [30, 32, 34, 36]);
        assert_eq!(days(&store)[0].bytes, [30, 32, 34, 36]);
    }

    #[test]
    fn migration_crashes_at_all_stages_preserve_v1_history() {
        let legacy = legacy_fixture();
        let bootstrap = JournalRecord {
            generation: 2,
            pending: None,
        };
        let mut complete = CrashIo::new(legacy.clone(), None);
        migrate_v1(&mut complete, &bootstrap).unwrap();
        assert_eq!(complete.operations.len(), 6);
        for (stage, operation) in complete.operations.iter().enumerate() {
            // Include word boundaries, partial payload/checksum, and complete writes.
            let cuts = match operation {
                Operation::Write(size) => vec![0, 1, 8, 16, size - 1, *size],
                _ => vec![0, 1],
            };
            for cut in cuts {
                let mut crash = CrashIo::new(legacy.clone(), Some((stage, cut)));
                assert!(migrate_v1(&mut crash, &bootstrap).is_err());
                for image in [crash.image, crash.durable] {
                    let dir = Temp::new();
                    fs::write(dir.file(), image).unwrap();
                    let store = HistoryStore::open(&dir.0).unwrap();
                    assert_eq!(
                        &disk_image(&store)[HEADER_SIZE..LEGACY_FILE_SIZE],
                        &legacy[HEADER_SIZE..]
                    );
                    assert_eq!(minutes(&store)[0].bytes, [10, 11, 12, 13]);
                    assert_eq!(days(&store)[0].duration_millis, 1_000);
                    assert_eq!(fs::metadata(dir.file()).unwrap().len(), SIZE);
                }
            }
        }
    }

    #[test]
    fn every_journal_byte_is_checksummed_and_mirror_recovers_latest_pair() {
        let (before, transaction, copy) = checkpoint_fixture();
        let mut io = CrashIo::new(before, None);
        checkpoint(&mut io, &transaction, copy).unwrap();
        for copy in 0..2 {
            for byte in 0..JOURNAL_RECORD_SIZE {
                let mut damaged = io.image.clone();
                damaged[journal_address(copy) + byte] ^= 1;
                let recovered = Journal::recover(&damaged).unwrap().unwrap();
                assert_eq!(recovered.record, transaction, "copy {copy}, byte {byte}");
            }
        }
    }

    #[test]
    fn lost_v2_journal_is_rejected_without_modifying_history() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1, [1; 4]).unwrap();
        store.flush().unwrap();
        let mut image = disk_image(&store);
        drop(store);
        image[LEGACY_FILE_SIZE..].fill(0);
        for length in [FILE_SIZE, LEGACY_FILE_SIZE] {
            fs::write(dir.file(), &image[..length]).unwrap();
            assert_eq!(
                HistoryStore::open(&dir.0).err().unwrap().kind(),
                io::ErrorKind::InvalidData
            );
            assert_eq!(fs::read(dir.file()).unwrap(), image[..length]);
        }
    }

    #[test]
    fn stale_journal_cannot_overwrite_newer_history_after_corruption() {
        let (before, transaction, copy) = checkpoint_fixture();
        let mut io = CrashIo::new(before.clone(), None);
        checkpoint(&mut io, &transaction, copy).unwrap();
        io.image[LEGACY_FILE_SIZE..].copy_from_slice(&before[LEGACY_FILE_SIZE..]);
        let dir = Temp::new();
        fs::write(dir.file(), &io.image).unwrap();
        assert_eq!(
            HistoryStore::open(&dir.0).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(dir.file()).unwrap(), io.image);
    }

    #[test]
    fn checksum_valid_unknown_journal_version_and_invalid_offsets_fail_closed() {
        let (before, transaction, copy) = checkpoint_fixture();
        let mut io = CrashIo::new(before, None);
        checkpoint(&mut io, &transaction, copy).unwrap();
        for (field, value) in [(1, 3), (4, 0), (5, FILE_SIZE as u64), (3, 1)] {
            let mut image = io.image.clone();
            let offset = journal_address(copy);
            let entry = &mut image[offset..offset + JOURNAL_RECORD_SIZE];
            put(entry, field, value);
            let hash = journal_checksum(&entry[..248], copy);
            put(entry, 31, hash);
            let dir = Temp::new();
            fs::write(dir.file(), &image).unwrap();
            assert_eq!(
                HistoryStore::open(&dir.0).err().unwrap().kind(),
                io::ErrorKind::InvalidData
            );
            assert_eq!(fs::read(dir.file()).unwrap(), image);
        }
    }

    #[test]
    fn recovery_on_both_sides_of_midnight_does_not_duplicate_day_contributions() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(DAY_PERIOD - 1, 1_000, [10; 4]).unwrap();
        store.record(DAY_PERIOD, 1_000, [20; 4]).unwrap();
        store.flush().unwrap();
        for _ in 0..3 {
            drop(store);
            store = HistoryStore::open(&dir.0).unwrap();
            assert_eq!(
                days(&store).iter().map(|r| r.bytes[0]).collect::<Vec<_>>(),
                [20, 10]
            );
            assert_eq!(
                minutes(&store)
                    .iter()
                    .map(|r| r.bytes[0])
                    .collect::<Vec<_>>(),
                [20, 10]
            );
        }
    }

    #[test]
    fn journal_copies_are_block_isolated_and_a_whole_torn_block_is_recoverable() {
        let (before, transaction, copy) = checkpoint_fixture();
        let mut io = CrashIo::new(before, None);
        checkpoint(&mut io, &transaction, copy).unwrap();
        for copy in 0..2 {
            let offset = journal_address(copy);
            assert!(offset.is_multiple_of(JOURNAL_STRIDE));
            assert!(offset >= LEGACY_FILE_SIZE);
            let mut damaged = io.image.clone();
            damaged[offset..offset + JOURNAL_STRIDE].fill(0xff);
            assert_replayed_pair(damaged, &transaction, true);
        }
        assert_eq!(journal_offset(1) - journal_offset(0), 4096);
    }

    #[test]
    fn in_flight_retry_blocks_new_samples_and_never_recounts_the_checkpoint() {
        let dir = Temp::new();
        let mut store = HistoryStore::open(&dir.0).unwrap();
        store.record(0, 1_000, [10; 4]).unwrap();
        store.flush().unwrap();
        let before = disk_image(&store);
        store.record(1, 1_000, [20; 4]).unwrap();
        let generation = store.journal.record.generation + 1;
        let mut minute = store.minutes.pending().unwrap().unwrap();
        let mut day = store.days.pending().unwrap().unwrap();
        minute.resequence(generation, MINUTE_PERIOD);
        day.resequence(generation, DAY_PERIOD);
        let transaction = JournalRecord {
            generation,
            pending: Some([minute, day]),
        };
        let mut crash = CrashIo::new(before, Some((3, 32)));
        assert!(checkpoint(&mut crash, &transaction, 1 - store.journal.copy).is_err());
        store.file.write_at(0, &crash.image).unwrap();
        store.file.durable_sync().unwrap();
        store.in_flight = Some(transaction);
        let owner = store.file.try_clone().unwrap();
        store.file = File::open(dir.file()).unwrap();
        assert!(store.record(2, 1_000, [99; 4]).is_err());
        assert_eq!(minutes(&store)[0].bytes, [30; 4]);
        assert_eq!(days(&store)[0].bytes, [30; 4]);
        store.file = owner;
        store.flush().unwrap();
        store.flush().unwrap();
        store.record(2, 1_000, [5; 4]).unwrap();
        store.flush().unwrap();
        drop(store);
        let store = HistoryStore::open(&dir.0).unwrap();
        assert_eq!(minutes(&store)[0].bytes, [35; 4]);
        assert_eq!(days(&store)[0].bytes, [35; 4]);
        assert_eq!(minutes(&store)[0].duration_millis, 3_000);
        assert_eq!(days(&store)[0].duration_millis, 3_000);
    }
}
