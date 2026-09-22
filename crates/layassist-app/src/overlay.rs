//! The full-screen overlay: capture items, run a decision, show results.
//!
//! Per ADR-14/25 the layer is transparent and **click-through** (the pointer
//! passes to the app underneath) while it keeps **keyboard interactivity**
//! (ADR-26). Results are dismissed with a click (ADR-15), at which point the
//! host takes the pointer so the click is seen.
//!
//! The windowing/host (a Wayland layer-shell surface) lives in
//! `layassist-platform` (ADR-20); this type only draws widgets and holds the
//! capture/decision state. It is driven through
//! [`OverlayApp`](layassist_platform::overlay::OverlayApp).
//!
//! M3 has no platform selection resolver yet (M4): a text field stands in, and
//! its committed entries are fed through
//! [`StubResolver`](layassist_resolvers::StubResolver) exactly like a real
//! auto-captured selection. The field is auto-focused so it works without the
//! pointer.

use layassist_core::Session;
use layassist_platform::overlay::OverlayApp;
use layassist_resolvers::{StubResolver, TextResolver};

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
    resolver: StubResolver,
    worker: Worker,
    phase: Phase,
    entry: String,
    threshold: f32,
    exit: bool,
}

impl Overlay {
    /// Create the overlay driving `worker`.
    #[must_use]
    pub fn new(worker: Worker) -> Self {
        Self {
            session: Session::new(),
            resolver: StubResolver::new(),
            worker,
            phase: Phase::Capturing,
            entry: String::new(),
            threshold: DEFAULT_CONFIDENCE_THRESHOLD,
            exit: false,
        }
    }

    /// Drain resolved selections into the session (auto-capture, ADR-26).
    fn capture(&mut self) {
        while let Ok(Some(selection)) = self.resolver.resolve_current_selection() {
            self.session.push(selection);
        }
    }

    /// Queue the manual entry as a selection.
    ///
    /// This is the M3 stand-in for the platform selection resolver (M4).
    fn commit_entry(&mut self) {
        let text = self.entry.trim().to_string();
        if !text.is_empty() {
            self.resolver.queue(text);
            self.entry.clear();
            self.capture();
        }
    }

    /// Run a decision if enough has been captured and nothing is in flight.
    fn decide(&mut self) {
        if matches!(self.phase, Phase::Running) || !self.session.is_ready() {
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

    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(48.0);
            theme::shadowed_text(ui, "layassist", theme::FG0, theme::TITLE_SIZE);
            ui.add_space(8.0);
            match &self.phase {
                Phase::Capturing => {
                    if let Some(question) = self.session.question() {
                        theme::shadowed_text(
                            ui,
                            &format!("Question: {}", question.selection.text),
                            theme::BLUE,
                            theme::BODY_SIZE,
                        );
                    } else {
                        theme::shadowed_text(
                            ui,
                            "Select the question text…",
                            theme::FG4,
                            theme::BODY_SIZE,
                        );
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
                            .hint_text("type an item, then press Tab to capture (M3 stub)")
                            .desired_width(480.0),
                    );
                    response.request_focus();
                    theme::shadowed_text(
                        ui,
                        "Tab: capture item · Enter: decide · Esc: cancel",
                        theme::FG4,
                        theme::BODY_SIZE,
                    );
                }
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
        self.capture();

        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
            self.commit_entry();
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Enter)) {
            self.decide();
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
    use super::*;
    use crate::worker::DecisionEngine;
    use layassist_model::RankedAnswer;

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

    fn overlay() -> Overlay {
        Overlay::new(Worker::spawn(FirstWins))
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
    fn committing_entries_builds_a_session() {
        let mut overlay = overlay();
        overlay.entry = "Which planet?".to_string();
        overlay.commit_entry();
        overlay.entry = "Mars".to_string();
        overlay.commit_entry();
        assert_eq!(overlay.session.question_text(), Some("Which planet?"));
        assert_eq!(overlay.session.answer_texts(), ["Mars"]);
        assert!(overlay.entry.is_empty());
    }

    #[test]
    fn decide_without_answers_stays_capturing() {
        let mut overlay = overlay();
        overlay.entry = "question only".to_string();
        overlay.commit_entry();
        overlay.decide();
        assert!(matches!(overlay.phase, Phase::Capturing));
    }

    #[test]
    fn decide_then_poll_yields_results() {
        let mut overlay = overlay();
        for text in ["Which planet?", "Mars", "Venus"] {
            overlay.entry = text.to_string();
            overlay.commit_entry();
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
        let mut overlay = overlay();
        overlay.entry = "q".to_string();
        overlay.commit_entry();
        overlay.dismiss();
        assert!(overlay.session.is_empty());
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
        let mut overlay = Overlay::new(Worker::spawn(Broken));
        for text in ["q", "a"] {
            overlay.entry = text.to_string();
            overlay.commit_entry();
        }
        overlay.decide();
        wait_for_reply(&mut overlay);
        assert!(matches!(overlay.phase, Phase::Error(ref error) if error == "no model"));
    }
}
