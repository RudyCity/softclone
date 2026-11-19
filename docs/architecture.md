# Architecture and compatibility

SoftClone is a user-space portable app duplicator, **not universal application virtualization or a license bypass**.

Copy pipeline: validate the source and relative entrypoint; write to unique temporary directory; reject external filesystem links (preserving safe relative internal symlinks on Unix); copy normal files and directories; atomically rename within the same filesystem; save metadata to an on-disk registry; launch the cloned executable by direct process API. No shell interpolation or admin elevation.

Rune powers a data-only clone label function. It cannot call OS file or process functions.

Windows supports portable exe files and folders, but apps may rely on COM, the registry, services, installers, activation, Known Folders or DRM. macOS supports executable files and simple app bundles, but signatures, LaunchServices, sandboxes and absolute or external symlinks can prevent duplication. Linux supports executables, AppImages and folders, but system libraries, FUSE and services can remain necessary.

Profile variables: XDG_CONFIG_HOME, XDG_DATA_HOME, XDG_CACHE_HOME, APPDATA, LOCALAPPDATA, plus SOFTCLONE_PROFILE_DIR. These are best-effort application hints and are not a security sandbox or a guarantee of separate accounts. macOS LaunchServices may ignore the environment variables.

## Follow-up development

P1: native file picker, copy progress and cancellation, macOS bundle symlink support, repaired registry flows, explicit-confirmation deletion, signed installers.
P2: app-specific adapters for apps with supported profile flags, optional authorized profile migration, snapshot and recovery support.
P3: opt-in, OS-specific virtualization integrations after prerequisites and threat-model review.

## Version 0.2: Universal compatibility track

Each instance stores a RunMode field. Previously saved instances default to Copy, preserving backward compatibility. ProfileOnly creates directories and metadata without duplicating the installed binary. At launch the engine resolves its original target and supplies known browser profile CLI flags.

Browser adapters: Chromium, Google Chrome, Microsoft Edge, Brave, Vivaldi (user-data-dir); Mozilla Firefox (no-remote and profile). All paths are passed through process APIs, not a shell.

### Compatibility tiers

| Tier | Example | Guarantee |
| --- | --- | --- |
| Independent profile | Known browsers with documented CLI profile flags | Separate explicit profile directory, subject to vendor behavior |
| Portable copy candidate | AppImage, executable, portable directory | File copy only; dependencies may be missing |
| Manual review | Generic installed Win32 app or macOS bundle | No isolation guarantee; best-effort environment variables only |
| Not supported | Store/MSIX, Flatpak, Snap, driver, service and DRM-bound apps | Requires vendor-specific adapters or genuine OS isolation |

Discovery reads program directories and desktop entry metadata without launching apps. Linux desktop Exec values are parsed as data; wrapper commands and shell constructs are skipped. No app sessions, passwords or browser profiles are imported.

Future OS-assisted isolation would be an explicitly opt-in integration requiring capabilities and permissions from Windows Sandbox/Hyper-V, Linux containers/namespaces or macOS OS sandbox facilities. These are not implemented by the current user-space engine.

## Windows installed programs (v0.2.1)

The Windows discovery source reads native uninstall records from HKLM and HKCU in both 32-bit and 64-bit registry views; uses DisplayName as the user-visible name and avoids version-directory names, update helpers and uninstallers. DisplayIcon and InstallLocation can supply a validated executable candidate. Unresolved records are shown but cannot be selected automatically. Browser executable scanning is only a fallback, with explicit known executable names. Windows Store/MSIX entries are not yet integrated.

All registry operations are read-only, and no installed application's configuration is modified by discovery. Discovery remains background-threaded. Full profile independence is only promised for known compatible adapters.

UI icons are drawn using egui Painter vector strokes, avoiding unsupported Unicode glyph fonts.

## UI modularity (v0.2.2)

The desktop shell owns navigation, registry integration, background workers and global notices. Form components own only presentation and controlled input changes: shared fields, the installed-application search toolbar and result cards, and the clone creation steps. Form components return user actions to the shell, which handles process/IO operations. Scanning and instance creation remain off the UI thread. This separation makes form styling reusable and avoids loading indicators affecting search input layout.

## Windows inventory and local source picker (v0.3.0 milestone)

- Windows uninstall-registry entries expose publisher and display version as optional metadata. These fields and executable paths can be searched; GUI filters are deterministic and unit-tested.
- Installed-program icons are extracted from primary executables with the Windows icon resource extractor in a bounded worker; GPU textures are created on the UI thread and held in a local cache. There is no executable invocation or elevation to extract an icon. A vector fallback is used when icons cannot be extracted.
- Source input now supports OS-provided file/folder dialogs. These are deliberately modal on the UI thread for native macOS window integration; no filesystem operation occurs until a user selects a source and submits the existing validated clone form.
- MSIX/Windows Store packages are not yet enumerated. Win32 uninstall registry entries are not a complete inventory of all packages; Windows PackageManager API integration remains an outstanding feature.
- Icons for macOS/Linux are currently fallback vectors rather than native app artwork; real icon extraction for these OSes remains outstanding.
- The scanner remains read-only and the engine does not bypass licenses, DRM, services, drivers or OS sandboxing.


## Smart Clone Engine (v0.4.0)

\`analyze(prepared)\` is read-only and reports \`CopyPlan { total_bytes, total_files }\` after validating symlinks and unsupported source file types. No library is created by analysis. For portable mode, \`create_clone_with_progress\` re-runs preflight, performs the original nested-library checks, writes files in 256 KiB chunks, periodically invokes \`CopyProgress\`, and checks a shared \`AtomicBool\` cancellation flag. It preserves file permission bits and relative symlinks that resolve within source on Unix. It does **not** preserve all extended filesystem metadata.

The \`.partial\` staging directory is removed on any copy error or cancellation. Registry write failure triggers a best-effort removal of the newly created clone directory. Source and instance folders are not touched during preflight. Original \`create_clone\` delegates to the new engine with cancellation disabled, preserving existing API and registry compatibility.

The desktop form provides a separate nonblocking preflight worker with stale-result suppression (input fingerprint), and a copy worker that streams progress to the UI. Cancellation requests remain visible while cleanup is pending; successful copies are saved and shown in the library. Profile-only instances continue to use the existing lightweight creation path, as their binaries are not copied.

Copy estimates do not currently check the target filesystem's free space. Before large clones, users should ensure sufficient free storage. True filesystem snapshots, atomic source isolation, full extended-attribute preservation and privileged OS sandboxing remain out of scope.

## v0.5.0: managed instance lifecycle

`instance_manager` exposes `rename`, `duplicate`, `backup`, `list_backups`, `restore`, and `delete`, with library and registry paths passed explicitly. The implementation refuses a root outside the canonical library or with a mismatching UUID suffix, refuses symlink roots, and verifies the matching registry record. Backup metadata includes snapshot ID and source instance ID. Restore checks both IDs and stages directory swaps with rollback when possible. Delete renames the instance directory before committing the registry update, and rolls that rename back if registry persistence fails.

The UI maintains a map of child process handles launched by SoftClone in the current session. It polls exit status, displays the PID, and only provides Stop/Restart for tracked child processes. It never sends signals to arbitrary PIDs or assumes externally launched processes are stopped. The process map is intentionally ephemeral. A macOS `open` launcher may exit before the app, so the tracked child is not necessarily the actual GUI app.

Backups are plain local folders and can include cookies, credentials, private browser profiles and application data. They are not encrypted or compressed. Long-running operations run on a worker thread; the UI shows busy state and explicit destructive confirmations. Tracked running processes block duplicate/backup/restore/delete, but external processes cannot reliably be detected. Instance backups are retained after deleting an instance.

Known gaps: encrypted backup export, OS process-tree tracking, source filesystem snapshots, accurate external process tracking, and native installers are not included. When cleanup fails after a registry update, SoftClone reports the staged recovery path.

## Zoom multi-instance compatibility (v0.5.1)

`universal::adapter_for` recognizes exactly Windows `Zoom.exe` as a Zoom-specific adapter; helpers/updaters are excluded. `adapter_args_for_instance` accepts the already persisted instance UUID and produces `--multipt=TRUE` and `--data=data<uuid-simple>`, per Zoom's Windows developer documentation. This makes the selector unique across separate clone records and stable across SoftClone restarts, including older profile records. Existing browser adapter behavior remains unchanged.

Zoom on Windows keeps normal OS APPDATA and LOCALAPPDATA while providing a separate Zoom data selector, avoiding env-only isolation that did not guarantee multiple instances. Launch feedback distinguishes a process spawn request from confirmed independent GUI windows. The UI labels unsupported profile adapters as experimental. Official docs: https://developers.zoom.us/docs/zoom-apps/guides/testing/.

No Zoom executable, install registry entry, account session, license or vendor config is modified by the adapter. This feature is not a generic multi-instance bypass; Zoom's command line options are vendor-documented. Windows-only automatic behavior. Runtime verification on a user-owned Windows device remains necessary.

## v0.5.2 packaged application discovery and native activation

On Windows, `windows_store` runs a fixed, non-elevated, read-only PowerShell discovery command in the existing background discovery worker: current-user `Get-AppxPackage`, manifest application IDs from `Get-AppxPackageManifest`, and human-friendly labels from `Get-StartApps`. Records must have valid bounded AUMIDs (`PackageFamilyName!ApplicationId`) before display or activation. No discovered value is interpolated into executable code. The source outputs JSON parsed with `serde_json`; unsupported or malformed entries are ignored. A separate Store entry is preserved even when a Win32 entry shares a product name.

Packaged apps have `store_id`, an intentionally empty executable path, `ReviewRequired` compatibility, and an `Open app` action using `explorer.exe shell:AppsFolder\<AUMID>`. This is ordinary OS activation, not managed cloning or isolation. An app without a registered launcher is not presented as directly launchable. Missing executable filters apply only to Win32 entries without usable targets.

On macOS, `macos_candidate` tags `.app` bundles containing `Contents/_MASReceipt/receipt` as probable Mac App Store distributions; the UI opens these through `open -a` rather than presenting direct clone support. The receipt is a heuristic only and does not bypass purchase validation. Existing bundle enumeration remains read-only.

Known Win32 launcher resolution adds name-specific executable lists and root discovery for Illustrator, Android Studio, CapCut and OBSBOT Center while excluding plugin and updater paths. Unresolved Win32 records instead offer an explicit manual executable file dialog. No arbitrary file is silently selected.
