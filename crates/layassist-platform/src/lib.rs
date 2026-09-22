//! Platform-specific backends: Wayland overlay, global hotkey, and the
//! accessibility/selection resolver implementations.
//!
//! This is the **only** crate permitted to contain `unsafe` code. Any `unsafe`
//! block must be isolated, minimal, and documented with a `// SAFETY:` comment.

#![allow(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// Returns whether the accessibility resolver can run on this system.
///
/// Real implementation lands in M4 (Linux `atspi`).
#[must_use]
pub fn accessibility_available() -> bool {
    false
}
