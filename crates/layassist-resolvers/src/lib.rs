//! Text resolvers: turn the user's native text selection into text for the app.
//!
//! See `RESOLVERS.md` for the resolution chain and per-platform backends. v1
//! reads the OS selection buffer only (ADR-13); [`StubResolver`] stands in for
//! the Linux backends until M4.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::collections::VecDeque;
use std::sync::Mutex;

use layassist_core::{Selection, Source};

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

/// A resolver whose selections are queued by the caller (M3 stub).
///
/// Until the Linux backends land in M4, the overlay's manual entry field queues
/// a selection here, which [`TextResolver::resolve_current_selection`] drains
/// one at a time. It touches no clipboard and no OS state.
#[derive(Debug, Default)]
pub struct StubResolver {
    queued: Mutex<VecDeque<Selection>>,
}

impl StubResolver {
    /// Create an empty stub resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a selection with [`Source::Selection`] and no bounds.
    pub fn queue(&self, text: impl Into<String>) {
        self.queue_selection(Selection {
            text: text.into(),
            source: Source::Selection,
            bounds: None,
        });
    }

    /// Queue a fully-formed selection.
    pub fn queue_selection(&self, selection: Selection) {
        if let Ok(mut queued) = self.queued.lock() {
            queued.push_back(selection);
        }
    }

    /// Number of selections waiting to be resolved.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.queued.lock().map_or(0, |queued| queued.len())
    }
}

impl TextResolver for StubResolver {
    fn name(&self) -> &'static str {
        "stub"
    }

    fn available(&self) -> bool {
        true
    }

    fn resolve_current_selection(&self) -> Result<Option<Selection>, ResolveError> {
        Ok(self.queued.lock().map_err(|_| ResolveError::Platform("poisoned".into()))?.pop_front())
    }

    fn watch(&self) -> Option<SelectionStream> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_resolver_drains_queued_selections_in_order() {
        let resolver = StubResolver::new();
        assert!(resolver.available());
        assert_eq!(resolver.pending(), 0);
        assert!(resolver.resolve_current_selection().unwrap().is_none());

        resolver.queue("first");
        resolver.queue("second");
        assert_eq!(resolver.pending(), 2);

        let first = resolver.resolve_current_selection().unwrap().unwrap();
        assert_eq!(first.text, "first");
        assert_eq!(first.source, Source::Selection);
        assert_eq!(resolver.resolve_current_selection().unwrap().unwrap().text, "second");
        assert!(resolver.resolve_current_selection().unwrap().is_none());
        assert!(resolver.watch().is_none());
    }
}
