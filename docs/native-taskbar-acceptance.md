# Native Windows Taskbar Acceptance

Scope: `src-tauri/src/taskbar.rs` and this document only. Cargo, dependencies,
other agents' files, and Git history were not changed by this work. No custom
external GUI automation, Explorer restart, or desktop-setting changes were used.
The ignored tests create only this module's native child and a temporary hidden
HWND sentinel, and destroy them afterward.

## Current Desktop Evidence

Acceptance ran on Windows version `10.0.26200.0`, on the existing bottom,
horizontal, centered-Start primary taskbar with widgets/weather present.
Taskbar DPI was `120` (125%). No claim is made about other configurations.

Final actual-desktop run:

```text
14 passed; 0 failed; 2 ignored (ordinary unit/lifecycle run, 0.01s)
2 passed; 0 failed (explicit ignored live run, 10.63s)
taskbar=[0, 1020, 1920, 1080], dpi=120
native child=[460, 1020, 644, 1080]
Start HWND=[654, 1020, 710, 1080]
measured left exclusion=198, physical gap=10, rendered rows=2
attached UIA refresh survived >5s
invalid bounds hid child; verified detection recovered
8 cycles: native child destroyed; supplied overlay HWND unchanged
```

All rectangles above are actual physical screen coordinates. The exclusion is
taskbar-relative and was remeasured through UIA, not supplied as a guessed offset.
The native child has class `NetDiskMonitor.NativeTaskbar.v1`, `WS_CHILD`, and
`GetParent(child) == Shell_TrayWnd`. The child fits inside the taskbar and leaves
at least ten pixels before Start and after the widgets/left-controls boundary.
Three input rows produced exactly two prepared native rows; the measured content
height fit inside the actual child rectangle.

Eight enable calls returned in 0.324-2.814 ms. Each disable call was asserted to
return in under 200 ms; asynchronous worker completion and HWND destruction took
192.346-252.053 ms in this run. These are observations, not platform-wide timing
guarantees. Re-enable of an unfinished retiring worker is covered by the focused
stalled-worker test.

The supplied HWND sentinel retained its original top-level parent, screen bounds,
and style throughout all eight cycles. It is a native `STATIC` test HWND, not a
real WebView. The renderer never reads or uses the overlay argument; the source
contains no `SetParent` call or WebView parenting API. A complete running Tauri
WebView workflow was not exercised by these tests.

## Test-Driven Defects And Fixes

1. Both original live tests failed because UIA geometry was rejected. The initial
   DPI-unaware probe saw `[0, 816, 1536, 864]`; the physical taskbar is
   `[0, 1020, 1920, 1080]`. The native worker now establishes a thread-only
   per-monitor-v2 DPI context and restores it on exit. Tauri/WebView process
   awareness and desktop scaling settings remain untouched.
2. The native XAML input host reported a scaled UIA rectangle despite its actual
   HWND being an empty `[0, 1020, 0, 1020]` child. The scan now verifies that known
   host's native parentage and empty/full-bar bounds before querying its XAML
   descendants. Partial/unknown host bounds are rejected. Other visible native
   child bounds are separately enumerated and reserved, so restricting the UIA
   subtree does not assume other HWND controls are absent.
3. A Start XAML rectangle could begin one physical pixel inside its Start HWND.
   Exact left-edge equality rejected valid measured containment. Start is now
   validated by positive-area containment inside the measured native Start HWND;
   the more conservative HWND edge remains the placement limit. There is no
   arbitrary pixel tolerance. Focused regressions reject out-of-bounds/empty
   Start rectangles and ambiguous host geometry.
4. After the first attachment succeeded, a fresh UIA query failed with
   `0x80131505` while querying this renderer's own HWND on its UI thread. A query
   condition alone did not solve it. UIA now scans the independent XAML-host
   subtree, structurally excluding our sibling HWND. The final live test both
   remeasured with the child attached and crossed the production five-second
   refresh interval without losing visibility.

The renderer continues to hide on unverifiable bounds, ambiguous geometry,
insufficient room, unsupported orientation, provider errors, and text that cannot
fit vertically. Deliberately invalid `i32::MAX` exclusion test data hid the real
child; returning to native detection restored it. No fabricated measured override
was used to obtain a passing attachment test.

## Reproduction

The taskbar module requires the `desktop` feature. A default-feature command
selects zero taskbar tests and is not native acceptance evidence.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features desktop --lib --offline taskbar:: -- --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --features desktop --lib --offline taskbar::native::live_tests -- --ignored --nocapture --test-threads=1
cargo clippy --manifest-path src-tauri/Cargo.toml --features desktop --lib --tests --offline -- -W clippy::pedantic
```

Run ignored tests only on an unlocked interactive Windows desktop, serially.
They intentionally fail rather than skip or guess when the current desktop cannot
provide verified bounds or a visible native child.

The initial Cargo desktop run demonstrated the failing live tests. A subsequent
whole-crate build was temporarily blocked by another agent's in-progress
`process_network.rs` (undefined `Packet`, `Transport`, and other symbols). To
avoid modifying that scope, final tests compiled this exact module via a rustc
stdin wrapper using existing Cargo-built artifacts. No source harness, manifest,
new dependency, or alternate renderer was created:

```powershell
$source = @'
#[path = "D:/Codetool/laragon/www/WORK/NetDisk-Monitor/src-tauri/src/taskbar.rs"]
mod taskbar;
use taskbar::TaskbarMode;
'@
$source | rustc --test --edition=2021 --crate-name taskbar_acceptance - -L dependency=src-tauri/target/debug/deps --extern windows=src-tauri/target/debug/deps/libwindows-43f6576634021a4a.rlib --extern windows_sys=src-tauri/target/debug/deps/libwindows_sys-0f3f6a351e04fc4e.rlib -L native=C:/Users/binhc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/windows_x86_64_msvc-0.52.6/lib -o "$env:TEMP/netdisk-taskbar-acceptance.exe"
& "$env:TEMP/netdisk-taskbar-acceptance.exe" --nocapture --test-threads=1
& "$env:TEMP/netdisk-taskbar-acceptance.exe" taskbar::native::live_tests --ignored --nocapture --test-threads=1
```

Artifact hashes are specific to this checkout's existing build, not portable
configuration. The parent should use the Cargo commands for whole-crate sign-off.

## Clippy

The same isolated wrapper and existing artifacts passed the installed
`clippy-driver` with `--test --emit=metadata -D clippy::all -D clippy::pedantic`,
exit 0 and no diagnostics. This proves the owned module and its tests are clean;
it does not assert that unrelated modules are warning-free.

No blanket `pedantic` suppression was added. The only intentional Clippy
allowances are function-local and carry `reason` explanations:

- GDI `i32` counts in `Paint::prepare` and `Paint::draw`: public input is bounded
  to 2048 Unicode scalars / 4096 UTF-16 units, 16 cells, and two rows.
- The Win32 `MA_NOACTIVATE` constant (`3`) fits `LRESULT` on 32- and 64-bit Windows.
- `refresh_now` retains one auditable discovery/validation/HWND-mutation sequence
  despite exceeding the line-count heuristic.

Other diagnostics were corrected: explicit raw-pointer borrows, checked time
subtraction, checked class-name length conversion, clone reuse, documentation
backticks, and the SDK's `E_FAIL` constant instead of handwritten HRESULT casts.

## Acceptance Checklist

- [x] Actual native renderer invoked on the current interactive desktop.
- [x] Native child class, `WS_CHILD`, and `Shell_TrayWnd` parent verified.
- [x] Actual child bounds checked against measured Start and widgets/left controls.
- [x] At most two rows and sufficient native content height verified.
- [x] Eight enable/disable cycles finish, destroy children, and preserve overlay HWND.
- [x] Fresh attached UIA scan and periodic refresh verified.
- [x] Invalid bounds hide; measured native detection recovers.
- [x] Owned module and tests pass Clippy all/pedantic as errors.
- [x] Whole-crate Cargo sign-off: 120 passed / 6 ignored; strict Clippy passed.
      Parent live Cargo rerun: 2 passed in 10.66s, identical bounds and DPI.

## Remaining Desktop States

Not exercised live because changing settings or restarting Explorer was forbidden:
auto-hide transitions; left-aligned Start; vertical/top taskbars; widgets disabled
or dynamically replaced; insufficient taskbar space; DPI changes and mixed-DPI
multi-monitor moves; secondary taskbars; Explorer loss/recreation; locked desktop
or remote-session transitions; UIA provider disappearance or timeout; high contrast
and alternate themes. Geometry rejection and stalled-worker behavior have focused
unit coverage, not live acceptance for these states. No macOS claim is made by
this Windows-only scope. Pixel-level typography and a full Tauri WebView workflow
also remain outside this test evidence.

## Rationale References

Microsoft documents that UIA uses physical coordinates in
[Understanding Screen Scaling Issues](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-screenscaling),
and that a thread's DPI context can be restored using the previous value returned
by [SetThreadDpiAwarenessContext](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext).
The host and Start distinctions above are observations from this live desktop,
not claims that every Explorer version exposes the same tree.
