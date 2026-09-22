//! Text resolvers: turn the user's native text selection into text for the app.
//!
//! See `RESOLVERS.md` for the resolution chain and per-platform backends. v1
//! reads the OS selection buffer only (ADR-13); the Linux/Wayland implementation
//! lives in `layanow-platform::selection`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use layanow_core::Selection;

/// Errors returned by a [`TextResolver`].
#[derive(Debug)]
pub enum ResolveError {
    /// The resolver is not available on this system.
    Unavailable,
    /// The underlying platform call failed.
    Platform(String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "resolver unavailable on this system"),
            Self::Platform(msg) => write!(f, "platform error: {msg}"),
        }
    }
}

impl std::error::Error for ResolveError {}

/// A stream of selection changes produced by [`TextResolver::watch`].
pub type SelectionStream = Box<dyn Iterator<Item = Result<Selection, ResolveError>> + Send>;

/// A source of text from the screen.
///
/// Implementations must be side-effect free with respect to the clipboard
/// (see ADR-5): they may read the selection buffer but must not overwrite it.
pub trait TextResolver: Send + Sync {
    /// Human-readable resolver name (for logs and the UI).
    fn name(&self) -> &'static str;

    /// Whether this resolver can run on the current system.
    fn available(&self) -> bool;

    /// Read the app's current native text selection, if any.
    fn resolve_current_selection(&self) -> Result<Option<Selection>, ResolveError>;

    /// Watch for selection changes (for auto-capture, ADR-26), or `None` when
    /// the backend cannot report changes.
    fn watch(&self) -> Option<SelectionStream>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_display_helpfully() {
        assert_eq!(ResolveError::Unavailable.to_string(), "resolver unavailable on this system");
        assert_eq!(
            ResolveError::Platform("pipe closed".to_string()).to_string(),
            "platform error: pipe closed"
        );
    }
}
