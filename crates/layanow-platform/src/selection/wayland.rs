//! Wayland PRIMARY-selection backend.
//!
//! Reads the focused seat's PRIMARY selection through `wl-clipboard-rs`, which
//! speaks `ext-data-control` / `wlr-data-control` v2 (the protocols window-less
//! clients use; `wl-paste -p` is built on the same). PRIMARY is a separate
//! buffer from the regular clipboard, so this never overwrites what the user
//! copied (ADR-5).

use std::io::Read;

use layanow_core::{Selection, Source};
use layanow_resolvers::{ResolveError, TextResolver};
use wl_clipboard_rs::paste::{ClipboardType, Error as PasteError, MimeType, Seat, get_contents};

/// Reads the Wayland **PRIMARY** selection (`wl-paste -p`).
///
/// Each call opens a short-lived connection to the compositor, requests the
/// current text offer, and reads it from a pipe. It holds no state between
/// calls, so it can be called from any thread. The data-control protocols have
/// no change notification, so there is no watch API (ADR-33).
pub struct PrimarySelectionResolver;

impl PrimarySelectionResolver {
    /// Create a PRIMARY-selection resolver.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for PrimarySelectionResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl TextResolver for PrimarySelectionResolver {
    fn name(&self) -> &'static str {
        "wayland-primary"
    }

    fn available(&self) -> bool {
        std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var_os("WAYLAND_SOCKET").is_some()
    }

    fn resolve_current_selection(&self) -> Result<Option<Selection>, ResolveError> {
        match get_contents(ClipboardType::Primary, Seat::Unspecified, MimeType::Text) {
            Ok((mut pipe, _mime_type)) => {
                let mut bytes = Vec::new();
                pipe.read_to_end(&mut bytes)
                    .map_err(|error| ResolveError::Platform(error.to_string()))?;
                Ok(selection_from_bytes(&bytes))
            }
            // An absent/empty selection is not an error: there is simply
            // nothing to capture (ADR-18 handles the "resolved nothing" case).
            Err(
                PasteError::NoSeats
                | PasteError::ClipboardEmpty
                | PasteError::NoMimeType
                | PasteError::SeatNotFound
                | PasteError::PrimarySelectionUnsupported,
            ) => Ok(None),
            Err(error) => Err(ResolveError::Platform(error.to_string())),
        }
    }
}

/// Turn raw selection bytes into a [`Selection`], trimming trailing line
/// breaks and treating whitespace-only text as "nothing".
///
/// Kept separate from the Wayland call so it can be unit-tested without a
/// compositor.
fn selection_from_bytes(bytes: &[u8]) -> Option<Selection> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches(['\n', '\r']);
    if text.trim().is_empty() {
        return None;
    }
    Some(Selection { text: text.to_string(), source: Source::Selection, bounds: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_trailing_newlines_but_keeps_inner_whitespace() {
        let selection = selection_from_bytes(b"  Which planet?\n\n").expect("text");
        assert_eq!(selection.text, "  Which planet?");
        assert_eq!(selection.source, Source::Selection);
        assert!(selection.bounds.is_none());
    }

    #[test]
    fn whitespace_only_is_no_selection() {
        assert!(selection_from_bytes(b"   \n\t").is_none());
        assert!(selection_from_bytes(b"").is_none());
    }

    #[test]
    fn invalid_utf8_is_lossy_rather_than_an_error() {
        let selection = selection_from_bytes(b"Mars\xff").expect("text");
        assert!(selection.text.starts_with("Mars"));
    }
}
