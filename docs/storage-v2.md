# Storage v2 Checkpoint Contract

`src-tauri/src/storage.rs` exports `HISTORY_FILE_BYTES = 1_347_584` for
validation and disk-budget reporting. The single file remains named
`history-v1.bin`; its header explicitly identifies format version 2.

## Fixed Disk Budget

| Region | Offset | Bytes |
| --- | ---: | ---: |
| Original v1 header and two-copy minute/day rings | 0 | 1,337,152 |
| Alignment padding | 1,337,152 | 2,240 |
| Journal copy 0, isolated 4 KiB region | 1,339,392 | 4,096 |
| Journal copy 1, isolated 4 KiB region | 1,343,488 | 4,096 |
| Total | | **1,347,584** |

Each journal region contains a 256-byte checksummed payload. There is no
growing log, companion file, temporary migration file, or backup file.
The earlier 1,337,664-byte intermediate-format soak result is historical
evidence only; a new soak must assert the exported constant above.

## Batched Transaction

Incoming samples accumulate in memory. The previous batch checkpoints on a
minute transition or explicit `flush`; an empty flush performs no I/O.
There is no durable write per normal one-second sample.

Each nonempty checkpoint performs four bounded writes, 640 logical bytes,
and three durability barriers:

1. Write the alternate journal's complete minute/day after-images, then sync.
2. Write the minute and day ring records, then sync.
3. Mirror the committed journal to the other copy, then sync.

Recovery overwrites complete after-images rather than adding contributions.
It is idempotent, including crashes during recovery itself. A durable first
barrier commits both resolutions; before that barrier, the old or new pair
may survive, but a mixed pair must not. Failed checkpoints retain their exact
generation and images for retry before accepting additional samples.

## Compatibility And Limits

Opening a v1 history migrates it in place without rewriting its record region.
The new journal is durable before the v2 header is published. Interrupted
migration remains recoverable. Older binaries reject the explicit v2 header;
there is no downgrade or silent reset. Migration cannot reconstruct deltas
already lost through the legacy minute/day divergence bug.

Invalid or unsupported journals, a journal older than valid ring records,
and oversized files fail closed. Existing legacy record corruption handling
still applies. Checksums detect accidental corruption, not malicious edits.
The guarantee does not cover simultaneous destruction of both journals,
unrelated historical records, or devices that dishonor durability barriers.
Loss of the current unflushed batch on a crash is explicitly allowed.

Durability uses Rust [`File::sync_all`](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all).
On macOS it additionally requests `F_FULLFSYNC`, defined as 51 in Apple's
[open-source header](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/fcntl.h).
Apple's [fsync documentation](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html)
explains the distinction between an OS flush and a drive-cache flush.
Windows unit tests do not constitute macOS runtime or physical power-cut proof.

## Validation

The 34 storage tests include all 650 checkpoint write-cut/barrier cases,
391 recovery write-cut/barrier cases, interrupted migration, journal checksum
damage, isolated journal-block damage, stale and unsupported journal rejection,
repeated recovery, exactly-once retry, locking, retention wrap, and batched
write limits. Fault injection models both durable-only and surviving volatile
images; unexpected errors fail the tests rather than being skipped.

With the repository's existing dependencies, parent integration can run:

```powershell
cargo test --offline --manifest-path src-tauri/Cargo.toml --lib storage::tests
cargo clippy --offline --manifest-path src-tauri/Cargo.toml --lib --tests -- -W clippy::all -W clippy::pedantic -D warnings
```

Storage has also been compiled and checked independently against the existing
Serde artifact, including test code, with both Clippy lint groups and
`-D warnings`. Whole-crate checks and the new soak remain integration checks.
