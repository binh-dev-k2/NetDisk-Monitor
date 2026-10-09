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
  4 KiB regions and is 1,347,584 bytes; the final-format probe is recorded below.
  The harness now waits the full configured duration.
- Final-format backend probe: 30.11 seconds, system peak 106.402 ms, process peak
  38.719 ms, test-process peak RAM 15,073,280 bytes, history 1,347,584 bytes.
  A release build overlapped the end of this short probe; these peaks describe
  that loaded machine, not an idle whole-application benchmark.
- This token lacks SeSystemProfilePrivilege. The collector reported that fact;
  no UAC was requested. Successful administrative packet capture is a separate
  gate, not implied by the permission-path test.
- Source inspection found no collector PowerShell, cmd.exe or WMIC invocation.
  External commands are explicit CSV reveal actions, plus a controlled test child.

## Native And Browser Evidence

See native-taskbar-acceptance.md for actual HWND parentage, 125% DPI bounds,
two-row rendering, eight enable/disable cycles and fail-closed recovery.
Final parent integration results:

- Final desktop Rust suite: 123 passed, 0 failed, 6 explicitly ignored; 41.52 seconds.
- Offline desktop all-targets Clippy with all/pedantic and -D warnings: passed.
- Final current-crate live taskbar tests: 2 passed, 11.19 seconds. Child rectangle
  [460,1020,644,1080], Start [654,1020,710,1080], DPI 120, two rows, 10 px gap.
  Eight lifecycle cycles destroyed the native child and preserved the supplied
  overlay sentinel; enable 0.324-1.498 ms in this run.
- Frontend: 58 tests passed across 12 files. TypeScript and production Vite build
  passed. Production-assets browser smoke passed at 1366/780/420 widths and
  336x164 overlay; native IPC was mocked, no dev server was started.
- Final read-only integration review found no additional confirmed high-impact
  issues after measured terminated-network tail and cross-monitor drag fixes.

## Intermittent Native Proof And Recovery

A later normal-desktop rerun failed both live tests when UIA could not verify
Start. The user confirmed that the desktop was not locked or fullscreen. A
subsequent stable-desktop run passed again; missing/offscreen/shifted cached Start
records were not captured in the first failure, so its underlying cause is not
claimed resolved. Bounded test-only diagnostics now include those records.

The old policy held failed proofs or geometry mismatches for up to five seconds.
The new policy invalidates old proof, retries at 250/500/1000/2000/5000 ms, and
retains a five-second healthy refresh. Repeated renderer ticks cannot postpone
the retry deadline or exceed the query floor. Full bar origin and Start bounds
are part of cache identity. Three pure regressions failed against the former
fixed-delay behavior and passed after this change. Final current-crate live
tests passed as recorded above; no actual live reflow was induced. This improves
bounded recovery, not a guarantee that every Explorer UIA provider is available.

## Release Gates

- [x] Final whole-crate Rust tests and strict Clippy.
- [x] Final frontend tests, TypeScript/Vite build and browser smoke.
- [x] Final embedded-assets Windows release binary.
- [x] Verification branch pushed without merging master.
- [x] macOS Actions build/test/lint, ARM ABI-only and Intel live-counter results.
- [ ] Administrative ETW loopback capture, not merely permission refusal.
- [ ] Full native WebView overlay workflow across desktop configurations.
- [ ] Complete application soak for 24 hours.

The backend soak command defaults to 24 hours but does not test Tauri/WebView.
macOS PID network, secondary/vertical taskbars, and left-aligned Start remain
unsupported. Menu-bar display uses one native row on macOS.

## Build Identity

Source commit: `1e2578002ae605082b1336182c4d14d8739e2ae3` on
`codex/24x7-native-validation-20261010`, pushed to origin; master was not merged.
Windows release build completed in 2m 17s with embedded production assets.
`src-tauri/target/release/netdisk-monitor.exe`: 10,272,768 bytes, x64 PE machine
0x8664, GUI subsystem 2. SHA-256:
`7F02D82D8592C55496D81D68720BD769473D225730C97B367C114C332813E489`.

macOS validation run:
https://github.com/binh-dev-k2/NetDisk-Monitor/actions/runs/37997623287
Completed successfully at the source commit above. Frontend: 58 tests / 12 files.
Rust: 105 passed / 3 ignored in 38.65s; strict all-targets Clippy passed. ARM64
desktop release build passed in 3m 09s. Artifact ID 11648232083, ZIP size 3,218,088
bytes; SHA-256 (ZIP, not the executable):
`11c2ab63b4c17c37d33747623839966b2ad2dc92a99f0fa373ac44106890d1d5`.
Download:
https://github.com/binh-dev-k2/NetDisk-Monitor/actions/runs/37997623287/artifacts/11648232083

Earlier complete green run 37996661420 at source 59ff113 passed 58 frontend tests,
102 Rust tests (3 ignored), strict macOS Clippy, release build, ARM SDK ABI-only
checks and Intel live IOKit checks. Its artifact ID is 11647930396. It is retained
as earlier evidence, not substituted for the final-source run.

## macOS Counter Scope

The final Intel runner (macOS 26.6.1, x86_64) passed nine pure tests, and its
independent SDK reader and Rust counters matched temporal bounds; 256 native
snapshots took 29.027 ms across one classified driver. Write-counter movement
passed after the scratch workload. This is not exact workload byte attribution.
The ARM runner's
AppleVirtIODiskStorageDevice provider lacks Physical Interconnect even though
Physical Interconnect Location is Internal. Both readers rejected that metadata.
The application reports unavailable, not fabricated physical traffic. ARM CI's
abi-only mode explicitly does not run/prove live disk counters; live remains the
script default. Physical Apple Silicon hardware and desktop GUI remain gates.
