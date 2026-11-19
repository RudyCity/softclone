# SoftClone

A focused, native desktop workspace for making **local, user-controlled copies of portable applications** on Windows, macOS and Linux.

> **Scope:** SoftClone copies portable application folders, macOS `.app` bundles, and stand-alone executables / AppImages. It does not bypass activation, licenses, DRM, platform signatures, installation services, sandbox restrictions or apps bound to a single machine. Separating per-instance configuration through environment variables is **best effort**, not a security sandbox.

## Stack

- **Rust** handles file operations, path validation, app launching and persistence.
- **Rune** (embedded Rust scripting VM) calculates clone labels without filesystem or shell APIs.
- **egui / eframe** provides the native desktop interface.
- **GitHub Actions** checks formatting, builds and tests on Windows, macOS and Linux.

## Quick start

Install Rust stable (1.88+) and required system graphics development libraries for your OS.

```bash
cargo run --release
cargo test
```

Open **Create clone** and enter an accessible local path to an application directory, `.app` bundle, `.exe` file or AppImage. For a general directory, select a relative executable entry point. Copies are stored under the application data directory in `instances/`; the source is never modified. A fresh profile directory is prepared per clone.

## Current support

| Capability | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Copy portable files / folders | Yes | Yes | Yes |
| Copy / launch macOS .app bundle | — | Yes | — |
| Copy / launch standalone executable | .exe | executable | executable / AppImage |
| Instance-specific environment directories | Best effort | Best effort | Best effort |
| Full OS / kernel-level sandbox | No | No | No |
| Installed system app cloning | Not guaranteed | Not guaranteed | Not guaranteed |

For architectural details and threat-model limitations, see [docs/architecture.md](docs/architecture.md). Design principles: [docs/design-system.md](docs/design-system.md).

## Safety

Use only software you have permission to copy. SoftClone does **not** elevate privileges, run an installer, copy application credentials or import browser profiles. Symlinks are rejected while copying to avoid accidentally copying files outside the chosen source. All launches use explicit process APIs, not shell interpolation.

License: MIT.
