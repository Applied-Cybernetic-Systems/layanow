//! Platform-agnostic core types and configuration for `layanow`.
//!
//! This crate contains no I/O and no platform code; it is shared by every other
//! crate in the workspace.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// A rectangle in logical screen coordinates.
///
/// Reserved: no v1 resolver populates a [`Selection::bounds`]; the
/// accessibility/OCR resolvers (ADR-3) are expected to report the screen region
/// a selection came from so the overlay can avoid covering it.
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
    /// Screen bounds, when the resolver can report them (reserved; every v1
    /// resolver leaves this `None`, see [`Rect`]).
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

/// Split a whole-quiz selection into its question and answer options.
///
/// Quiz mode's single-capture format (ADR-44). The question is the first
/// blank-line-separated block, so a wrapped question survives. Every non-empty
/// line after it is one option — whether the options are consecutive lines (a
/// Likert scale) or separated by blank lines. When there is no blank line at
/// all, the whole text falls back to one item per line.
///
/// Returns `None` when there is no question plus at least one option. The
/// caller treats that as "not a quiz" and captures nothing (all-or-nothing).
#[must_use]
pub fn parse_quiz(text: &str) -> Option<(String, Vec<String>)> {
    let blocks = split_blocks(text);
    let (question, options) = if blocks.len() < 2 {
        let lines = non_empty_lines(text);
        (lines.first()?.clone(), lines.get(1..)?.to_vec())
    } else {
        // Each line after the question is one option, so consecutive and
        // blank-line-separated options both parse (a Likert scale is a single
        // block whose options are one per line).
        let options = blocks[1..].iter().flat_map(|block| non_empty_lines(block)).collect();
        (blocks.first()?.clone(), options)
    };
    if options.is_empty() {
        return None;
    }
    Some((question, options))
}

/// The trimmed, non-empty lines of `text`, in order.
fn non_empty_lines(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect()
}

/// Group `text` into blocks separated by one or more blank lines, preserving
/// internal newlines within a block and trimming each block.
fn split_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(current.join("\n").trim().to_string());
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current.join("\n").trim().to_string());
    }
    blocks
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

    #[test]
    fn quiz_parsing_splits_on_blank_lines() {
        let text = "Which stakeholder is external?\n\n\nThe design team\n\n\nThe quality team\n\n\nThe development team";
        let (question, options) = parse_quiz(text).expect("a quiz");
        assert_eq!(question, "Which stakeholder is external?");
        assert_eq!(options, ["The design team", "The quality team", "The development team"]);
    }

    #[test]
    fn quiz_parsing_falls_back_to_single_newlines() {
        let text = "All taxes should be abolished.\nStrongly Agree\nAgree\nNeutral / Not Sure\nDisagree\nStrongly Disagree";
        let (question, options) = parse_quiz(text).expect("a quiz");
        assert_eq!(question, "All taxes should be abolished.");
        assert_eq!(
            options,
            ["Strongly Agree", "Agree", "Neutral / Not Sure", "Disagree", "Strongly Disagree"]
        );
    }

    #[test]
    fn quiz_parsing_keeps_a_wrapped_question() {
        let text = "A wrapped question\nthat continues here\n\nOption one\n\nOption two";
        let (question, options) = parse_quiz(text).expect("a quiz");
        assert_eq!(question, "A wrapped question\nthat continues here");
        assert_eq!(options, ["Option one", "Option two"]);
    }

    #[test]
    fn quiz_parsing_splits_consecutive_option_lines_in_one_block() {
        let text = "Publicly owned research institutions, such as space programs, should receive more public funding.\n\nStrongly Agree\nAgree\nNeutral / Not Sure\nDisagree\nStrongly Disagree";
        let (question, options) = parse_quiz(text).expect("a quiz");
        assert_eq!(
            question,
            "Publicly owned research institutions, such as space programs, should receive more public funding."
        );
        assert_eq!(
            options,
            ["Strongly Agree", "Agree", "Neutral / Not Sure", "Disagree", "Strongly Disagree"]
        );
    }

    #[test]
    fn quiz_parsing_rejects_a_single_line_or_empty_text() {
        assert_eq!(parse_quiz("just one line"), None);
        assert_eq!(parse_quiz("  \n \n"), None);
    }
}
