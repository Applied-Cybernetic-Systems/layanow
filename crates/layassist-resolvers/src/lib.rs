//! Text resolvers: turn a screen gesture (a dragged region, or the current
//! selection) into text.
//!
//! See `RESOLVERS.md` for the resolution chain and per-platform backends.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use layassist_core::{Rect, Selection};

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

/// A source of text from the screen.
///
/// Implementations must be side-effect free with respect to the clipboard
/// (see ADR-5): they may read the selection buffer but must not overwrite it.
pub trait TextResolver: Send + Sync {
    /// Human-readable resolver name (for logs and the UI).
    fn name(&self) -> &'static str;

    /// Whether this resolver can run on the current system.
    fn available(&self) -> bool;

    /// Resolve the region the user dragged, if it contains text.
    fn resolve_region(&self, rect: Rect) -> Result<Option<Selection>, ResolveError>;

    /// Read the app's current native text selection, if any.
    fn resolve_current_selection(&self) -> Result<Option<Selection>, ResolveError>;
}
