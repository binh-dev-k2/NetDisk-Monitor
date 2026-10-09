# Verification 2026-10-10

This report distinguishes implemented code, current-machine evidence, and release
gates that still need external or long-running verification. No universal 100%
claim is made. The learning checklist in refactor-24x7.md remains unconfirmed
until the user explains the measurement and retention semantics.

## Current Parent Evidence

- Nine process tracker tests passed, including native creation identity, measured
  network deltas, actual elapsed rates, unavailable intervals and terminated rows.
- Two process-control tests passed. The live test rejected a stale creation token,
  left its own child alive, then terminated the checked native process object.
  The child ran with CREATE_NO_WINDOW, not an interactive terminal.
- Short backend pipeline probe passed: 30 samples / 29 elapsed seconds;
  system peak 10.343 ms; process peak 31.428 ms; test process peak RAM 15,982,592
  bytes; intermediate-format history size 1,337,664 bytes. This is not whole-app/WebView RAM or a
  24-hour result. The final format isolates journal copies in separate aligned
  4 KiB regions and is 1,347,584 bytes; a fresh final-format probe is required.
  The harness now waits the full configured duration.
- This token lacks SeSystemProfilePrivilege. The collector reported that fact;
  Final-format backend probe: 30.11 seconds, system peak 106.402 ms, process peak
  38.719 ms, test-process peak RAM 15,073,280 bytes, history 1,347,584 bytes.
  A release build overlapped the end of this short probe; these peaks describe
  that loaded machine, not an idle whole-application benchmark.
  no UAC was requested. Successful administrative packet capture is a separate
  gate, not implied by the permission-path test.
- Source inspection found no collector PowerShell, cmd.exe or WMIC invocation.
  External commands are explicit CSV reveal actions, plus a controlled test child.

## Native And Browser Evidence

See native-taskbar-acceptance.md for actual HWND parentage, 125% DPI bounds,
two-row rendering, eight enable/disable cycles and fail-closed recovery.
Final parent integration results:

- Desktop Rust suite: 120 passed, 0 failed, 6 explicitly ignored; 66.92 seconds.
- Offline desktop all-targets Clippy with all/pedantic and -D warnings: passed.
- Current-crate live taskbar tests: 2 passed, 10.66 seconds. Child rectangle
  [460,1020,644,1080], Start [654,1020,710,1080], DPI 120, two rows, 10 px gap.
  Eight lifecycle cycles destroyed the native child and preserved the supplied
  overlay sentinel; enable 0.341-16.803 ms in this run.
- Frontend: 58 tests passed across 12 files. TypeScript and production Vite build
  passed. Production-assets browser smoke passed at 1366/780/420 widths and
  336x164 overlay; native IPC was mocked, no dev server was started.
- Final read-only integration review found no additional confirmed high-impact
  issues after measured terminated-network tail and cross-monitor drag fixes.

## Release Gates

- [x] Final whole-crate Rust tests and strict Clippy.
- [x] Final frontend tests, TypeScript/Vite build and browser smoke.
- [ ] Final embedded-assets Windows release binary.
- [ ] Verification branch pushed without merging master.
- [ ] macOS Actions build/test/native probe result.
- [ ] Administrative ETW loopback capture, not merely permission refusal.
- [ ] Full native WebView overlay workflow across desktop configurations.
- [ ] Complete application soak for 24 hours.

The backend soak command defaults to 24 hours but does not test Tauri/WebView.
macOS PID network, secondary/vertical taskbars, and left-aligned Start remain
unsupported. Menu-bar display uses one native row on macOS.
