//! Standalone macOS native probe, independent of Tauri/Cargo integration.
#[path = "../src-tauri/src/macos_disk.rs"]
mod macos_disk;

use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Counters = BTreeMap<u64, (u64, u64)>;

fn reference(executable: &Path) -> Result<Counters, String> {
    let result = Command::new(executable)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "SDK reference reader failed ({}): {}; Rust sample: {:?}",
            result.status,
            String::from_utf8_lossy(&result.stderr),
            macos_disk::sample()
        ));
    }
    let text = String::from_utf8(result.stdout).map_err(|e| e.to_string())?;
    let mut rows = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<u64> = line
            .split(',')
            .map(str::parse::<u64>)
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        if fields.len() != 3 || rows.insert(fields[0], (fields[1], fields[2])).is_some() {
            return Err(format!("invalid/duplicate reference row: {line}"));
        }
    }
    Ok(rows)
}

fn rust_snapshot() -> Result<Counters, String> {
    let mut rows = BTreeMap::new();
    for (id, read, write) in macos_disk::sample()? {
        if rows.insert(id, (read, write)).is_some() {
            return Err(format!("duplicate Rust driver ID: {id}"));
        }
    }
    Ok(rows)
}

fn compare(executable: &Path) -> Result<Counters, String> {
    let before = reference(executable)?;
    let rust = rust_snapshot()?;
    let after = reference(executable)?;
    if rust.is_empty() {
        return Err(
            "no physical driver counters: this host does not prove native collection".into(),
        );
    }
    if before.keys().ne(rust.keys()) || after.keys().ne(rust.keys()) {
        return Err("driver topology changed or Rust/reference identity sets differ".into());
    }
    for (&id, &(read, write)) in &rust {
        let (lo_read, lo_write) = before[&id];
        let (hi_read, hi_write) = after[&id];
        if !(lo_read <= read && read <= hi_read && lo_write <= write && write <= hi_write) {
            return Err(format!(
                "driver {id}: Rust counters outside SDK reference temporal bounds"
            ));
        }
    }
    Ok(rust)
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn exercise_write(directory: &Path) -> Result<Scratch, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = directory.join(format!(
        "netdisk-io-probe-{}-{nonce}.bin",
        std::process::id()
    ));
    let mut file: File = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let scratch = Scratch(path);
    // Nonzero pseudorandom data avoids a sparse/zero-only workload; no raw-disk writes.
    let mut data = vec![0_u8; 1024 * 1024];
    let mut state = 0x4d59_5df4_d0f3_3173_u64;
    for byte in &mut data {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }
    for _ in 0..32 {
        file.write_all(&data).map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(scratch)
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err("usage: probe SDK_REFERENCE_EXECUTABLE SCRATCH_DIRECTORY".into());
    }
    let executable = Path::new(&args[1]);
    let baseline = compare(executable)?;
    let start = Instant::now();
    for _ in 0..256 {
        let rows = rust_snapshot()?;
        if rows.keys().ne(baseline.keys()) {
            return Err("topology changed during repeated sampling".into());
        }
    }
    println!(
        "256 native snapshots: {:?}; physical drivers: {}",
        start.elapsed(),
        baseline.len()
    );
    let baseline = compare(executable)?;
    let _scratch = exercise_write(Path::new(&args[2]))?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let now = compare(executable)?;
        if now
            .iter()
            .any(|(id, &(_, write))| baseline.get(id).is_some_and(|&(_, old)| write > old))
        {
            println!("SDK comparison and physical write-counter movement passed: {now:?}");
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("no physical write-counter movement within 15 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("macOS disk validation FAILED: {error}");
        std::process::exit(1);
    }
}
