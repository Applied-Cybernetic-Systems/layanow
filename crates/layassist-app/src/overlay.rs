//! The full-screen overlay: capture items, run a decision, show results.
//!
//! Per ADR-14/25 the layer is transparent and **click-through** (the pointer
//! passes to the app underneath) while it keeps **keyboard interactivity**
//! (ADR-26). Items are captured on **`Tab`**: the field text if the user typed
//! something, otherwise the current native **PRIMARY** highlight. The first
//! item is the question, every later one an answer (ADR-33). `Enter` decides,
//! `Esc` cancels (or quits when nothing is captured), and a click dismisses the
//! results (ADR-15).
//!
//! The windowing/host (a Wayland layer-shell surface) lives in
//! `layassist-platform` (ADR-20); this type only draws widgets and holds the
//! capture/decision state. It is driven through
//! [`OverlayApp`](layassist_platform::overlay::OverlayApp). Resolving the
//! selection is delegated to a
//! [`TextResolver`](layassist_resolvers::TextResolver) supplied by the platform
//! (`layassist_platform::selection`).

use layassist_core::{Selection, Session, Source};
use layassist_platform::overlay::OverlayApp;
use layassist_resolvers::TextResolver;

use crate::results::{self, DEFAULT_CONFIDENCE_THRESHOLD, Results};
use crate::theme;
use crate::worker::{Response, Worker};

/// The overlay's current phase.
#[derive(Debug)]
enum Phase {
    /// Capturing items; `Enter` runs the decision.
    Capturing,
    /// A decision is running on the worker thread.
    Running,
    /// Ranked results are displayed.
    Results(Box<Results>),
    /// The last decision failed; the string is user-facing.
    Error(String),
}

/// The egui application behind the overlay.
pub struct Overlay {
    session: Session,
    resolver: Box<dyn TextResolver>,
    worker: Worker,
    phase: Phase,
    threshold: f32,
    /// A short user-facing hint (e.g. "no selection") drawn while capturing.
    status: Option<String>,
    /// Freely typed item text, captured when `Tab` is pressed.
    entry: String,
    exit: bool,
}

impl Overlay {
    /// Create the overlay driving `worker` and reading selections from
    /// `resolver`.
    #[must_use]
    pub fn new(worker: Worker, resolver: Box<dyn TextResolver>) -> Self {
        Self {
            session: Session::new(),
            resolver,
            worker,
            phase: Phase::Capturing,
            threshold: DEFAULT_CONFIDENCE_THRESHOLD,
            status: None,
            entry: String::new(),
            exit: false,
        }
    }

    /// Capture one item on `Tab`: the typed entry if there is one, otherwise
    /// the current native (PRIMARY) selection (ADR-33).
    ///
    /// Typing stays available as a fallback when nothing is highlighted. A
    /// repeat of the most recent item is ignored so a double-tap does not add
    /// the same text twice.
    fn capture_item(&mut self) {
        let typed = self.entry.trim();
        Self::debug(&format!("capture_item entry_len={}", typed.len()));
        if !typed.is_empty() {
            // v1 has no dedicated `Source` for typed text; it is treated as a
            // manual selection.
            let selection =
                Selection { text: typed.to_string(), source: Source::Selection, bounds: None };
            self.entry.clear();
            self.push(selection);
            return;
        }
        match self.resolver.resolve_current_selection() {
            Ok(Some(selection)) => {
                Self::debug(&format!("captured highlight_len={}", selection.text.len()));
                self.push(selection);
            }
            Ok(None) => {
                Self::debug("no highlight found");
                self.status = Some(
                    "no highlight found — the app may not publish it (type instead)".to_string(),
                );
            }
            Err(error) => {
                Self::debug(&format!("resolver error: {error}"));
                self.status = Some(format!("selection error: {error}"));
            }
        }
    }

    /// Print a capture diagnostic when `LAYASSIST_DEBUG` is set.
    ///
    /// Only lengths are logged, never the selected text.
    fn debug(message: &str) {
        if std::env::var_os("LAYASSIST_DEBUG").is_some() {
            eprintln!("layassist: {message}");
        }
    }

    /// Append `selection`, ignoring a repeat of the most recent item.
    fn push(&mut self, selection: Selection) {
        let repeated =
            self.session.items().last().is_some_and(|item| item.selection.text == selection.text);
        if repeated {
            self.status = Some("already captured".to_string());
        } else {
            self.session.push(selection);
            self.status = None;
        }
    }

    /// Run a decision if enough has been captured and nothing is in flight.
    fn decide(&mut self) {
        if matches!(self.phase, Phase::Running) {
            return;
        }
        if !self.session.is_ready() {
            self.status = Some("capture a question and at least one answer first".to_string());
            return;
        }
        let Some(question) = self.session.question_text().map(str::to_string) else {
            return;
        };
        let answers = self.session.answer_texts();
        self.phase = match self.worker.decide(question, answers) {
            Ok(()) => Phase::Running,
            Err(error) => Phase::Error(error.to_string()),
        };
    }

    /// Take the worker's reply, if any.
    fn poll(&mut self) {
        if let Some(response) = self.worker.try_recv() {
            self.phase = match response {
                Response::Ranked(ranked) => {
                    Phase::Results(Box::new(results::results(&ranked, self.threshold)))
                }
                Response::Failed(error) => Phase::Error(error),
            };
        }
    }

    /// Clear everything and return to capturing (`Esc`, or a click on results).
    fn dismiss(&mut self) {
        self.phase = Phase::Capturing;
        self.session.clear();
        self.status = None;
        self.entry.clear();
    }

    fn draw_results(ui: &mut egui::Ui, panel: &Results) {
        for row in &panel.rows {
            ui.horizontal(|ui| {
                theme::shadowed_text(ui, &row.label, theme::FG0, theme::BODY_SIZE);
                ui.add(
                    egui::ProgressBar::new(row.probability)
                        .desired_width(260.0)
                        .fill(egui::Color32::from_rgb(row.colour[0], row.colour[1], row.colour[2]))
                        .text(format!("{:>5.1}%", row.probability * 100.0)),
                );
                let color = if row.is_top { theme::FG0 } else { theme::FG1 };
                theme::shadowed_text(ui, &row.text, color, theme::BODY_SIZE);
            });
        }
        if panel.low_confidence {
            theme::shadowed_text(
                ui,
                &format!("low confidence ({:.0}%)", panel.confidence * 100.0),
                theme::YELLOW,
                theme::BODY_SIZE,
            );
        }
    }

    fn draw_capturing(&mut self, ui: &mut egui::Ui) {
        if let Some(question) = self.session.question() {
            theme::shadowed_text(
                ui,
                &format!("Question: {}", question.selection.text),
                theme::BLUE,
                theme::BODY_SIZE,
            );
        } else {
            theme::shadowed_text(ui, "Highlight the question text…", theme::FG4, theme::BODY_SIZE);
        }
        for (index, answer) in self.session.answers().iter().enumerate() {
            theme::shadowed_text(
                ui,
                &format!("Answer {}: {}", index + 1, answer.selection.text),
                theme::GREEN,
                theme::BODY_SIZE,
            );
        }
        ui.add_space(12.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.entry)
                .hint_text("type an item — or highlight text — then press Tab")
                .desired_width(480.0),
        );
        response.request_focus();
        if let Some(status) = &self.status {
            theme::shadowed_text(ui, status, theme::YELLOW, theme::BODY_SIZE);
        }
        theme::shadowed_text(
            ui,
            "Tab: add · Enter: decide · Esc: cancel/quit",
            theme::FG4,
            theme::BODY_SIZE,
        );
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(48.0);
            theme::shadowed_text(ui, "layassist", theme::FG0, theme::TITLE_SIZE);
            ui.add_space(8.0);
            if matches!(self.phase, Phase::Capturing) {
                self.draw_capturing(ui);
                return;
            }
            match &self.phase {
                Phase::Capturing => {}
                Phase::Running => {
                    ui.spinner();
                    theme::shadowed_text(ui, "Deciding…", theme::FG1, theme::BODY_SIZE);
                }
                Phase::Results(panel) => {
                    Self::draw_results(ui, panel);
                    ui.add_space(8.0);
                    theme::shadowed_text(ui, "click or Esc: dismiss", theme::FG4, theme::BODY_SIZE);
                }
                Phase::Error(error) => {
                    theme::shadowed_text(
                        ui,
                        &format!("error: {error}"),
                        theme::RED,
                        theme::BODY_SIZE,
                    );
                    theme::shadowed_text(ui, "Esc: dismiss", theme::FG4, theme::BODY_SIZE);
                }
            }
        });
    }
}

impl OverlayApp for Overlay {
    fn configure(&mut self, ctx: &egui::Context) {
        theme::install(ctx);
    }

    fn update(&mut self, ctx: &egui::Context) {
        self.poll();

        if matches!(self.phase, Phase::Capturing) {
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                self.capture_item();
            }
            if ctx.input(|input| input.key_pressed(egui::Key::Enter)) {
                self.decide();
            }
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            // `Esc` on an empty session quits the applet; otherwise it cancels.
            if matches!(self.phase, Phase::Capturing) && self.session.is_empty() {
                self.exit = true;
            } else {
                self.dismiss();
            }
        }
        if matches!(self.phase, Phase::Results(_)) && ctx.input(|input| input.pointer.any_click()) {
            self.dismiss();
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::OVERLAY_BG))
            .show(ctx, |ui| self.draw(ui));
    }

    fn wants_pointer(&self) -> bool {
        // The pointer is captured only to receive the dismissing click (ADR-15).
        matches!(self.phase, Phase::Results(_))
    }

    fn should_exit(&self) -> bool {
        self.exit
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::worker::DecisionEngine;
    use layassist_core::{Selection, Source};
    use layassist_model::RankedAnswer;
    use layassist_resolvers::{ResolveError, SelectionStream};

    /// An engine that ranks the first answer highest.
    struct FirstWins;

    impl DecisionEngine for FirstWins {
        fn decide_choice(
            &mut self,
            _question: &str,
            answers: &[String],
        ) -> Result<Vec<RankedAnswer>, String> {
            let mut ranked: Vec<RankedAnswer> = answers
                .iter()
                .enumerate()
                .map(|(index, text)| RankedAnswer {
                    index,
                    text: text.clone(),
                    probability: if index == 0 { 0.8 } else { 0.2 },
                    confidence: 0.6,
                })
                .collect();
            ranked.sort_by(|a, b| b.probability.total_cmp(&a.probability));
            Ok(ranked)
        }
    }

    /// A resolver whose "current selection" the test sets directly.
    #[derive(Clone, Default)]
    struct TestResolver {
        current: Arc<Mutex<Option<Selection>>>,
    }

    impl TestResolver {
        fn set(&self, text: &str) {
            *self.current.lock().expect("test lock") =
                Some(Selection { text: text.to_string(), source: Source::Selection, bounds: None });
        }
    }

    impl TextResolver for TestResolver {
        fn name(&self) -> &'static str {
            "test"
        }

        fn available(&self) -> bool {
            true
        }

        fn resolve_current_selection(&self) -> Result<Option<Selection>, ResolveError> {
            Ok(self.current.lock().expect("test lock").clone())
        }

        fn watch(&self) -> Option<SelectionStream> {
            None
        }
    }

    fn overlay_with(engine: impl DecisionEngine) -> (Overlay, TestResolver) {
        let resolver = TestResolver::default();
        let overlay = Overlay::new(Worker::spawn(engine), Box::new(resolver.clone()));
        (overlay, resolver)
    }

    fn overlay() -> (Overlay, TestResolver) {
        overlay_with(FirstWins)
    }

    /// Poll until the worker replies, mirroring the per-frame `update` loop.
    fn wait_for_reply(overlay: &mut Overlay) {
        for _ in 0..1000 {
            overlay.poll();
            if !matches!(overlay.phase, Phase::Running) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("the worker did not reply");
    }

    #[test]
    fn highlight_captures_build_a_session() {
        let (mut overlay, resolver) = overlay();
        resolver.set("Which planet?");
        overlay.capture_item();
        resolver.set("Mars");
        overlay.capture_item();
        assert_eq!(overlay.session.question_text(), Some("Which planet?"));
        assert_eq!(overlay.session.answer_texts(), ["Mars"]);
        assert!(overlay.status.is_none());
    }

    #[test]
    fn typed_entry_is_captured_and_cleared() {
        let (mut overlay, _resolver) = overlay();
        overlay.entry = "Which planet?".to_string();
        overlay.capture_item();
        assert_eq!(overlay.session.question_text(), Some("Which planet?"));
        assert!(overlay.entry.is_empty());
        assert!(overlay.status.is_none());
    }

    #[test]
    fn typed_entry_takes_precedence_over_a_highlight() {
        let (mut overlay, resolver) = overlay();
        resolver.set("native highlight");
        overlay.entry = "typed".to_string();
        overlay.capture_item();
        assert_eq!(overlay.session.question_text(), Some("typed"));
    }

    #[test]
    fn repeated_selection_is_only_captured_once() {
        let (mut overlay, resolver) = overlay();
        resolver.set("Which planet?");
        overlay.capture_item();
        overlay.capture_item();
        assert_eq!(overlay.session.len(), 1);
        assert!(overlay.status.is_some());
    }

    #[test]
    fn nothing_to_add_sets_a_status_and_captures_nothing() {
        let (mut overlay, _resolver) = overlay();
        overlay.capture_item();
        assert!(overlay.session.is_empty());
        assert!(overlay.status.is_some());
    }

    #[test]
    fn decide_without_answers_stays_capturing() {
        let (mut overlay, resolver) = overlay();
        resolver.set("question only");
        overlay.capture_item();
        overlay.decide();
        assert!(matches!(overlay.phase, Phase::Capturing));
    }

    #[test]
    fn decide_then_poll_yields_results() {
        let (mut overlay, resolver) = overlay();
        for text in ["Which planet?", "Mars", "Venus"] {
            resolver.set(text);
            overlay.capture_item();
        }
        overlay.decide();
        assert!(matches!(overlay.phase, Phase::Running));

        wait_for_reply(&mut overlay);
        let Phase::Results(panel) = &overlay.phase else {
            panic!("expected results");
        };
        assert_eq!(panel.rows[0].text, "Mars");
        assert!(panel.rows[0].is_top);
    }

    #[test]
    fn dismiss_clears_the_session() {
        let (mut overlay, resolver) = overlay();
        resolver.set("q");
        overlay.capture_item();
        overlay.entry = "uncommitted".to_string();
        overlay.dismiss();
        assert!(overlay.session.is_empty());
        assert!(overlay.status.is_none());
        assert!(overlay.entry.is_empty());
        assert!(matches!(overlay.phase, Phase::Capturing));
    }

    #[test]
    fn a_failing_engine_becomes_an_error_phase() {
        struct Broken;
        impl DecisionEngine for Broken {
            fn decide_choice(
                &mut self,
                _q: &str,
                _a: &[String],
            ) -> Result<Vec<RankedAnswer>, String> {
                Err("no model".to_string())
            }
        }
        let (mut overlay, resolver) = overlay_with(Broken);
        for text in ["q", "a"] {
            resolver.set(text);
            overlay.capture_item();
        }
        overlay.decide();
        wait_for_reply(&mut overlay);
        assert!(matches!(overlay.phase, Phase::Error(ref error) if error == "no model"));
    }
}
