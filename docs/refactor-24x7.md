# 24x7 Refactor

Approved: Rust/Tauri; independent workers and native taskbar; no inferred PID
network data. Fixed binary retention: 7 days of minute buckets, 366 daily buckets.

## Delivery Checklist

- [x] Find checkout; clean Git baseline; user approved architecture.
- [x] Correct counter deltas, timing and PID reuse; regression tests.
- [x] Separate system/process/storage workers with cooperative shutdown.
- [x] Fixed-size checksummed binary rings, cached API, history table and CSV.
- [x] Independent native taskbar renderer, no WebView parenting.
- [x] Authoritative UI totals, stream cleanup, overlay drag IPC, responsive layout.
- [x] Automated checks and independent review; six review findings addressed.
- [x] Current-desktop native taskbar parent/bounds/two rows/eight lifecycle cycles.
- [x] macOS native disk identity implementation and manual/branch CI workflow.
- [x] Windows real per-PID ETW implementation with rights/loss reporting.
- [x] Windows embedded-assets release and final macOS Actions build/test/lint.
- [x] Intel hosted-runner live disk SDK comparison; ARM SDK ABI-only checks.
- [ ] Full native overlay acceptance and 24-hour complete application soak.
- [ ] Real macOS desktop menu-bar/overlay and physical Apple Silicon disk acceptance.
- [ ] macOS per-PID network collector (unsupported, shown as unavailable).

## Design Decisions

System samples every second; process scans every five seconds. Collector owners
never share mutable sysinfo instances. Persistence has a bounded handoff, batches
once per minute and uses fixed slots. No growing log, index or vacuum. Process
history is bounded separately; unsupported measurements remain unavailable.
Byte rates use actual monotonic elapsed time. Process identity includes creation
time. Global disk bytes come from device counters, not sums of process counters.

The latest cumulative sample is a bounded handoff, not a growing queue. The
persistence cursor advances only after a record is accepted. A failed write
keeps the accumulated byte delta for retry; startup retains the producer's first
baseline. Clock rollback clamps bucket timestamps against recovered history,
not against a duplicate/latest poll. Cache freshness uses monotonic time.

Quit stops data producers before the persistence worker takes the final sample.
Joining and flushing happen outside the UI thread. Repeated quit cannot bypass
an in-flight flush. Failed shutdown is shown on the dashboard; measurement has
stopped and a second quit acknowledges possible unsaved data.

Native taskbar work stays on one dedicated Win32/COM worker. Stop never waits on
a stalled renderer. Bounds must be verified; unknown geometry hides the renderer
and reports an error. This is not universal taskbar support: left-aligned Start,
vertical taskbars, insufficient space and secondary taskbars remain unsupported.
Font measurement controls size; at most two text rows. Overlay opacity does not
apply to native GDI text. DPI comes from the taskbar HWND, not desktop LOGPIXELSX.
Unknown or changed native geometry invalidates cached proof. Recovery attempts
use bounded 250ms-to-5s backoff; healthy UIA refresh remains five seconds. A normal
desktop missing-Start condition was observed intermittently, not fully explained.

## Retention And Accuracy

The history file is exactly 1,347,584 bytes: 10,080 minute buckets and 366 daily
buckets, each with two checksummed copies, plus a 10,432-byte aligned tail for
two isolated journal regions. The journal payloads total 512 bytes. Old detail expires by design. UI lists
120 recent minute buckets or 366 days; CSV includes the full retained resolution.
Process data is session-only: current processes plus at most 512 terminated
identities. The current default does not persist per-process history.

Each sample delta belongs wholly to its ending UTC bucket. Delayed/retried data
is not interpolated across missed buckets. Duration reflects measured monotonic
time; gaps are not fabricated. Global network sums adapter counters and can
include virtual/VPN interfaces, not just internet traffic.

Ordinary writes checkpoint once per minute. A crash can lose the current
unflushed batch. Minute/day checkpoints use a two-copy fixed-size redo journal.
Recovery overwrites after-images, never adds them again; this assumes storage
honors durable flush barriers. v1 migration preserves valid records, but cannot
retroactively repair unknown inconsistencies left by older software. Checksums
detect accidental corruption, not malicious edits. These are explicit limits,
not a promise of lossless accounting.

## Understanding Checklist

- [ ] Visible HWND flags and valid bounds do not prove on-screen visibility; sibling z-order matters.
- [ ] The old taskbar reused a popup WebView; the native replacement creates a new child and must establish its z-order.
- [ ] HWND_TOP orders taskbar siblings only; it is not a global HWND_TOPMOST overlay.
- [ ] Native Start HWND is authoritative; a duplicate UIA Start element may disappear during taskbar reflow.
- [ ] Connection count cannot attribute bytes to a process.
- [ ] Process I/O is not physical disk traffic.
- [ ] PID reuse requires creation-time identity.
- [ ] Bounded storage necessarily discards old detail.
- [ ] Worker isolation prevents slow scans blocking meters.
- [ ] Tests are evidence, not a guarantee of 100% functionality.

## Release Gates

Test counter resets, invalid timing, process churn, PID reuse, retention wraps,
restart and torn writes. Verify frontend/Rust builds. Native taskbar must be tested
with DPI changes, Explorer restart, fullscreen, auto-hide and multiple monitors.
macOS needs a real macOS build. Per-process network needs a native event source
with permission and loss reporting. 24h stability requires a recorded soak run.

See [verification-2026-10-10.md](verification-2026-10-10.md) and
[native-taskbar-acceptance.md](native-taskbar-acceptance.md) for actual evidence.
