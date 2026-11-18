# P1A Tauri Host Upgrade Implementation Plan

> Use executing-plans for the coupled dependency changes and a scoped subagent for the independent CI file.

**Goal:** Move only the Tauri Rust/JS host cohort to the verified stable versions and keep automated
Windows/macOS validation reproducible without changing product behavior.

**Architecture:** Preserve P0 lifecycle/lease/health and current collectors/storage/UI. Pin the host cohort,
check declarations plus installed JS versions, resolve existing lockfiles and add a Windows/macOS CI matrix.

**Tech Stack:** Existing Rust 1.93.1, Node 26.13.0, pnpm 11.21.0; Tauri 2.12 host cohort.

**Spec:** `docs/research-redesign-2026-11-17.md`; release boundary `docs/DELIVERY.md`.

## Global Constraints

- One <=90-minute slice. No Rust/Node installation, React/Vite/sysinfo upgrades, UI/collector/history changes.
- Pins queried from official registries on 2026-11-17: tauri 2.12.6, tauri-build 2.7.5,
  opener Rust/JS 2.7.3, single-instance 2.5.5, JS API/CLI 2.12.5. Required MSRV 1.90.
- Existing compiler 1.93.1 meets that floor. Full toolchain modernization remains another P1 batch.
- No app launch/install/replacement, Explorer changes, signing, automatic push or CI-success claim without a run.
- Preserve UTF-8 and all existing P0 changes. Do not revert a dirty worktree or create a duplicate install/worktree.

## Task 1: Cohort Guard And Resolution

Files: `scripts/verify-host-dependencies.mjs`, `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`,
`src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`.

- [x] Add a Node test consuming package JSON and structured `cargo metadata --no-deps --offline`.
  Assert exact host versions, matching Tauri minor / opener patch, installed JS versions and MSRV >=1.90.
- [x] Observe red against the P0 declarations before changing manifests.
- [x] Pin the cohort and packageManager pnpm 11.21.0. Add `test:host` to package scripts.
- [x] Resolve JS once with existing node_modules/store and scripts disabled; update the four Cargo roots once.
  No force/purge/reinstall loop. A genuine environment error allows one non-installing fallback.
- [x] Guard: 3 pass; frontend: 60 pass/build pass; Rust: 151 pass/8 ignored; strict Clippy/fmt pass.

## Task 2: CI Matrix

File: `.github/workflows/macos-disk-validation.yml` only, owned by the CI subagent.

- [x] Preserve independent macOS native ABI/live probe jobs and permissions/triggers.
- [x] Change desktop build/test to a Windows/macOS matrix with explicit Node 26.13.0, Rust 1.93.1,
  pnpm 11.21.0 from packageManager, frozen/locked dependency commands, 2 Cargo jobs and bounded timeouts.
- [x] Configure `test:host`, frontend tests/build, Rust library tests and strict Clippy; package Windows NSIS,
  build macOS executable. Upload per-platform artifacts; no signing, release publishing or native-pixel claim.
- [x] Inspect workflow structure locally. GitHub execution is a separate gate; no push in this slice.

## Task 3: Candidate And Handoff

- [x] Independent static cohort/CI/schema review found no confirmed remaining issues; no default grants added.
- [x] Build Windows release/NSIS once after the cohort stabilizes; retain the P0 artifact separately.
- [x] Capture hash/unsigned status and generated artifact paths. Neither artifact launched/installed.
- [x] Update `docs/DELIVERY.md` and `docs/p1a-evidence-2026-11-17.md` with actual results and open gates.

## Rulings

- Continue in the existing validation branch/worktree: P0 is dirty and required by P1A; a new worktree or
  duplicate install would fragment the checkpoint and violate the anti-loop constraint.
- Pin CI to the already exercised local compiler/runtime for this cohort; latest Rust/Node and frontend
  major migrations remain explicit next batches. The cost is that P1 as a whole is not complete yet.
- The Node guard uses a non-`.test` filename: Vitest otherwise discovers Node's independent test suite
  and fails with "No test suite found". The `test:host` command still invokes Node test mode explicitly.
- pnpm added only version-specific minimum-release-age exceptions for the explicitly selected Tauri
  API/CLI cohort and its native CLI binaries. Preserve the allowlist, not a wildcard or global disable.
- The Windows `pnpm exec` wrapper lost the Cargo separator in the observed build invocation (unexpected
  `--offline` before compilation). One direct local `tauri.cmd` fallback passed parsing and began building;
  CI now uses that direct command too. No install/cache fallback was used for this error.
- Keep the three generated Tauri schema/ACL files with the upgraded cohort. New exit/fullscreen permission
  definitions were added by upstream, but generated default sets and application grants are unchanged.
