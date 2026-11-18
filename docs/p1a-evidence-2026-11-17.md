# P1A Tauri Host Cohort Evidence

Date: 2026-11-17. Branch: `codex/24x7-native-validation-20261010`.
Boundary: `docs/DELIVERY.md`. P1A changes the host dependency cohort and CI, not product scope.

## Delivered

| Component | Pinned version |
| --- | --- |
| tauri Rust | 2.12.6 |
| tauri-build | 2.7.5 |
| tauri-plugin-opener Rust / JS | 2.7.3 / 2.7.3 |
| tauri-plugin-single-instance | 2.5.5 |
| @tauri-apps/api / CLI | 2.12.5 / 2.12.5 |
| package manager | pnpm 11.21.0 |

Official registry metadata was queried immediately before the batch. These are a dated stable snapshot,
not a claim that versions will stay latest. Tauri API/host minor and opener patch are aligned according
to [Tauri's dependency guidance](https://v2.tauri.app/develop/updating-dependencies/).
The [single-instance documentation](https://v2.tauri.app/plugin/single-instance/) requires Rust >=1.90
and registration before other plugins; P0 registration order is unchanged.

Declared MSRV increased from 1.89 to 1.90; tested compiler remains 1.93.1, Node remains 26.13.0.
Latest Rust/Node, sysinfo/Windows bindings and frontend majors are separate remaining P1 batches.
No Rust/Node installation or unrelated dependency modernization was performed in P1A.

The existing JS install/store was reused once. pnpm added only exact-version release-age exceptions
for the selected Tauri API/CLI and its native CLI packages. No wildcard/global policy disable.
Cargo resolved the four pinned roots and 72 compatible transitive package changes; existing collector,
history and P0 Rust source were not edited by this batch. Three generated schema/ACL files were retained.

## Verification

- New `test:host` guard: three tests first failed against P0 declarations, then passed after pinning.
  It consumes package JSON, installed package metadata and structured Cargo metadata.
- Frontend regression: **60 passed in 13 files**. The first run caught Vitest discovering a Node-only
  `.test.mjs` file; renaming the guard to `verify-host-dependencies.mjs` isolated the runners. No assertions
  were weakened and no application logic changed to address that tooling failure.
- Frontend production build passed, including Tauri's final beforeBuildCommand.
- Browser smoke passed at 1366/780/420px, overlay 336x164, without a server. Native IPC is mocked.
  Existing Playwright expected a missing Chromium revision; the single non-installing fallback used
  installed Edge. No browser download/reinstall. Output: `%TEMP%/netdisk-monitor-p1a-browser`.
- Rust library: **151 passed, 0 failed, 8 explicitly ignored**, 57.73 seconds; compile 3m40s.
- Strict all-target Clippy passed on the new cohort, 2m01s; rustfmt check passed.
- Independent static cohort/CI/schema review found no confirmed remaining actionable findings.
  The generated ACL adds exit/fullscreen permission definitions, but default sets and application
  grants are unchanged. Runtime authorization was not tested by that review.
- CI now has Windows/macOS desktop matrix, pinned tested Node/Rust/pnpm, bounded jobs/timeouts,
  frozen/locked commands, tests/lint, Windows NSIS/macOS binary artifacts. Existing macOS ABI/live
  probe purpose and matrix remain. Final PyYAML structural validation passed; GitHub execution is open.
- UTF-8 and case-sensitive mojibake scan passed for all changed/new checkpoint files; Vietnamese
  README content was visually read back. Git diff whitespace checks passed (CRLF notices only).

## Windows Validation Artifacts

The `pnpm exec` Windows wrapper rejected Cargo arguments before compilation. One direct local CLI
fallback passed parsing and completed; CI uses the direct command as well:

```powershell
.\node_modules\.bin\tauri.cmd build --features desktop --bundles nsis --ci -- --offline --locked --jobs 2
```

Optimized release compile: **6m33s**. NSIS packaging: exit 0, one bundle, completed at 19:30 Asia/Saigon.
Cargo was offline; Tauri's bundler separately refreshed its NSIS 3.13 / utils 0.5.3 cache, reporting
hash validation. This was packaging tooling, not an app install or a dependency reinstall loop.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `src-tauri/target/p1a-2026-11-17/windows-x64/NetDisk-Monitor-P1A.exe` | 10,636,800 | `2D67ECD59E9C86C4CA6DD20B09530696853F4642C7FDD5A96B10829B491C69C1` |
| `src-tauri/target/p1a-2026-11-17/windows-x64/NetDisk-Monitor-P1A-setup.exe` | 2,331,234 | `2AE557D8006507D415DE4F36CC4937F1CDE5333947C52EDC2E10DE8ACD7C6312` |

Both are unsigned (`NotSigned`). Executable PE verified as AMD64 / Windows GUI subsystem. P0 artifacts
remain in their separate directory. Neither executable nor installer was launched or installed.

## Still Open

This is not the finished redesign or a taskbar fix. Production UI is unchanged; the approved four-view
wireframe has not been integrated. Native lifecycle/focus/taskbar/overlay, autostart, elevated ETW,
actual whole-tree CPU/RAM baseline, macOS builds on GitHub/hardware, packaging acceptance and 24-hour
app soak remain unqualified for this candidate. Browser/unit/CI structure checks do not replace them.

No automatic commit, push, merge, app replacement or release publication. The release destination
and stop rules remain in `docs/DELIVERY.md`; do not add unrelated features to fill incomplete gates.
