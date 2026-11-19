# SoftClone design: Antislop and Taste Design v1 principles

The named Antislop / Taste Design v1 skill files were not installed here. This project uses their requested intent as product design guidance, not a claim to have read those exact files.

**Visual language**: pine accent #286960, near-black slate #18222B, warm white background #F8F9F7, charcoal navigation #16262A, hairline borders #DDE4E1. Native system fonts, purposeful spacing, compact functional rows, one primary action per view. No gratuitous gradients, colorful badges, AI decoration or fake metrics.

**Information architecture**: My clones (actual counts, bytes and launch actions), Create clone (source/details/profile), How it works (actual supported boundaries).

**Copy**: accurate language rather than claims of unlimited app duplication or full security isolation. Explicit user consent and clear errors. The creation process runs on a background thread. All visible buttons have actions; display an empty state when the registry has no entries. Do not synthesize runtime activity or success before the file copy and registry save return.

## Compatibility-led UX

Installed app discovery has a read-only scan, search and a single explicit selection action. The selected source passes into the creation form, which asks whether to copy files or create a profile-only instance. Support notes distinguish known browser adapters from experimental generic application handling. No fake universal-success states or security claims.

## Reusable form components (v0.2.2)

- `src/components/fields.rs` is the shared label/input/helper/section visual vocabulary.
- `src/components/discovery_form.rs` contains an integrated search field with its icon inside the input, a distinct Rescan action, loading/loaded status, and responsive application cards. Avoid placing status strings into horizontal toolbars that can clip.
- `src/components/create_form.rs` owns source, details, strategy, and submit forms. Inputs use consistent heights; helper notes wrap; invalid submissions stay disabled; persistent work remains outside the view component.
- All forms share the same neutral palette and border system and adapt to available content width, not absolute screen coordinates.

## v0.3.0 installed-app inventory

Search applies across name, publisher, version and path. Filter tabs distinguish profile-ready browsers, manual review and unresolved executable entries; result cards retain honest support notes. Windows exe icons load off the UI thread and fall back to the app vector icon without placeholder glyphs. Create clone includes a native file/folder browse affordance while retaining path editing for advanced use.


## Smart Clone preflight and progress (v0.4.0)

The Create clone form adds a fourth group, **Preflight Check**, with an explicit Analyze source action and a result line showing estimated bytes and file count. For profile-only mode, it explicitly states that no binary copying occurs. The worker never blocks egui during source traversal. While cloning, the submit action is replaced with a progress bar, completed/total byte and file counts, the current filename, and an explicit Cancel action. A cancellation request disables repeated cancel clicks and surfaces the final cleanup result in the standard notice component.

## v0.5.0 Instance Manager actions

Each clone card is a reusable component showing a tracked PID when running, otherwise 'No managed process tracked'. The action row provides Launch/Stop/Restart, Folder, Rename, Duplicate, Backup, Restore, and Delete. Rename and Duplicate validate user-facing names, Restore requires selecting a backup, and Delete requires typing the exact instance name. Heavy folder operations use a worker and display busy status without blocking the UI.

## v0.5.2 native Store inventory

Discover cards distinguish Win32 launchers, AppX/MSIX packages, Mac App Store bundles and OBS plugins. Win32 entries with unresolved executables show a secondary **Locate executable…** picker rather than a disabled action. Package entries show **Open app** with a tooltip explaining that native package launching does not create a separate SoftClone instance. Search supports AUMID, product names, publisher and version; a Store entry is distinct from a same-named Win32 entry.
