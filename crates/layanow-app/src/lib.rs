//! The `layanow` applet: item capture, inference wiring, and the overlay UI.
//!
//! This library holds the testable pieces of the binary; `main.rs` is the thin
//! entry point that wires them to the platform.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// Pure results-panel logic (rows, probability colours, low-confidence flag).
pub mod results;
/// The dedicated inference worker (ADR-1).
pub mod worker;

/// The full-screen, click-through overlay (ADR-14/25/26).
pub mod overlay;

/// Gruvbox (medium contrast) theme and shadowed text.
pub mod theme;
