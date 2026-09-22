//! Platform-specific backends: the Wayland overlay host, the selection
//! resolver, and the applet control socket. The global hotkey/tray (M5) and the
//! accessibility resolver (M9) are not implemented yet.
//!
//! This is the **only** crate permitted to contain `unsafe` code. Any `unsafe`
//! block must be isolated, minimal, and documented with a `// SAFETY:` comment.

#![allow(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// The applet control channel: cross-platform local-socket IPC (ADR-34).
pub mod control;

/// The overlay host: a Wayland layer-shell surface rendering egui (ADR-20).
pub mod overlay;

/// Platform selection resolvers: read the OS selection buffer (ADR-13).
pub mod selection;
