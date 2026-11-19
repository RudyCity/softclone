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
