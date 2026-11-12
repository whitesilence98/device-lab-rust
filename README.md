# Device Lab

Standalone learning project: native Rust interface, `eframe` + `wgpu`, Windows Plug and Play metadata. No Electron, webview, JavaScript, vendor SDK, or connection to the Anne workspace.

**Source-only delivery. The app has not been built or run.** MSVC C++ Build Tools and Windows SDK were not found on the development machine. Nothing was installed. Dependency/API integration and visual fidelity remain unverified until a Windows build and manual review.

## Implemented scope

- Native dashboard-style device cards, original vector category illustrations, class filters and search.
- Windows present-device enumeration across vendors and setup classes, including unknown devices.
- Container-based grouping with all underlying OS records preserved in the detail panel.
- Background rescans about every two seconds plus manual refresh. Failed scans retain the last successful inventory with a stale warning.
- Read-only details: manufacturer, class, OS enumerator, instance/container/parent IDs, hardware IDs and optional USB VID/PID.
- Keyboard-operable buttons/cards, Ctrl+F search, Escape close/clear, AccessKit integration.
- Local theme and scale preferences, saved only when requested under `data/preferences.txt`.

Dashboard composition follows the existing application's card workflow, not a pixel-exact or branded copy. Generic illustrations replace commercial product images. No battery percentages, configuration controls or firmware indicators are invented.

## Safety and limits

Enumeration queries **Windows metadata**, not hardware endpoints. No input capture, HID reports, raw device commands, firmware updates, driver installation, device-setting writes, admin requirement, telemetry or network discovery.

Windows present-device nodes include internal/system/virtual devices and multiple nodes for composite peripherals. Bluetooth presence does not establish active connection. Unpaired BLE advertisements and arbitrary network devices are not discovered. A grouped container is an OS grouping, not a guarantee of one physical product. Missing metadata is displayed as unknown.

Instance IDs are case-insensitive record keys; VID/PID never identify an individual device. Identifiers may change across ports, driver changes or reinstallations. A device removed between enumeration calls can invalidate a scan; the previous snapshot is retained until the next successful scan. Polling can miss devices present for less than a scan interval. SetupAPI calls are not cancellable; shutdown signals the worker without blocking the UI on an active call.

No files, packages, assets, native binaries, links or path dependencies are imported from `D:\Work\anne`. Project-controlled output stays within this project. Windows and graphics drivers may maintain their own caches.

## Build prerequisites — install separately only with permission

- Existing Rust MSVC toolchain (development machine: Rust/Cargo 1.99.1).
- Microsoft C++ x64 build tools and Windows 10/11 SDK.
- Working graphics driver/backend supported by `wgpu`.

No toolchain installer or automatic setup is included. The Git distribution's `link.exe` is **not** the Microsoft linker. Use a Visual Studio Developer PowerShell once the prerequisites are available.

The application pins `eframe 0.31.1` to its documented `App::update` API and `windows-sys 0.59.0` to the inspected Win32 declarations. These are deliberate baseline versions, not a claim of latest releases. A transitive-dependency lockfile has **not** been fabricated; generate and retain `Cargo.lock` on the first dependency resolution, then use `--locked`.

## Commands

Run from this project, never from the existing workspace. These environment changes affect the current PowerShell process only:

```powershell
Set-Location "D:\Work\device-lab-rust"
$env:PATH = "C:\Users\tuan.nguyend\.cargo\bin;" + $env:PATH
$env:CARGO_HOME = "$PWD\.cargo-home"
$env:CARGO_TARGET_DIR = "$PWD\target"

cargo fmt --check
cargo generate-lockfile
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked
```

First Cargo dependency resolution downloads public dependencies into the project-local cache; no private repositories are needed. Do not run `rustup update` or install tools as part of these commands.

### Source-only validation without a linker

With the existing Rust binaries, these commands format and type-check the pure device model, including its test bodies. They **do not run tests** or validate external dependency integration:

```powershell
cargo fmt --check
rustc --edition 2021 --crate-type lib --emit metadata src/devices.rs -o target/devices.rmeta
rustc --edition 2021 --test --emit metadata src/devices.rs -o target/devices-tests.rmeta
```

Create `target` first if absent. No standalone executable is produced.

## Files

- `src/main.rs`: explicitly selects the native `wgpu` renderer.
- `src/app.rs`: dashboard, background worker, appearance preferences and preference tests.
- `src/devices.rs`: pure device model, grouping, search, snapshot reconciliation and tests.
- `src/windows_devices.rs`: narrowly scoped, documented Win32 unsafe boundary with handle cleanup and checked property buffers.

Preferences are anchored to this project's compile-time manifest directory, not the launch directory. This intentionally favors the learning checkout over portable packaging. Corrupt/unreadable preferences disable saving to preserve the original. Concurrent instances use last-successful-save wins; no shared database or inventory persistence.

## Validation completed

- `rustfmt --check`: passed for all four source files.
- `cargo metadata --offline --no-deps`: passed; one standalone package, only public registry dependencies, project-local target directory.
- `rustc --crate-type lib --emit metadata src/devices.rs`: passed.
- `rustc --test --emit metadata src/devices.rs`: passed; model test bodies type-check, but **tests were not executed**.
- Full dependency resolution, `Cargo.lock`, compilation, clippy, test execution and GUI/hardware smoke checks: **not performed**. Source-only delivery; missing MSVC/SDK.

## Verification still required

1. Build/check/test/clippy after the separately approved toolchain prerequisites exist.
2. Run as standard user. Compare representative records with Device Manager, accounting for grouped interfaces.
3. Plug/unplug USB devices; check refresh, responsiveness and removal details. Test two devices sharing VID/PID.
4. Test stale/error/empty states, search, keyboard navigation, display scaling and GPU startup failure.
5. Save/reload theme and scale. Corrupt preferences manually; verify the original is preserved and saving disabled.
6. Compare at a known reference window size before claiming visual fidelity. No reference screenshot or runtime comparison has been completed.
7. Confirm all generated output stays in this project. Existing repositories are not build or runtime dependencies.

## Later, not implemented

One explicitly selected device and one documented/authorized setting can introduce a supported write path. Add protocols and capabilities only when that device requires them. Detecting a device does not imply universal configuration support. Firmware, input remapping, cloud, installers and cross-platform enumeration require separate scope.

References: [egui](https://github.com/emilk/egui), [eframe 0.31.1 App API](https://docs.rs/eframe/0.31.1/eframe/trait.App.html), [Windows enumeration](https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetclassdevsw).
