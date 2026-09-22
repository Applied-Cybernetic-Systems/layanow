//! Platform-agnostic core types and configuration for `layassist`.
//!
//! This crate contains no I/O and no platform code; it is shared by every other
//! crate in the workspace.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// A rectangle in logical screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge, in logical pixels.
    pub x: i32,
    /// Top edge, in logical pixels.
    pub y: i32,
    /// Width, in logical pixels.
    pub width: i32,
    /// Height, in logical pixels.
    pub height: i32,
}

/// Where a piece of resolved text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The operating system's selection buffer (e.g. PRIMARY on Linux).
    Selection,
    /// The accessibility tree.
    Accessibility,
    /// Optical character recognition (reserved; not implemented in v1).
    Ocr,
}

/// A block of text resolved from the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The resolved text.
    pub text: String,
    /// Which resolver produced it.
    pub source: Source,
    /// Screen bounds, when the resolver can report them.
    pub bounds: Option<Rect>,
}

/// The role of an item within a decision session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The question being answered.
    Question,
    /// A candidate answer.
    Answer,
}

/// One captured item: the question or an answer, with its resolved text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Whether this item is the question or an answer.
    pub role: Role,
    /// The resolved selection.
    pub selection: Selection,
}
