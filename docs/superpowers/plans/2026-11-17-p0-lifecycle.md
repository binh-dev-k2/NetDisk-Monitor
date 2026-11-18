# P0 Lifecycle And Wireframe Implementation Plan

> **For agentic workers:** Use executing-plans task by task. Check off verified steps, not intentions.

**Goal:** Make implicit window exit unable to stop monitoring, prevent duplicate host instances, expose bounded diagnostics/worker health, and deliver the approved UI wireframe.

**Architecture:** Retain the current collectors and storage. A one-shot quit intent wraps ShutdownGate. A bounded nonblocking log sink and fixed worker registry provide evidence without tick-by-tick logs. A separate offline prototype covers four UI views; production UI remains unchanged until wireframe review.

**Tech Stack:** Existing Rust/Tauri/Serde and React toolchain; pinned single-instance plugin only. No general dependency upgrades.

**Spec:** `docs/research-redesign-2026-11-17.md`, approved by user message "Làm".

## Global Constraints

- UTF-8 Vietnamese must remain valid; no unrelated cleanup or changes to history format.
- No dev server, Explorer restart, duplicate running app, or replacement of installed binary in P0.
- At most two hypotheses/30 minutes without new evidence; one dependency fallback only.
- Close dashboard is not Quit. OS forced termination can still lose the documented unflushed batch.
- No statement that taskbar/native pixels, spontaneous exit or 24-hour stability are solved by unit tests.

## Task 1: Explicit Quit Intent

Files: `src-tauri/src/runtime.rs`, `src-tauri/src/lib.rs`.
Interface: `ShutdownGate::authorize_quit()`, `authorize_finalization()`,
`request() -> ShutdownRequest`, `complete()`, `finish()`.
Mutex-serialized phases bind intent to flush/drain epochs. A distinct discard action
acknowledges failed flush; ordinary or delayed Quit cannot acknowledge data loss.

- [x] Add and run a red test using the current API:
  `assert_ne!(ShutdownGate::default().request(), ShutdownRequest::Start)`.
- [x] Implement phased authorization in owned menu Quit and successful flush/finalize callbacks.
- [x] Test repeated Quit cannot bypass flush, implicit exit after failure cannot acknowledge data loss,
  and successful completion requires a fresh authorization before Exit.
- [x] Run the final desktop library suite: 151 passed, 0 failed, 8 explicitly ignored.

## Task 2: Bounded Diagnostics And Worker Health

Files: new `src-tauri/src/diagnostics.rs`, new `src-tauri/src/health.rs`;
integrate in `lib.rs`, `service.rs`, `persistence.rs`.
Interfaces: `LogOwner::start(PathBuf)`, `LogSink::record(&'static str, &str)`,
`LogOwner::shutdown(Duration)`, `WorkerHealth::start(WorkerKind) -> WorkerGuard`,
`WorkerGuard::beat()`, `WorkerHealth::snapshot()`.

- [x] Cover UTF-8 truncation, rotation/disk bound, saturated queue drops and worker heartbeat/panic lifecycle.
  Red/green evidence was observed for exit-policy and late-admission races; initial red output for every
  other new case was not retained and is not claimed.
- [x] Implement a 128-event queue, 512-byte messages, two 256-KiB event files and one bounded fatal file.
  Diagnostics I/O failures must be readable, not crash or stop collectors.
- [x] Add one worker guard per collector owner. Heartbeat is not a claim of valid measurement;
  storage warnings are separate. Expose `get_runtime_health` for actual dashboard integration later.
- [x] Hook panic evidence with bounded last-fatal text; preserve the existing panic hook.
- [x] Final desktop library suite, strict all-target Clippy and rustfmt check passed.

## Task 3: Single Instance And Baseline Tool

Files: `Cargo.toml`, `Cargo.lock`, `lib.rs`, `scripts/windows-runtime-baseline.ps1`.

- [x] Pin `tauri-plugin-single-instance = 2.4.0`, desktop-only, registered before opener.
  Version 2.5.5 requires Tauri ^2.12; 2.4.0 requires ^2.10 and preserves the 2.11.5 baseline.
  Move to current plugin with the coordinated P1 Tauri upgrade.
  A second foreground launch focuses the existing dashboard; background launch does not.
  Independent per-data-directory file lease closes the plugin's concurrent-startup race before workers/logging.
  Initial windows are hidden until ownership; actual multi-process focus still needs native acceptance.
- [x] Resolve the new dependency once. If unavailable offline, permit one online resolution, no reinstall loop.
- [x] Add a read-only bounded sampler requiring an explicit Monitor PID, collecting whole process-tree CPU/RAM.
  No launch, stop, registry edits, terminal spawn or persistent telemetry.
- [x] Check PowerShell syntax and safe missing/invalid PID behavior; do not fabricate a baseline without a running app.

## Task 4: Wireframe And Review

Files: offline prototype under the permitted visualization output directory; no production UI changes.

- [x] Four working views, compact tables/charts, independent display toggles, theme, search/sort/export.
- [x] Desktop/mobile Playwright: 108 assertions, 18 screenshots, zero page JS errors/network requests.
- [x] Independent source review; fixed quit-intent, duplicate-owner and log-drain races.
- [x] User approved the four-view layout on 2026-11-17. Production integration remains a later slice.
- [x] Update this checklist and research status; evidence is in `docs/p0-evidence-2026-11-17.md`.

## P0 Outcome

Implementation and automated verification are complete for this slice. Native close/quit/focus
acceptance and an actual whole-process-tree baseline remain open; no running Monitor was available
for a non-invasive measurement. Do not promote these outstanding gates to passed.

## Deferred Gates

P1 upgrades, core extraction, taskbar helper, native presenter acceptance, packaged installer,
macOS hardware validation and 24h soak remain separate. No automatic commit/push or installed-app replacement.
