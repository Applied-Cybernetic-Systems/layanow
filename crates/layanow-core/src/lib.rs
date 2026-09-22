//! Platform-agnostic core types and configuration for `layanow`.
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
    /// Text the user typed into the overlay, not read from the screen.
    Manual,
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

/// An ordered capture session (ADR-26).
///
/// The first item captured is the question; every later item is an answer. The
/// model is list-shaped so multi-answer quizzes can be added later (ADR-8):
/// nothing here assumes exactly one correct answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    items: Vec<Item>,
}

impl Session {
    /// Create an empty session.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a captured selection and return the role it was assigned.
    ///
    /// The first item becomes the [`Role::Question`]; all others are
    /// [`Role::Answer`]s.
    pub fn push(&mut self, selection: Selection) -> Role {
        let role = if self.items.is_empty() { Role::Question } else { Role::Answer };
        self.items.push(Item { role, selection });
        role
    }

    /// All captured items, in capture order.
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The question item, if one has been captured.
    #[must_use]
    pub fn question(&self) -> Option<&Item> {
        self.items.first()
    }

    /// The answer items (everything after the question).
    #[must_use]
    pub fn answers(&self) -> &[Item] {
        self.items.get(1..).unwrap_or_default()
    }

    /// The question text, if captured.
    #[must_use]
    pub fn question_text(&self) -> Option<&str> {
        self.question().map(|item| item.selection.text.as_str())
    }

    /// The answer texts, in capture order.
    #[must_use]
    pub fn answer_texts(&self) -> Vec<String> {
        self.answers().iter().map(|item| item.selection.text.clone()).collect()
    }

    /// Whether the session has a question and at least one answer, i.e. enough
    /// to run a `choice` decision.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        !self.answers().is_empty()
    }

    /// Number of captured items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing has been captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Remove every item (e.g. when the overlay is dismissed or `Esc` is pressed).
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(text: &str) -> Selection {
        Selection { text: text.to_string(), source: Source::Selection, bounds: None }
    }

    #[test]
    fn first_item_is_the_question_and_the_rest_are_answers() {
        let mut session = Session::new();
        assert!(session.is_empty());
        assert!(!session.is_ready());

        assert_eq!(session.push(selection("q?")), Role::Question);
        assert_eq!(session.push(selection("a")), Role::Answer);
        assert_eq!(session.push(selection("b")), Role::Answer);

        assert_eq!(session.question_text(), Some("q?"));
        assert_eq!(session.answer_texts(), ["a", "b"]);
        assert!(session.is_ready());
        assert_eq!(session.len(), 3);
    }

    #[test]
    fn answers_without_a_question_are_not_ready() {
        let mut session = Session::new();
        session.push(selection("only"));
        assert!(session.question().is_some());
        assert!(session.answers().is_empty());
        assert!(!session.is_ready());
    }

    #[test]
    fn clear_empties_the_session() {
        let mut session = Session::new();
        session.push(selection("q?"));
        session.push(selection("a"));
        session.clear();
        assert!(session.is_empty());
        assert_eq!(session.question_text(), None);
    }
}
