# Frontend UX Contract and Acceptance

## Measurement Semantics

- System samples arrive via `metrics://snapshot`; `sequence` rejects stale or duplicate samples.
- Process samples arrive as `{processSequence, processes}` via `processes://snapshot` on the separate five-second cadence. The bootstrap snapshot also contains `processSequence`.
- Initial `get_snapshot` contains both system metrics and process records.
- Process identity includes creation time, not PID alone. Windows uses exact native FILETIME ticks in the identity string; startTime remains an OS-seconds display field. Close actions include the exact identity, and terminated records have no close action.
- Process rates and session totals are measured I/O, not physical disk traffic. Unsupported per-PID network metrics display `N/A`; the frontend does not integrate rates into fabricated cumulative totals.
- Process duration starts at first observation. A name group displays the maximum duration among its currently retained children, never a sum. Session I/O is the sum of those retained children's counters.
- Active process records are distinct from a FIFO of at most 512 terminated records. Once full, the longest-retained terminated record is evicted. Evicted records no longer contribute to table/CSV groups, so group counts and I/O totals can decrease. This is not a complete lifetime process ledger.
- Process records are memory-only for the current app run. Reset clears retained terminated records and rebaselines active process I/O; duration still means time since first observation. App restart starts new process observations.
- Persisted minute/day system history is separate from process records. The overview includes a retained-history table and CSV; no persisted per-process history is claimed.

## Lifecycle and Performance

- Subscribe before bootstrap fetch; an event received while fetching prevents the older response from overwriting it.
- Late listener registration is cleaned up after disposal. Fetch callbacks cannot update disposed subscriptions.
- Stale polling has only one request in flight. Duplicate system sequences do not append graph samples.
- A new process sequence refreshes freshness without replacing an unchanged process array. Equality compares fields, not serialized JSON. Sequence advancement is independent of value equality, so polling the same cached sequence cannot hide a stalled process worker.
- ProcessTab and ProcessRow are memoized. Row callbacks are stable; one-second system updates do not rerender the process table with unchanged props.
- Dashboard metric subscriptions pause on document visibility changes. Overlay system subscriptions stop while hidden or in native taskbar mode. Native visibility-event behavior still needs verification.
- The native taskbar has its own renderer. The overlay WebView never resizes the taskbar.

## CSV Safety

- CSV is UTF-8 with BOM, CRLF record separators, and quoted/escaped process names.
- Names starting with formula characters `=`, `+`, `-`, `@` (including whitespace before them), or leading tab/CR/LF, are prefixed with an apostrophe before CSV quoting for Excel text interpretation.
- Export contains the current filtered groups and retained records, not all historical process activity. Actual Excel import behavior has not been exercised in this session.

## Acceptance Checklist

- [x] Local helper tests cover fetch/event races, stale sequences, late unlisten, disposal, duplicate polls, and identical process heartbeats.
- [x] Local model/render tests cover PID reuse, real PID action targeting, retired-row states, network unavailability, group duration, and Excel formula escaping.
- [ ] Native overlay: drag while pinned/unpinned; pin/close controls do not start dragging; hide/reopen restores authoritative state.
- [ ] Native taskbar: no overlay WebView sizing calls, at most two renderer rows, no taskbar/widget overlap, and async placement failures appear in dashboard diagnostics.
- [ ] Native geometry: 420px/800px/980px widths; 100/125/150/200 percent DPI; long names and 24-hour durations. Only the process table scrolls horizontally.
- [ ] Desktop lifecycle: minimize/hide/reopen, Explorer restart, display/DPI changes, unsupported geometry, and rapid mode/opacity changes.
- [ ] Long run: churn beyond 512 terminated identities; UI explains eviction and group-total decreases; verify CPU/memory while hidden and over 24 hours.
- [ ] Real Excel: formula-like names remain text, Vietnamese accents remain valid, and multi-PID grouped exports are interpretable.

Native interactive acceptance and a 24-hour resource profile are unverified. Automated frontend tests do not establish those properties.
