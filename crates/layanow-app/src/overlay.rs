//! The full-screen overlay: capture items, run a decision, show results.
//!
//! The overlay starts **hidden** and the applet is resident (ADR-34):
//! `layanow toggle` shows or hides it through the control socket
//! ([`layanow_platform::control`]), and it only takes keyboard interactivity
//! (ADR-26) while shown. Hiding clears the session.
//!
//! While shown the layer is transparent and **click-through** (the pointer
//! passes to the app underneath, ADR-14/25). Items are captured on **`Tab`**:
//! the field text if the user typed something, otherwise the current native
//! **PRIMARY** highlight. The first item is the question, every later one an
//! answer (ADR-33). `Enter` decides, `Esc` hides the overlay, and a click
//! dismisses the results (ADR-15).
//!
//! The windowing/host (a Wayland layer-shell surface) lives in
//! `layanow-platform` (ADR-20); this type only draws widgets and holds the
//! capture/decision state. It is driven through
//! [`OverlayApp`](layanow_platform::overlay::OverlayApp). Resolving the
//! selection is delegated to a
//! [`TextResolver`](layanow_resolvers::TextResolver) supplied by the platform
//! (`layanow_platform::selection`).

use crossbeam_channel::Receiver;

use layanow_core::{Selection, Session, Source};
use layanow_platform::control::Command;
use layanow_platform::overlay::OverlayApp;
use layanow_resolvers::TextResolver;

use crate::results::{self, DEFAULT_CONFIDENCE_THRESHOLD, Palette, Results};
use crate::settings::{NamedContext, Settings, UnloadPolicy};
use crate::theme;
use crate::worker::{Response, Worker};

/// Maximum context text kept from a selected file, in characters. The renderer
/// truncates the state to the model's token budget anyway; this only bounds
/// memory (ADR-40).
const MAX_CONTEXT_CHARS: usize = 32_768;

/// Left inset for the results block so it does not sit flush against the screen
/// edge.
const RESULTS_LEFT_MARGIN: f32 = 160.0;

/// Distance from the top of the screen to the UI panel.
const PANEL_TOP_MARGIN: f32 = 48.0;

/// Padding inside the panel's translucent backdrop.
const PANEL_PADDING: i8 = 16;

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
    commands: Receiver<Command>,
    phase: Phase,
    threshold: f32,
    /// Probability-bar colour anchors from settings (T-116).
    palette: Palette,
    /// A short user-facing hint (e.g. "no selection") drawn while capturing.
    status: Option<String>,
    /// Freely typed item text, captured when `Tab` is pressed.
    entry: String,
    /// The optional context/evidence text, sent as the model's `state` (ADR-40).
    context: String,
    /// Saved context templates loaded from settings (ADR-40).
    contexts: Vec<NamedContext>,
    /// The template name being typed in the Save field, when open.
    saving_name: Option<String>,
    /// The template currently chosen in the dropdown.
    selected_template: Option<String>,
    /// The id assigned to the next decision.
    next_request: u64,
    /// The id of the decision currently in flight, if any. A reply whose id
    /// does not match is stale (the decision it belongs to was dismissed) and
    /// is dropped (T-167).
    pending_request: Option<u64>,
    /// The panel's screen rectangle from the last frame, used to keep only the
    /// panel interactive (ADR-14, T-166).
    panel_rect: Option<egui::Rect>,
    /// Whether the overlay is shown; starts hidden (ADR-34).
    visible: bool,
    exit: bool,
}

impl Overlay {
    /// Create the overlay driving `worker`, reading selections from `resolver`,
    /// and handling control commands from `commands`.
    #[must_use]
    pub fn new(
        worker: Worker,
        resolver: Box<dyn TextResolver>,
        commands: Receiver<Command>,
    ) -> Self {
        Self {
            session: Session::new(),
            resolver,
            worker,
            commands,
            phase: Phase::Capturing,
            threshold: DEFAULT_CONFIDENCE_THRESHOLD,
            palette: Palette::default(),
            status: None,
            entry: String::new(),
            context: String::new(),
            contexts: Vec::new(),
            saving_name: None,
            selected_template: None,
            next_request: 0,
            pending_request: None,
            panel_rect: None,
            visible: false,
            exit: false,
        }
    }

    /// Override the low-confidence threshold (from settings, T-115).
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold;
    }

    /// Re-read the settings file and apply it live (T-113/T-115/T-117): the
    /// threshold changes immediately, and the worker is reconfigured for the
    /// checkpoint and unload policy.
    fn apply_settings(&mut self) {
        let settings = Settings::load();
        self.threshold = settings.confidence_threshold;
        self.palette = settings.palette;
        self.contexts = settings.contexts;
        let on_demand = settings.unload == UnloadPolicy::OnDemand;
        if let Err(error) = self.worker.configure(settings.checkpoint, settings.quant, on_demand) {
            tracing::warn!(%error, "could not apply settings");
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
        tracing::debug!(entry_len = typed.len(), "capture item");
        if !typed.is_empty() {
            // Typed text was not read from the screen; record that provenance
            // so it can be told apart from a native highlight.
            let text = typed.to_string();
            self.entry.clear();
            if self.try_capture_files(&text) {
                return;
            }
            let selection = Selection { text, source: Source::Manual, bounds: None };
            self.push(selection);
            return;
        }
        match self.resolver.resolve_current_selection() {
            Ok(Some(selection)) => {
                if self.try_capture_files(&selection.text) {
                    return;
                }
                tracing::debug!(highlight_len = selection.text.len(), "captured highlight");
                self.push(selection);
            }
            Ok(None) => {
                tracing::debug!("no highlight found");
                self.status = Some(
                    "no highlight found — the app may not publish it (type instead)".to_string(),
                );
            }
            Err(error) => {
                tracing::warn!(%error, "selection resolver failed");
                self.status = Some(format!("selection error: {error}"));
            }
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

    /// If `text` names files (a file-manager `file://` URI, or an existing
    /// absolute path), read them into the context and return `true` (ADR-40).
    ///
    /// A file selection is never a question or an answer, so it is routed to
    /// the context regardless of what the user was about to capture.
    fn try_capture_files(&mut self, text: &str) -> bool {
        let paths = file_paths(text);
        if paths.is_empty() {
            return false;
        }
        let mut contents = String::new();
        let mut read = 0_usize;
        for path in &paths {
            match std::fs::read_to_string(path) {
                Ok(file) => {
                    if !contents.is_empty() {
                        contents.push_str("\n\n");
                    }
                    contents.push_str(&file);
                    read += 1;
                }
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "could not read context file");
                }
            }
        }
        if read == 0 {
            self.status = Some("could not read the selected file(s)".to_string());
            return true;
        }
        self.context = truncate_chars(&contents, MAX_CONTEXT_CHARS);
        self.selected_template = None;
        self.status = Some(format!("context loaded from {read} file(s)"));
        true
    }

    /// Save the current context as a named template (ADR-40), replacing an
    /// existing template of the same name.
    fn save_template(&mut self) {
        let Some(name) = self.saving_name.take() else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            self.status = Some("a template needs a name".to_string());
            return;
        }
        let mut settings = Settings::load();
        if let Some(existing) = settings.contexts.iter_mut().find(|template| template.name == name)
        {
            existing.text.clone_from(&self.context);
        } else {
            settings.contexts.push(NamedContext { name: name.clone(), text: self.context.clone() });
        }
        match settings.save() {
            Ok(()) => {
                self.contexts = settings.contexts;
                self.selected_template = Some(name.clone());
                self.status = Some(format!("saved template {name:?}"));
            }
            Err(error) => self.status = Some(format!("could not save template: {error}")),
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
        // Each decision gets a fresh id so a reply for an abandoned decision
        // cannot be mistaken for the current one (T-167).
        let id = self.next_request;
        self.next_request = self.next_request.wrapping_add(1);
        self.phase = match self.worker.decide(id, self.context.clone(), question, answers) {
            Ok(()) => {
                self.pending_request = Some(id);
                Phase::Running
            }
            Err(error) => {
                self.pending_request = None;
                Phase::Error(error.to_string())
            }
        };
    }

    /// Take the worker's reply, if any.
    ///
    /// A reply is accepted only if its id matches the decision currently in
    /// flight. Replies for a decision that was abandoned (e.g. the overlay was
    /// hidden mid-decision) no longer match and are dropped, so stale results
    /// cannot reappear (T-167).
    fn poll_worker(&mut self) {
        while let Some(response) = self.worker.try_recv() {
            let id = match &response {
                Response::Ranked { id, .. } | Response::Failed { id, .. } => *id,
            };
            if Some(id) != self.pending_request {
                continue;
            }
            self.pending_request = None;
            self.phase = match response {
                Response::Ranked { ranked, .. } => Phase::Results(Box::new(results::results(
                    &ranked,
                    self.threshold,
                    &self.palette,
                ))),
                Response::Failed { error, .. } => Phase::Error(error),
            };
        }
    }

    /// Clear everything and return to capturing (`Esc`, or a click on results).
    fn dismiss(&mut self) {
        self.phase = Phase::Capturing;
        self.pending_request = None;
        self.session.clear();
        self.status = None;
        self.entry.clear();
        self.context.clear();
        self.saving_name = None;
        self.selected_template = None;
    }

    /// Show or hide the overlay, clearing the session on any change.
    ///
    /// A hidden applet must not retain captured text, and each showing starts
    /// fresh (ADR-34). Showing also refreshes the saved context templates, in
    /// case they changed while hidden (ADR-40).
    fn set_visible(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if visible {
            self.contexts = Settings::load().contexts;
        }
        self.dismiss();
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
        // Context (state) box with a template dropdown and Save (ADR-40).
        let mut selected = self.selected_template.clone();
        let mut save_clicked = false;
        ui.horizontal(|ui| {
            theme::shadowed_text(ui, "Context:", theme::FG4, theme::BODY_SIZE);
            ui.add(
                egui::TextEdit::singleline(&mut self.context)
                    .hint_text("optional — type/paste, or select a file")
                    .desired_width(280.0),
            );
            egui::ComboBox::from_id_salt("context-template")
                .selected_text(selected.as_deref().unwrap_or("templates"))
                .show_ui(ui, |ui| {
                    for template in &self.contexts {
                        ui.selectable_value(
                            &mut selected,
                            Some(template.name.clone()),
                            &template.name,
                        );
                    }
                });
            save_clicked = ui.button("Save…").clicked();
        });
        if selected != self.selected_template {
            self.selected_template = selected;
            if let Some(name) = &self.selected_template {
                if let Some(template) = self.contexts.iter().find(|template| &template.name == name)
                {
                    self.context = template.text.clone();
                }
            }
        }
        if save_clicked {
            self.saving_name = Some(String::new());
        }
        let mut confirm = false;
        let mut cancel = false;
        if self.saving_name.is_some() {
            ui.horizontal(|ui| {
                theme::shadowed_text(ui, "Name:", theme::FG4, theme::BODY_SIZE);
                if let Some(name) = &mut self.saving_name {
                    ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                }
                confirm = ui.button("Save").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        }
        if confirm {
            self.save_template();
        } else if cancel {
            self.saving_name = None;
        }

        ui.add_space(6.0);
        if let Some(question) = self.session.question() {
            theme::shadowed_labelled_text(
                ui,
                "Question: ",
                theme::FG4,
                &question.selection.text,
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
        // Only claim focus when nothing else has it, so the context box stays
        // editable after a click.
        if ui.ctx().memory(|memory| memory.focused().is_none()) {
            response.request_focus();
        }
        if let Some(status) = &self.status {
            theme::shadowed_text(ui, status, theme::YELLOW, theme::BODY_SIZE);
        }
        theme::shadowed_text(
            ui,
            "Tab: add · Enter: decide · Esc: hide",
            theme::FG4,
            theme::BODY_SIZE,
        );
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            theme::shadowed_text(ui, "layanow", theme::FG0, theme::TITLE_SIZE);
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
                    let question = self.session.question_text().map(str::to_string);
                    ui.horizontal(|ui| {
                        ui.add_space(RESULTS_LEFT_MARGIN);
                        ui.vertical(|ui| {
                            if let Some(question) = &question {
                                theme::shadowed_labelled_text(
                                    ui,
                                    "Question: ",
                                    theme::FG4,
                                    question,
                                    theme::BLUE,
                                    theme::BODY_SIZE,
                                );
                                ui.add_space(6.0);
                            }
                            Self::draw_results(ui, panel);
                            ui.add_space(8.0);
                            theme::shadowed_text(
                                ui,
                                "click: dismiss · Esc: hide",
                                theme::FG4,
                                theme::BODY_SIZE,
                            );
                        });
                    });
                }
                Phase::Error(error) => {
                    theme::shadowed_text(
                        ui,
                        &format!("error: {error}"),
                        theme::RED,
                        theme::BODY_SIZE,
                    );
                    theme::shadowed_text(ui, "Esc: hide", theme::FG4, theme::BODY_SIZE);
                }
            }
        });
    }
}

impl OverlayApp for Overlay {
    fn configure(&mut self, ctx: &egui::Context) {
        theme::install(ctx);
    }

    fn poll(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Toggle => self.set_visible(!self.visible),
                Command::Show => self.set_visible(true),
                Command::Hide => self.set_visible(false),
                Command::Quit => self.exit = true,
                Command::Reload => self.apply_settings(),
            }
        }
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn update(&mut self, ctx: &egui::Context) {
        self.poll_worker();

        if matches!(self.phase, Phase::Capturing) {
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                self.capture_item();
            }
            if ctx.input(|input| input.key_pressed(egui::Key::Enter)) {
                self.decide();
            }
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            // The applet is resident: `Esc` hides the overlay instead of
            // quitting (quit through `layanow quit`, or the tray later).
            self.set_visible(false);
        }
        if matches!(self.phase, Phase::Results(_)) && ctx.input(|input| input.pointer.any_click()) {
            self.dismiss();
        }

        let area = egui::Area::new(egui::Id::new("layanow-panel"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, PANEL_TOP_MARGIN))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::OVERLAY_BG)
                    .inner_margin(egui::Margin::same(PANEL_PADDING))
                    .corner_radius(egui::CornerRadius::same(8))
                    .show(ui, |ui| self.draw(ui));
            });
        // The panel rectangle feeds the host's input region next frame, so only
        // the panel is clickable and the rest stays click-through (T-166).
        self.panel_rect = Some(area.response.rect);
    }

    fn interactive_rect(&self) -> Option<egui::Rect> {
        // The panel is interactive in every phase so the context box, template
        // dropdown and Save button work (ADR-40); everything outside it stays
        // click-through (ADR-14).
        self.panel_rect
    }

    fn should_exit(&self) -> bool {
        self.exit
    }
}

/// The file paths named by `text`: `file://` URIs, or existing absolute paths.
fn file_paths(text: &str) -> Vec<std::path::PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            if let Some(path) = file_uri_to_path(line) {
                return Some(path);
            }
            let path = std::path::Path::new(line);
            path.is_file().then(|| path.to_path_buf())
        })
        .collect()
}

/// Convert a `file://` URI to a path, dropping any host and percent-decoding.
fn file_uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = if let Some(without_slash) = rest.strip_prefix('/') {
        // Empty host: `file:///path` -> `/path`.
        format!("/{without_slash}")
    } else {
        // `file://host/path` -> `/path`.
        let index = rest.find('/')?;
        rest.get(index..)?.to_string()
    };
    Some(std::path::PathBuf::from(percent_decode(&path)))
}

/// Minimal percent-decoding for `file://` URIs.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_digit(bytes[index + 1]), hex_digit(bytes[index + 2]))
            {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Truncate `text` to at most `max` characters (not bytes).
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::worker::DecisionEngine;
    use layanow_core::{Selection, Source};
    use layanow_model::RankedAnswer;
    use layanow_resolvers::{ResolveError, TextResolver};

    /// An engine that ranks the first answer highest.
    struct FirstWins;

    impl DecisionEngine for FirstWins {
        fn decide_choice(
            &mut self,
            _context: &str,
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
    }

    /// An engine that blocks each decision until the test releases it, so
    /// replies can be forced to arrive after the overlay moved on.
    struct Gated {
        started: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    }

    impl DecisionEngine for Gated {
        fn decide_choice(
            &mut self,
            _context: &str,
            _question: &str,
            answers: &[String],
        ) -> Result<Vec<RankedAnswer>, String> {
            self.started.send(()).expect("started");
            self.release.recv().expect("release");
            Ok(answers
                .iter()
                .enumerate()
                .map(|(index, text)| RankedAnswer {
                    index,
                    text: text.clone(),
                    probability: if index == 0 { 0.9 } else { 0.1 },
                    confidence: 0.5,
                })
                .collect())
        }
    }

    fn overlay_with(engine: impl DecisionEngine) -> (Overlay, TestResolver) {
        let resolver = TestResolver::default();
        let (_commands_tx, commands) = crossbeam_channel::unbounded();
        let overlay =
            Overlay::new(Worker::spawn_fixed(engine), Box::new(resolver.clone()), commands);
        (overlay, resolver)
    }

    /// An overlay controlled through its command channel.
    fn controlled() -> (Overlay, crossbeam_channel::Sender<Command>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let overlay =
            Overlay::new(Worker::spawn_fixed(FirstWins), Box::new(TestResolver::default()), rx);
        (overlay, tx)
    }

    fn overlay() -> (Overlay, TestResolver) {
        overlay_with(FirstWins)
    }

    #[test]
    fn overlay_starts_hidden() {
        let (overlay, _commands) = controlled();
        assert!(!overlay.visible());
    }

    #[test]
    fn toggle_command_flips_visibility() {
        let (mut overlay, commands) = controlled();
        commands.send(Command::Toggle).expect("send");
        overlay.poll();
        assert!(overlay.visible());
        commands.send(Command::Toggle).expect("send");
        overlay.poll();
        assert!(!overlay.visible());
    }

    #[test]
    fn quit_command_requests_exit() {
        let (mut overlay, commands) = controlled();
        commands.send(Command::Quit).expect("send");
        overlay.poll();
        assert!(overlay.exit);
    }

    #[test]
    fn hiding_clears_the_session() {
        let (mut overlay, commands) = controlled();
        overlay.entry = "Which planet?".to_string();
        overlay.capture_item();
        commands.send(Command::Show).expect("send");
        overlay.poll();
        assert!(overlay.visible());
        commands.send(Command::Hide).expect("send");
        overlay.poll();
        assert!(!overlay.visible());
        assert!(overlay.session.is_empty());
    }

    /// Poll until the worker replies, mirroring the per-frame `update` loop.
    fn wait_for_reply(overlay: &mut Overlay) {
        for _ in 0..1000 {
            overlay.poll_worker();
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
    fn typed_entry_is_recorded_as_manual() {
        let (mut overlay, _resolver) = overlay();
        overlay.entry = "Which planet?".to_string();
        overlay.capture_item();
        let source = overlay.session.question().map(|item| item.selection.source);
        assert_eq!(source, Some(Source::Manual));
    }

    #[test]
    fn highlight_is_recorded_as_a_selection() {
        let (mut overlay, resolver) = overlay();
        resolver.set("Which planet?");
        overlay.capture_item();
        let source = overlay.session.question().map(|item| item.selection.source);
        assert_eq!(source, Some(Source::Selection));
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
    fn a_stale_reply_is_not_consumed_as_the_current_decision() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let resolver = TestResolver::default();
        let (_commands_tx, commands) = crossbeam_channel::unbounded();
        let mut overlay = Overlay::new(
            Worker::spawn_fixed(Gated { started: started_tx, release: release_rx }),
            Box::new(resolver.clone()),
            commands,
        );

        // First decision (id 0) blocks in the engine.
        for text in ["q1", "a1"] {
            resolver.set(text);
            overlay.capture_item();
        }
        overlay.decide();
        started_rx.recv().expect("first decision started");

        // Abandon it, then run a second decision (id 1).
        overlay.dismiss();
        for text in ["q2", "a2"] {
            resolver.set(text);
            overlay.capture_item();
        }
        overlay.decide();
        assert!(matches!(overlay.phase, Phase::Running));

        // Releasing the first decision lets the worker send its reply and then
        // start the second; once the second start is observed, the stale reply
        // is already queued.
        release_tx.send(()).expect("release first");
        started_rx.recv().expect("second decision started");
        overlay.poll_worker();
        assert!(
            matches!(overlay.phase, Phase::Running),
            "the abandoned decision's reply must be dropped"
        );

        release_tx.send(()).expect("release second");
        wait_for_reply(&mut overlay);
        let Phase::Results(panel) = &overlay.phase else {
            panic!("expected results");
        };
        assert_eq!(panel.rows[0].text, "a2", "results must be for the current decision");
    }

    #[test]
    fn a_failing_engine_becomes_an_error_phase() {
        struct Broken;
        impl DecisionEngine for Broken {
            fn decide_choice(
                &mut self,
                _context: &str,
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

    #[test]
    fn file_uris_are_parsed_and_decoded() {
        assert_eq!(
            file_uri_to_path("file:///tmp/a%20b.txt"),
            Some(std::path::PathBuf::from("/tmp/a b.txt"))
        );
        assert_eq!(
            file_uri_to_path("file://localhost/tmp/c.txt"),
            Some(std::path::PathBuf::from("/tmp/c.txt"))
        );
        assert_eq!(file_uri_to_path("https://example.com"), None);
    }

    #[test]
    fn truncation_keeps_character_boundaries() {
        assert_eq!(truncate_chars("héllo", 3), "hél");
        assert_eq!(truncate_chars("hi", 10), "hi");
    }

    #[test]
    fn a_file_selection_becomes_context() {
        let dir = std::env::temp_dir().join(format!("layanow-ctx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("passage.txt");
        std::fs::write(&path, "Mars is the Red Planet.").unwrap();

        let (mut overlay, resolver) = overlay();
        resolver.set(&format!("file://{}", path.display()));
        overlay.capture_item();

        assert!(overlay.session.is_empty(), "a file is context, not a question");
        assert_eq!(overlay.context, "Mars is the Red Planet.");
        assert!(overlay.status.is_some());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn the_context_reaches_the_engine() {
        struct RecordsContext {
            seen: Arc<Mutex<Option<String>>>,
        }
        impl DecisionEngine for RecordsContext {
            fn decide_choice(
                &mut self,
                context: &str,
                _question: &str,
                answers: &[String],
            ) -> Result<Vec<RankedAnswer>, String> {
                *self.seen.lock().expect("lock") = Some(context.to_string());
                Ok(answers
                    .iter()
                    .enumerate()
                    .map(|(index, text)| RankedAnswer {
                        index,
                        text: text.clone(),
                        probability: 1.0,
                        confidence: 1.0,
                    })
                    .collect())
            }
        }

        let seen = Arc::new(Mutex::new(None));
        let resolver = TestResolver::default();
        let (_commands_tx, commands) = crossbeam_channel::unbounded();
        let mut overlay = Overlay::new(
            Worker::spawn_fixed(RecordsContext { seen: Arc::clone(&seen) }),
            Box::new(resolver.clone()),
            commands,
        );
        overlay.context = "the evidence".to_string();
        for text in ["q", "a"] {
            resolver.set(text);
            overlay.capture_item();
        }
        overlay.decide();
        wait_for_reply(&mut overlay);
        assert_eq!(seen.lock().expect("lock").as_deref(), Some("the evidence"));
    }
}
