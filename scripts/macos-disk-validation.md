# macOS Disk I/O Validation

## Interface and Integration

`src-tauri/src/macos_disk.rs` exposes:

```rust
pub fn sample() -> Result<Vec<(u64, u64, u64)>, String>
```

Each tuple is `(IORegistry entry ID, lifetime bytes read, lifetime bytes written)`.
These are driver counters, not volume counters, rates, or session totals. The module
owns no persistent state. Every successful snapshot replaces the active identity
set, so absent IDs are immediately retired. Parent integration must feed the tuples
to its reset-aware `CounterAccumulator<u64>`, baseline new/reappearing IDs, and
mark errors as unavailable rather than reporting them as zero activity. After an
outage, clear/rebaseline parent state to avoid attributing unobserved I/O to a tick.
Do not retain registry IDs across reboot. Parent owns collector/lib/Cargo changes.

## Windows Pure Tests

No Cargo/Tauri build or dependency installation is needed:

```powershell
rustc --edition 2021 --test src-tauri/src/macos_disk.rs -o "$env:TEMP/netdisk-macos-disk-tests.exe"
& "$env:TEMP/netdisk-macos-disk-tests.exe"
```

Tests cover driver-ID deduplication, distinct devices, disappearance/reappearance,
virtual exclusions, exact scan limit, over-limit/infinite inputs, error propagation,
and exact nonnegative integer conversion. They do not execute macOS framework FFI.

## Native macOS Validation

On an Intel Mac and an Apple Silicon Mac with existing Rust and Xcode Command Line
Tools, run from the repository:

```bash
bash scripts/validate-macos-disk.sh
```

For a temporary directory on the particular physical disk you want to exercise:

```bash
NETDISK_TEST_DISK_DIR=/Volumes/External/test-area bash scripts/validate-macos-disk.sh
```

The script installs nothing and records OS, architecture, Rust, and SDK versions.
It compiles C ABI assertions against the actual SDK, runs the pure Rust tests,
links the Rust framework declarations, and compares Rust identities/counters with
an independent C SDK-header reader immediately before and after the Rust sample.
Counters must lie within those temporal bounds and physical identity sets must
match. It runs 256 repeated snapshots, writes and syncs a 32 MiB scratch file, then
allows 15 seconds for physical write-counter movement. Scratch files and build
artifacts are cleaned up. Use an ordinary disk-backed directory with 32 MiB free.
No sudo, raw-device access, cache purge, mount changes, or destructive disk actions.

## Limits and Additional Hardware Checks

`.github/workflows/macos-disk-validation.yml` runs manually or on pushes to the
approved `codex/24x7-native-validation-*` branch pattern. Its independent build job
tests/builds the frontend, tests and lints the Rust parent, and uploads an ARM64
release executable. It uses the existing lockfiles and read-only repository access;
it does not sign, notarize, publish a release, or merge master.

The probe matrix distinguishes `abi-only` on `macos-latest` from `live` on
`macos-26-intel`. ABI-only compiles the SDK reference/probe and runs pure tests;
its output explicitly says that live counters were NOT validated. The script's
default remains `live`; no live-test failure is caught and converted to success.
This split follows actual runner evidence: the ARM virtual provider
`AppleVirtIODiskStorageDevice` lacks `Physical Interconnect`, so the strict
physical-counter API reports unavailable. Do not classify it as physical merely
because its location is Internal. Intel runner SDK comparison and write-counter
movement passed in run 37995165201. See the dated verification report for details.
Neither hosted VM proves Apple Silicon physical hardware or desktop GUI behavior.

- No macOS runtime or SDK compilation is claimed from Windows-only test results.
- Enumeration is capped at 1024 entries (one extra lookahead detects overflow),
  including duplicates/virtual entries. Native calls are synchronous and cannot
  be forcibly timed out; this is a work/memory bound, not a wall-clock guarantee.
- Only matching IOBlockStorageDriver instances with valid provider Protocol
  Characteristics are supported. Explicit Virtual Interface providers are excluded;
  missing/malformed required properties fail the entire snapshot. Drivers that do
  not expose this interface are not observable through this module.
- CFNumber conversion is exact signed 64-bit: negative, float, lossy, or counters
  beyond the signed range are rejected instead of fabricated or saturated.
- The write test proves aggregate counter movement, not exact byte attribution.
  Background I/O contributes. Cached file reads need not increase physical reads.
- On hardware, separately verify multiple APFS volumes share one driver ID, mount
  a disk image to confirm no additional physical record, and safely eject/reconnect
  a test external device to confirm disappearance and fresh parent baselines.
  Topology changes during a scan may return Err; retry on the next sampling tick.
- Leak/port-growth profiling and real desktop UI integration remain separate
  validation tasks; SDK/build evidence does not establish those properties.

## Official Sources

- [IOKit functions, ABI and ownership](https://github.com/apple-oss-distributions/IOKitUser/blob/main/IOKitLib.h)
- [Storage statistics properties](https://github.com/apple-oss-distributions/IOStorageFamily/blob/main/IOBlockStorageDriver.h)
- [Physical/virtual protocol properties](https://github.com/apple-oss-distributions/IOStorageFamily/blob/main/IOStorageProtocolCharacteristics.h)
- [Driver provider property access](https://github.com/apple-oss-distributions/IOStorageFamily/blob/main/IOBlockStorageDriver.cpp)
- [Core Foundation numeric ABI](https://github.com/apple-oss-distributions/CF/blob/main/CFNumber.h)
- [IOKit handle types](https://github.com/apple-oss-distributions/xnu/blob/main/iokit/IOKit/IOTypes.h)
