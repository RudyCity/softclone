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
