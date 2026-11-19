# SoftClone Desktop — UI system (Tauri 2)

## Principles

SoftClone v0.6.0 uses a fully local HTML/CSS/JavaScript frontend hosted by a Tauri 2 desktop window. egui/eframe and their icon paint routines are no longer shipped.

- Neutral backgrounds, a deep teal vertical navigation, consistent border rhythm, 8–14px radii.
- Compact one-line primary actions; secondary actions are grouped within cards.
- Standard HTML button, label, input, select and modal semantics; all user-supplied strings are HTML escaped before insertion.
- Clear descriptions of copy/profile compatibility boundaries, including native Store app activation.
- Progress and cancellation are shown without blocking the webview UI.
- Local-only UI resources without third-party fonts, images, analytics, remote scripts or inline event handlers.
- Desktop responsive layouts adapt to a 900px minimum window width, reducing columns before truncating core controls.

## Pages

My clones, Create clone, Discover apps, Debug logs, How it works and About.

## Security contract

Every filesystem/process operation is invoked through an explicit Rust Tauri command and validated against the managed registry. The frontend is a presentation client, not a security boundary. The app does not expose arbitrary shell command execution, filesystem traversal or dynamic external URL opening. Destructive operations require in-app confirmation. The Tauri capability only grants core defaults.

## Accessibility

Inputs have visible labels or aria-labels. Buttons have explicit types. Keyboard focus has a visible outline. Modal dialogs have dialog labels and can be dismissed with Escape. Errors and non-sensitive completion notices are announced through a live status region.
