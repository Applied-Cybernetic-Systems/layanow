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

/// A source of text from the screen.
///
/// Implementations must never touch the regular clipboard (ADR-5). They may read
/// the selection buffer, and — for the opt-in "clear on `Enter`" action — clear
/// the PRIMARY selection so the source app drops its highlight (ADR-46).
///
/// There is deliberately no change-notification API: the data-control
/// protocols expose none, and v1 capture is a manual `Tab`-commit (ADR-33).
/// Auto-capture (ADR-26), if it returns, would observe the overlay surface's own
/// `wl_data_device` and would need its own interface.
pub trait TextResolver: Send + Sync {
    /// Human-readable resolver name (for logs and the UI).
    fn name(&self) -> &'static str;

    /// Whether this resolver can run on the current system.
    fn available(&self) -> bool;

    /// Read the app's current native text selection, if any.
    fn resolve_current_selection(&self) -> Result<Option<Selection>, ResolveError>;

    /// Clear the current native selection so the source app un-highlights it.
    ///
    /// Used by the opt-in "clear on `Enter`" setting (ADR-46). Backends that
    /// cannot clear keep the default no-op. This must never touch the regular
    /// clipboard (ADR-5).
    ///
    /// # Errors
    /// Returns a [`ResolveError`] when the platform call fails.
    fn clear_current_selection(&self) -> Result<(), ResolveError> {
        Ok(())
    }
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
