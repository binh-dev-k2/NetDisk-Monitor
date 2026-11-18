# NetDisk Monitor Delivery Boundary

## Destination

A lightweight Windows/macOS monitor suitable for continuous use: truthful network and storage
measurements, bounded long-term storage, reliable lifecycle and native presenters, and the approved
compact four-view dashboard. Version upgrades alone do not satisfy this destination.

The user reaffirmed this boundary on 2026-11-17. This file is the release checklist, not a feature wishlist.

## Required Gates

- [ ] System and process measurements retain their documented units, sources, identity and coverage.
  Process time starts when this app first observes a process. Missing/unsupported data is not zero.
- [ ] Windows per-PID network measures real events when permission is available; no guessed allocation.
  macOS per-PID network capability remains unresolved, not silently claimed as implemented.
- [x] Host ownership, close-versus-Quit, `--background` autostart with read-back, and bounded shutdown semantics implemented; native installer acceptance open.
- [x] Windows taskbar tracks explicit status (`Disabled` / `Starting` / `Attached` / `Degraded` / `Unsupported`), wakes immediately on stop, follows dark/light system colors, and stabilizes metric slot widths; multi-build desktop visual acceptance remains open.
- [x] Overlay toggles independently of taskbar mode, lazily instantiates its WebView only when enabled, supports drag/pin/opacity, and clamps/restores its validated multi-monitor position.
- [x] Approved Overview / Applications / History / Settings (+ Runtime Diagnostics) 4-view UI integrated with real backend data, Light/Dark/System themes, demand-driven IPC emission/subscription, and single-render stream history updates.
- [ ] System history stays fixed at 1,347,584 bytes; logs stay bounded; queues and exited-process retention
  stay bounded. No new per-application persistent history is implied by this release.
- [ ] Target CPU/RAM/responsiveness budgets in the research plan are measured, not assumed.
- [ ] Windows and macOS builds pass CI; platform-specific native gates remain distinguishable from CI.
- [ ] Packaged install/update/uninstall and a single relevant 24-hour soak pass before release sign-off.

## Work Ledger

| Slice | State | Evidence / Next Gate |
| --- | --- | --- |
| Research/design | Approved | `research-redesign-2026-11-17.md` |
| P0 lifecycle/diagnostics | Code verified; Windows validation artifacts built | `p0-evidence-2026-11-17.md`; native acceptance open |
| Four-view wireframe | Layout approved | Offline prototype; production UI not integrated |
| P1A Tauri host upgrade | Local tests/lint/build verified; CI execution open | `p1a-evidence-2026-11-17.md` |
| Remaining P1 toolchain/collector/frontend batches | Not started | Separate compatible cohorts; no blanket upgrade |
| P2A system core/scoped streams | Local tests/lint/build verified | `p2a-evidence-2026-11-18.md`; rest of P2 remains open |
| P3 native presenters | Code + unit tests verified | Taskbar status machine, instant stop wakeup, theme colors, slot stabilization, lazy multi-monitor overlay; live desktop visual acceptance open |
| P4 production UI | Code + Vitest + Edge smoke verified | 4-view Overview/Applications/History/Settings+Diagnostics UI, Light/Dark/System themes, independent presenters |
| P6 release/soak | Not started | No release claim while required gates remain open |

## Stop Rules

Each slice has one deliverable, a bounded diff and a verification record. At most two hypotheses or
30 minutes without new evidence; after three failed patches, reconsider the design. One environment
fallback, no repeated reinstall/cache/worktree loops. Keep a 90-minute slice timebox and record incomplete
gates explicitly. Do not replace the installed app, restart Explorer, push/merge or publish as a hidden step.

Out of scope: framework replacement, cloud sync/accounts, telemetry service, process-history expansion,
kernel driver, root/system daemon, decorative redesign iterations and unsupported-platform claims.

## Understanding Checklist

- [ ] User explains close versus Quit and why browser tests do not prove taskbar pixels.
- [ ] User distinguishes adapter traffic, per-PID traffic, process I/O and physical disk I/O.
- [ ] User understands that an upgrade/build does not prove CPU/RAM or 24-hour reliability.
- [x] Architecture direction and four-view layout explicitly approved.

Do not mark understanding as demonstrated without the user's answer.
