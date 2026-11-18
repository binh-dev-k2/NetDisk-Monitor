# P2A System Core And Scoped Streams Evidence

Date: 2026-11-18, Asia/Saigon. Branch: `codex/24x7-native-validation-20261010`.
Release boundary remains `docs/DELIVERY.md`. This is one P2 vertical slice, not full core/release sign-off.

## Implemented

- `metrics_core::SystemReducer` owns system sequence, session reset generation, interval accounting
  and storage observation framing. It receives observations, monotonic time and wall timestamp;
  it does not read clocks, disk, Tauri state or UI. The existing collector `Collection` is an alias.
- Existing baseline/rate/integer-total behavior is retained. Reset rebaselines session counters but
  does not erase history counters; storage byte order stays network down/up then disk read/write.
  Backwards monotonic replay inputs cannot regress storage elapsed time or manufacture a rate.
- `get_system_snapshot` clones only system state; `get_process_snapshot` only process state.
  Legacy `get_snapshot` remains available. Combined JSON retains distinct diagnostic keys.
- Active-process count is computed once per process observation, cached as a nonserialized host field,
  and copied into the system summary with the process-network status. It is not recomputed from rows
  in JS or by scanning rows on every system tick. Unobserved/unavailable is null, measured empty is 0.
- Summary reads use `try_lock`: a busy process-row cache yields unknown for that frame instead of
  blocking the system worker behind a large IPC clone. Poison yields unknown with an error.
- Overview and overlay use system-only bootstrap. Process streaming is enabled only on its active
  tab. Leaving removes the listener/timer; reentry fetches fresh rows. The mounted memoized Process
  view preserves search, expansion, columns and pagination state while its data is frozen.
- Overview still receives the cached process-network diagnostic without subscribing to process rows;
  stale hidden process IPC errors are not surfaced there. Count renders N/A instead of an invented zero.
- CI now includes the no-desktop system-core test on both desktop matrix platforms.

No dependencies, storage format, collector cadence, process identity/time semantics, native taskbar
logic or product features were changed. Collectors and the backend process-emission cadence continue
while the tab is hidden: this is UI-side demand reduction, not fully demand-driven native transport.

## Red / Green And Checks

- Initial core/bridge contract runs failed because their new APIs did not yet exist. This demonstrates
  missing contract coverage, not a claimed preexisting arithmetic bug.
- The summary-contention regression then failed with a bounded 2-second timeout against a blocking
  read. It passed after `try_lock`, including a fresh count after contention clears.
- Frontend red: P1A assets failed the new narrow-bootstrap browser assertion; unit candidate had
  63 pass / 1 fail before implementation. Assertions were not weakened.
- Final desktop Rust: **162 passed, 0 failed, 8 explicitly ignored**, 33.28 seconds.
- Core without `desktop`: **6 passed** (these same six tests also appear in the desktop suite).
  Three existing dead-code warnings from the non-desktop network stub remain; no clean no-desktop
  Clippy claim. Normal dependency tree in this configuration contains no Tauri runtime dependency.
- Strict desktop all-target Clippy, rustfmt check and git whitespace check passed.
- Final frontend: **64 passed across 13 files**, two workers. Production build passed.
- Edge browser smoke passed 1366/780/420px and 336x164 overlay using existing installed tooling,
  no server/download/install. IPC is mocked; native kill/autostart/settings actions did not run.
- Browser covers no initial process bootstrap/listener in Overview/overlay; one fetch/listener on
  entry; no hidden fetch or resubscribe after 20 seconds of test clock; frozen table; reentry with
  fresh totals and retained controls; nullable count/diagnostics plus previous CSV/identity/layout cases.
- Screenshots: `%TEMP%/netdisk-monitor-p2a-browser`. Parent inspected mobile Overview after integration.
- Independent focused static review found no confirmed remaining actionable findings. Existing PyYAML
  parsed the final workflow and confirmed the core gate. GitHub CI has not run for this candidate.

## Windows Artifacts

```powershell
.\node_modules\.bin\tauri.cmd build --features desktop --bundles nsis --ci -- --offline --locked --jobs 2
```

Release compile passed in **3m52s**; NSIS produced one bundle, exit 0. Both are unsigned (`NotSigned`).
Executable PE verified as AMD64 / Windows GUI subsystem. Built at 03:29 on 2026-11-18 Asia/Saigon.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `src-tauri/target/p2a-2026-11-18/windows-x64/netdisk-monitor.exe` | 10,642,432 | `60BB98E5D9B8B30BEFAFE9560631D701ADDA4F80A6D6EFB3D225333189D4171B` |
| `src-tauri/target/p2a-2026-11-18/windows-x64/NetDisk-Monitor-P2A-setup.exe` | 2,339,077 | `6ABF9CC5CE9A93379A4B8E0159C1EA347D0B47B73182597584199D63471C8441` |

The executable keeps its canonical name so the read-only baseline sampler can identify it. Earlier
P0/P1A artifacts remain separate. No installed app was replaced, launched or installed by this slice.
Exit an existing Monitor through its tray menu before manually trying this candidate, otherwise the
single-instance policy may keep the earlier host active.

## Open Gates

P2 is incomplete: worker ownership/supervision, remaining core contracts, application ledger and
fully demand-driven transport still need separate verified slices. The approved four-view UI is
not integrated. This change is not a native taskbar fix or proof of lower CPU/RAM.

Native Windows lifecycle/presenter behavior, actual process-tree budgets, privileged ETW acceptance,
macOS execution, package install/update/uninstall and 24-hour app soak remain open. The
remaining P1 modernization batches are still in the delivery ledger, not silently dropped.

No automatic commit, push, merge, Explorer restart, signing or release publication. Understanding
checklist remains unverified until the user's answer; automated tests do not establish user mastery.
