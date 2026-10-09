# Verification 2026-10-09

## Verified

- Rust desktop library: 66 passed, 0 failed, 1 explicit live probe excluded from routine tests.
- Live collector probe: passed separately; 30 samples / 6 process scans over 29 seconds.
- Probe maximum system collection: 6.98 ms; maximum process collection: 16.44 ms.
- Probe process records: 348; probe executable working set: 13,496,320 bytes.
  This is NOT the working set of the Tauri/WebView application.
- Frontend Vitest: 41 passed across 11 files.
- TypeScript build check and Vite production build: passed.
- Windows release with embedded custom-protocol frontend: passed, offline build.
- Release executable: src-tauri/target/release/netdisk-monitor.exe, 10,181,632 bytes.
- PE header verified: x64, Windows GUI subsystem (no console on ordinary launch).
- SHA256: C569D228AE3C696811C66FECB302AD75E82DF9549ADBA2AEDA108A6D498E1A8E.
- Source/docs UTF-8 validation and Git whitespace check: passed.
- Clippy all group with pedantic excluded and warnings denied: passed.
- Full pedantic with warnings denied: failed; not a clean pedantic release.
- Chromium built-asset smoke: 1366, 780, 420 px; no page overflow/runtime errors.
- Process horizontal scrolling at 420 px: passed.
- History table, CSV command payload, autostart toggle and display mode transitions:
  passed against a Tauri IPC fixture, NOT against the real Windows APIs.
- Overlay 336 x 164: compact layout, pointer gesture requests drag IPC.
- Screenshots under verification/ were visually checked for valid Vietnamese text,
  fitting controls and responsive layout.

## Regression Coverage

Counter baseline/reset, invalid elapsed time, device alias deduplication,
process disappearance/reappearance, PID reuse, terminated-cache bounds,
retention wrap, fixed file size, torn writes, corrupt copies/header,
exclusive storage ownership, unknown versions, failed-write retry cursor,
wall-clock rollback/restart watermark, shutdown gate, stream late cleanup,
fetch/event races, single in-flight poll, CSV formula injection and export bounds.

An independent review identified six persistence/runtime findings. They were
addressed in persistence.rs/runtime.rs/service.rs/lib.rs. Cursor and storage
failure scenarios have unit coverage; the full Tauri storage worker still needs
real filesystem-failure acceptance (permissions, disk full and unplugged media).

## Not Verified

- Native taskbar visual placement, Explorer restart, auto-hide, fullscreen and DPI
  changes on the actual Windows desktop.
- Native overlay drag/pin and real Registry autostart round trip.
- A 24-hour full-application soak including CPU/RAM/I/O and suspend/resume.
- macOS build and disk-device collector.
- Per-PID network byte collection (intentionally unavailable, not inferred).
- Atomic agreement between minute/day history across a power-loss checkpoint.
- Persistence of individual process history across app restarts.

## Acceptance Run

Use the release executable with no dev server. Record process CPU/RAM and history
file length at startup, 1 hour and 24 hours. Toggle overlay/taskbar repeatedly;
resize the dashboard; add/remove metric selections; open another app; lock,
suspend and resume Windows. Restart Explorer only during a deliberate manual
test. Confirm taskbar never covers reserved controls or becomes a top-level
floating strip. Test autostart and restore the previous setting afterward.
Force a storage write failure in a disposable data directory, then restore it;
confirm retry, warning and quit handling. These are release gates, not completed
test claims.
