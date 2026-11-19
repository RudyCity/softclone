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
