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

Use only software you have permission to copy. SoftClone does **not** elevate privileges, run an installer, copy application credentials or import browser profiles. Relative symlinks that resolve within the chosen source are preserved on Unix; external links and Windows symlinks are rejected. All launches use explicit process APIs, not shell interpolation.

License: MIT.

## Universal compatibility track (0.2)

SoftClone now offers two separate strategies:

- **Portable copy:** creates a local copy of a self-contained executable, directory, AppImage or macOS app bundle. It cannot reproduce missing services, licensing or installation state.
- **Profile-only:** keeps installed binary files where they are and prepares separate local data directories. Known Chromium-family browsers use the --user-data-dir argument; Firefox uses -no-remote and -profile. Other apps may ignore those settings.

**Discover apps** scans Windows Program Files and user Programs, macOS Applications directories, and Linux desktop launchers and selected AppImage locations. Discovery does not execute candidate applications.

Installed applications without known profile adapters are labeled as requiring manual review. This initial discovery does not support Windows Store/MSIX packages, Flatpak/Snap sandboxed launches, system services, drivers, DRM or bypass of licensing or activation restrictions.

Select **Create clone** to choose copying app files versus using an installed app with a dedicated profile. This distinction matters: a profile is not a virtual machine or a security sandbox.

See docs/architecture.md for the current support matrix.

### Installed application discovery and vector icons (0.2.1)

Windows discovery now reads the native **Apps & Features / Installed Programs registry** (HKLM + HKCU, 32-bit and 64-bit views) instead of listing arbitrary executables and folders such as browser version `157.0.*`. It uses the registered `DisplayName`, resolves the real launch executable from `DisplayIcon` or `InstallLocation` when possible, filters system components and updates, and de-duplicates product names. A narrowly scoped executable fallback discovers known browser launchers only.

Registered software without a known safe launch path remains visible but cannot be selected automatically; provide the executable path manually instead. Microsoft Store/MSIX apps are not supported by this reader yet.

The navigation and status UI uses **native egui-painted vector icons**, not Unicode glyphs or system icon fonts. This avoids missing-glyph boxes on Windows, macOS and Linux. See [the release notes](docs/releases/v0.2.1.md).

### Reusable desktop form components (0.2.2)

Every form is now composed from reusable egui components under `src/components/`: `fields.rs` (text input, label, helper text, section dividers and integrated search icon), `discovery_form.rs` (search/rescan/status controls and responsive app cards), and `create_form.rs` (source, instance details, strategy and submit sections).

Discover search and Rescan are grouped in a unified toolbar, while scanning progress and match counts occupy their own status line. Explanatory text wraps instead of extending outside the viewport. The interface supports narrow and wide desktop content areas without detached search icons. Creation keeps its validation and existing Rune name suggestions.

This is a presentation and organization update; it does not claim new OS-level cloning isolation. [Release notes](docs/releases/v0.2.2.md).
