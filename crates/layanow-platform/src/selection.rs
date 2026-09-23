//! Platform selection resolvers (ADR-13): read the OS selection buffer.
//!
//! v1's only text source is the native selection buffer. On Linux/Wayland the
//! backend reads the **PRIMARY** selection with `wl-clipboard-rs` (equivalent
//! to `wl-paste -p`); PRIMARY is separate from the clipboard, so reading it
//! never clobbers the user's clipboard (ADR-5). Windows/macOS backends and the
//! X11 fallback are later milestones (M4/M7/M8).
//!
//! The trait itself lives in `layanow-resolvers`; the OS calls live here so
//! the rest of the workspace stays portable (see `AGENTS.md`).

use layanow_resolvers::TextResolver;

#[cfg(target_os = "linux")]
mod wayland;

#[cfg(target_os = "linux")]
pub use wayland::PrimarySelectionResolver;

/// The best selection resolver available on the current platform.
///
/// There is exactly one source in v1 (the selection buffer, ADR-13); this is
/// the seam where the resolution chain would grow.
#[must_use]
pub fn resolver() -> Box<dyn TextResolver> {
    #[cfg(target_os = "linux")]
    let resolver: Box<dyn TextResolver> = Box::new(PrimarySelectionResolver::new());
    #[cfg(not(target_os = "linux"))]
    let resolver: Box<dyn TextResolver> = Box::new(UnavailableResolver);
    resolver
}

/// A resolver for platforms whose selection backend has not landed yet.
#[cfg(not(target_os = "linux"))]
struct UnavailableResolver;

#[cfg(not(target_os = "linux"))]
impl TextResolver for UnavailableResolver {
    fn name(&self) -> &'static str {
        "unavailable"
    }

    fn available(&self) -> bool {
        false
    }

    fn resolve_current_selection(
        &self,
    ) -> Result<Option<layanow_core::Selection>, layanow_resolvers::ResolveError> {
        Ok(None)
    }
}
