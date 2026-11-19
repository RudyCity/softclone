# SoftClone

**Tauri 2 desktop application powered by Rust and Rune.** Make and manage local, user-controlled copies of portable apps, plus separate profiles for installed apps with documented adapters.

**Platforms:** Windows, macOS and Linux. **Project license:** [Apache-2.0](LICENSE) (see [NOTICE](NOTICE)).

## Technology

- **Tauri 2** hosts the native desktop window and its offline HTML/CSS/JavaScript frontend.
- **Rust** retains the safe copy engine, preflight, cancellation, registry, app discovery, launch control, backups, restore and diagnostics.
- **Rune** runs a data-only, embedded rule for clone-name suggestions. It has no OS or filesystem bindings.
- The frontend uses no web frameworks, CDNs, remote scripts or telemetry.

This release **replaces egui/eframe** entirely. Existing instance data stays in `dirs::data_local_dir()/SoftClone/instances.json`; there is no registry schema migration, and earlier clones remain available.

## Develop

Install [Rust stable](https://www.rust-lang.org/tools/install), Node.js and the OS prerequisites from [Tauri's setup guide](https://v2.tauri.app/start/prerequisites/). On Linux, install the GTK3/WebKit2GTK 4.1 development libraries, AppIndicator, librsvg, patchelf and the system build toolchain.

```sh
npm install
npm run dev
```

## Verify and build

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
node --check frontend/app.js
npm run build
```

`npm run build` produces **native Tauri installers and packages**, not the earlier unbundled egui executables. The CI workflow tests the core and builds native desktop bundles on Windows, macOS and Linux. Uploads are GitHub Actions artifacts; a release is only published after the platform build matrix passes.

## Features

- **My clones:** list, search, launch, stop and restart managed child processes; rename, duplicate, back up, restore, delete, and open managed folders.
- **Create clone:** portable-copy and profile-only modes; native file/folder chooser, path and entrypoint validation, preflight, progress and cancellation.
- **Discover apps:** read-only Windows Win32/Store inventory, macOS app bundles, Linux desktop launchers, compatibility notes and filters.
- **Debug logs:** local bounded event history with severity filtering, sanitized export and clear.
- **About:** version, platform, license, source, local data folder and optional [Ko-fi](https://ko-fi.com/rudycity).

Only directly tracked process handles can be stopped. The app does not elevate privileges or terminate arbitrary PIDs.

## Boundaries and privacy

SoftClone **does not** bypass activation, software licenses, DRM, signatures, platform sandbox rules, app services or installation systems. Portable file copying is not universal application virtualization. Browser-specific profile flags, Windows Zoom multi-instance arguments and environment-based profile paths are best effort and do not guarantee true isolation or distinct GUI windows. Windows Store/MSIX packages can be opened, **not cloned**; vendor rules continue to apply.

All clone files and diagnostic logs stay on the local device. Backups may contain private application data and are not encrypted. There is no background telemetry. Third-party software retains its own license.

See [architecture](docs/architecture.md), [design system](docs/design-system.md) and [Tauri migration notes](docs/releases/v0.6.0.md).
