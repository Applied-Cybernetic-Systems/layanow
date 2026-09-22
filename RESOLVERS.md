# RESOLVERS

A resolver turns the user's **native text selection** into text for the app.

> v1 scope (ADR-13): the only text source is the OS **selection buffer**. The
> "3 resolvers" are the three **platform backends** — Linux, Windows, macOS.
> Accessibility-tree and OCR resolvers are future work.

## Interface (Rust)

```rust
pub struct Selection {
    pub text: String,
    pub source: Source,               // Selection (v1) | Accessibility | Ocr (future)
    pub bounds: Option<Rect>,         // screen coords, if known
}

pub trait TextResolver: Send + Sync {
    fn name(&self) -> &'static str;
    fn available(&self) -> bool;
    /// Read the app's current native text selection, if any.
    fn resolve_current_selection(&self) -> Result<Option<Selection>>;
    /// Watch for selection changes (for auto-capture); `None` if unsupported.
    fn watch(&self) -> Option<SelectionStream>;
}
```

`resolve_region` is removed from v1: the overlay is click-through and does not
receive drags (ADR-14).

Resolvers must be side-effect free with respect to the clipboard (ADR-5): read
the selection buffer; never overwrite the clipboard.

## The three platform backends

### Linux
- **Wayland:** read the PRIMARY selection via `wl-clipboard-rs`
  (`wl-paste -p` equivalent). PRIMARY is separate from the clipboard, so nothing
  is clobbered. If an app does not publish PRIMARY, optionally fall back to a
  simulated copy (`wtype`) **with clipboard save + restore** (opt-in).
- **X11:** read the PRIMARY selection directly (`x11rb` / `x11-clipboard`).

### Windows
- No PRIMARY selection. Read the clipboard after an explicit/simulated copy
  (`SendInput` Ctrl+C), **saving and restoring** the previous clipboard contents
  (opt-in, ADR-5), or read the clipboard as-is if the user copied manually.

### macOS
- No PRIMARY. Simulated copy (`CGEvent` Cmd+C) with clipboard save/restore
  (opt-in), or read the clipboard as-is.

## Capture model (ADR-26)

The overlay is click-through, so it does not see mouse events.

- **Auto-capture:** `watch()` the selection buffer; every new selection is
  appended to the item list (first = question, rest = answers).
- **Keyboard:** the overlay holds **keyboard interactivity** (Wayland layer-shell
  keyboard mode) so `Enter` (decide) and `Esc` (cancel) are delivered while the
  pointer passes through to the target app.

## Future resolvers

### `accessibility` — future
Read selected text / text-at-point from the a11y tree without copying:
Linux AT-SPI (`atspi`), Windows UIA (`uiautomation`), macOS AX
(`axuielement-rs`). Enables region resolution and clipboard-free reads.

### `ocr` — future
Screenshot a region and OCR it, for non-selectable text (ADR-3).

## Resolution order (v1)

1. Platform selection resolver → current selection.
2. If empty → applet popup error (ADR-18).
