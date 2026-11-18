# P2A System Core And Scoped Streams

**Goal:** Extract the system measurement reducer from Tauri and stop moving process rows to consumers
that only need system metrics, without pausing background accounting or changing history bytes.

**Spec:** Approved architecture in `docs/research-redesign-2026-11-17.md`; boundary `docs/DELIVERY.md`.
Use existing-branch inline implementation for the Rust integration and a scoped subagent for frontend.

## Boundaries

- One <=90-minute slice. No dependency/toolchain installs, format migration, new product features,
  native taskbar changes, app launch/install/replacement, automatic commit/push or dev server.
- Preserve P0/P1A dirty changes. No new worktree/store/cache loop.
- Keep system cadence 1s, process cadence 5s, existing stop/flush order and integer counter semantics.
- Keep `get_snapshot` as a legacy adapter. New readers use `get_system_snapshot` / `get_process_snapshot`.
- System JSON adds `activeProcessCount` and `processNetworkStatusError`; it contains no process rows.
  Legacy flattened `processNetworkError` remains distinct, avoiding duplicate JSON keys.
- Collector runs even if Process tab is hidden. Only UI subscription/bootstrap/table updates pause.
- No CPU/RAM/native-pixel claim from unit or browser tests. UTF-8 remains valid.

## Interfaces And Ownership

Parent owns `src-tauri/src/metrics_core.rs`, `collector.rs`, `service.rs`, `lib.rs`, CI/core gate and docs.
Frontend agent owns `src/App.tsx`, `src/meter.tsx`, `src/types.ts`,
`src/hooks/useMonitorSnapshot.ts`, its tests, and `scripts/browser-smoke.mjs`.
The frontend agent also owns nullable-count type/render changes in `OverviewTab.tsx` and `overview/SessionBand.tsx`.
Neither edits the other's files or package/lockfiles.

- `SystemObservation`: counters, interfaces, total/used memory bytes, disk availability; no Tauri.
  Existing collector `Collection` becomes an alias to avoid duplicate observation contracts.
- `SystemReducer::new(origin: Instant)` and
  `observe(observation, now: Instant, timestamp: u64, generation: u64) -> SystemFrame`.
- `SystemFrame`: MetricsSnapshot + observation metadata + sequence + StorageSample.
- `StorageSample::observed() -> ObservedSample` keeps the same timestamp/elapsed/byte ordering.
- `get_system_snapshot` returns only SystemSnapshot; `get_process_snapshot` only ProcessSnapshot.
  System summary count is cached with process observations, not computed by JS from process rows.
- `useMonitorSnapshot(processesEnabled = true)` uses narrow bootstrap commands and the existing
  `useTauriStream` enabled option. App passes active-tab state; overlay only uses system command.
- Summary diagnostic is available without a process subscription. Process controls/filter state remain mounted.

## Task 1: Pure Reducer And Rust Bridge

- [x] Add contract tests before implementing reducer; record initial missing-API red explicitly.
- [x] Implement reducer without AppHandle, I/O or wall-clock reads. Test baseline, reset/generation,
  elapsed rate, counter ordering, backwards wall/monotonic inputs and saturating sequence.
- [x] Integrate system worker and share StorageSample without altering persistence format.
- [x] Add scoped command helpers, cached process summary and serialization/lock-independence tests.
- [x] Core: 6 pass without desktop; final desktop: 162 pass/8 ignored; strict Clippy/fmt pass.

## Task 2: Demand-Driven Frontend

- [x] Extend mock commands and browser assertions; observe old bootstrap/subscription behavior fail first.
- [x] Use scoped commands, cached active count/diagnostic and enabled process subscription.
- [x] Prove initial Overview/overlay never bootstrap process rows, leaving Process removes listener,
  reentry fetches current rows, and existing controls/CSV/diagnostic regressions remain covered.
- [x] Frontend: 64 pass/build pass; Edge smoke/screenshots pass, no browser installs/server.

## Task 3: Review And Artifact

- [x] Independent focused review found no confirmed remaining actionable issues.
- [x] Add no-desktop core test gate to CI; existing matrix/probes preserved, CI execution open.
- [x] Build Windows release/NSIS, preserve earlier artifacts, capture hashes/unsigned state.
- [x] Update evidence and DELIVERY ledger. Native/memory/24h/full-UI gates stay open.

## Rulings

- Start this product-relevant P2 vertical slice before remaining independent P1 major upgrades: the
  stable host cohort is already verified, and the requested resource/data-flow work does not require
  a newer compiler or frontend major. Cost: repeat compatibility checks when those later batches land.
  The remaining upgrades are not deleted from release gates.
- Keep host diagnostic fields out of the pure reducer. Tauri adapter adds storage/taskbar/process status.
- Preserve hidden Process view state but freeze its data subscription; no new store/router dependency.
- Cache the active count inside ProcessSnapshot as a nonserialized host field, calculated on the
  process cadence. System adapter reads only that count/error under the existing snapshot lock;
  it neither clones rows nor scans every process on each system tick.
- `activeProcessCount` is nullable: no observation/summary access failure is null, a measured empty list
  is 0. UI renders N/A rather than coercing absence to zero.
- Confirmed P2A-01: a blocking summary read can couple system cadence to a large row clone. The
  focused contention test timed out after 2s before the fix. Use try_lock for summary only: busy yields
  unknown count/no asserted network status for that frame; poison yields unknown plus an error.

## Understanding

- [ ] User distinguishes keeping collectors running from pausing a hidden UI subscription.
- [ ] User explains why extracted deterministic tests do not prove native taskbar or measured CPU savings.
Do not mark these as demonstrated without an answer.
