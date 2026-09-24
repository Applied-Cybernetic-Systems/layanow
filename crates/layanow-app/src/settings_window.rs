//! The standalone settings window (T-113/T-115/T-117).
//!
//! This runs as its own process (`layanow settings`) with a normal
//! `xdg_toplevel` and an eframe/glow event loop, so it is independent of the
//! Wayland layer-shell overlay host (ADR-31). Editing writes
//! `~/.config/layanow/config.toml` and sends `Command::Reload` to the resident
//! applet, which re-reads the file and applies the change live. Keeping the
//! file as the source of truth means the window needs no bidirectional protocol.
//!
//! A named control channel (`layanow-settings`) acts as the single-instance
//! lock, so opening settings twice does not spawn a second window.

use eframe::egui;

use crate::settings::{Settings, UnloadPolicy};
use layanow_model::Quant;
use layanow_model::bundle;
use layanow_platform::control::{self, Command, ControlError};

/// The control-channel id that locks the settings window to one instance.
const LOCK_ID: &str = "layanow-settings";

/// Run the settings window until the user closes it.
///
/// # Errors
/// Returns an error if the lock cannot be taken or eframe fails to start.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = match control::claim(LOCK_ID) {
        Ok(lock) => lock,
        Err(ControlError::AlreadyRunning) => {
            tracing::info!("a settings window is already open");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("layanow settings")
            .with_inner_size([480.0, 470.0])
            .with_min_inner_size([400.0, 360.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "layanow settings",
        options,
        Box::new(|_cc| Ok(Box::new(SettingsApp::default()))),
    );
    control::cleanup_named(LOCK_ID);
    result.map_err(Into::into)
}

/// The settings window's persistent state.
struct SettingsApp {
    /// The (possibly edited) settings shown in the widgets.
    settings: Settings,
    /// The last settings written to disk, so an edit can be detected.
    applied: Settings,
    /// A short user-facing status line.
    status: Option<String>,
}

impl Default for SettingsApp {
    fn default() -> Self {
        let settings = Settings::load();
        Self { applied: settings.clone(), settings, status: None }
    }
}

impl SettingsApp {
    /// Persist the current settings and tell the applet to apply them.
    fn apply(&mut self) {
        match self.settings.save() {
            Ok(()) => match control::send(Command::Reload) {
                Ok(()) => self.status = Some("Applied.".to_string()),
                Err(error) => {
                    self.status = Some(format!("Saved; the applet is not running ({error})."));
                }
            },
            Err(error) => self.status = Some(format!("Could not save settings: {error}")),
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("layanow");
            ui.add_space(8.0);

            ui.label("Checkpoint");
            egui::ComboBox::from_id_salt("checkpoint")
                .width(300.0)
                .selected_text(checkpoint_label(&self.settings.checkpoint))
                .show_ui(ui, |ui| {
                    for spec in bundle::CHECKPOINTS {
                        ui.selectable_value(
                            &mut self.settings.checkpoint,
                            spec.id.to_string(),
                            spec.name,
                        );
                    }
                });
            let spec = bundle::checkpoint(&self.settings.checkpoint);
            if let Some(spec) = spec {
                ui.small(format!("{} graph variant(s) available", spec.variants.len()));
                // A checkpoint change may drop the currently selected precision;
                // fall back to its first (fp32) variant.
                if bundle::variant(spec, self.settings.quant).is_none() {
                    if let Some(first) = spec.variants.first() {
                        self.settings.quant = first.quant;
                    }
                }
            } else {
                ui.small(format!("unknown checkpoint {:?}", self.settings.checkpoint));
            }

            ui.add_space(10.0);
            ui.label("Precision");
            egui::ComboBox::from_id_salt("quant")
                .width(300.0)
                .selected_text(quant_label(self.settings.quant))
                .show_ui(ui, |ui| {
                    if let Some(spec) = spec {
                        for variant in spec.variants {
                            ui.selectable_value(
                                &mut self.settings.quant,
                                variant.quant,
                                quant_label(variant.quant),
                            );
                        }
                    }
                });
            ui.small(
                "fp32: highest fidelity · fp16: smaller, slower on CPUs without native FP16 · \
                 int8: smallest and fastest, opt-in (ADR-24/39)",
            );
            if self.settings.quant == Quant::Int8 {
                ui.colored_label(
                    crate::theme::YELLOW,
                    "⚠ int8 can change answers: 19/20 top-1 agreement with fp32 in our \
                     evaluation (ADR-39).",
                );
            }

            ui.add_space(10.0);
            ui.add(
                egui::Slider::new(&mut self.settings.confidence_threshold, 0.0..=1.0)
                    .text("low-confidence threshold"),
            )
            .on_hover_text("Below this, the decision is flagged — it is never refused.");

            ui.add_space(10.0);
            ui.label("Model residency");
            ui.radio_value(
                &mut self.settings.unload,
                UnloadPolicy::Hot,
                "Hot — keep the model ready (instant answers, ~2 GB idle)",
            );
            ui.radio_value(
                &mut self.settings.unload,
                UnloadPolicy::OnDemand,
                "On demand — unload after each decision (slower next answer)",
            );

            ui.add_space(10.0);
            ui.label("Probability colours");
            ui.horizontal(|ui| {
                ui.label("low");
                ui.color_edit_button_srgb(&mut self.settings.palette.low);
                ui.label("mid");
                ui.color_edit_button_srgb(&mut self.settings.palette.mid);
                ui.label("high");
                ui.color_edit_button_srgb(&mut self.settings.palette.high);
            });
            ui.small("The bar colour lerps low → mid → high as the probability rises.");

            ui.add_space(12.0);
            if let Some(status) = &self.status {
                ui.label(status);
            }
            ui.small(format!("Saved to {}", Settings::path().display()));
        });

        if self.settings != self.applied {
            self.apply();
            self.applied = self.settings.clone();
        }
    }
}

/// The display label for a checkpoint id, falling back to the raw id.
fn checkpoint_label(id: &str) -> String {
    bundle::checkpoint(id).map_or_else(|| id.to_string(), |spec| spec.name.to_string())
}

/// The display label for a weight precision.
fn quant_label(quant: Quant) -> &'static str {
    match quant {
        Quant::Fp32 => "fp32 — highest fidelity",
        Quant::Fp16 => "fp16 — smaller, slower on CPU",
        Quant::Int8 => "int8 — smallest, fastest (lower accuracy)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_label_falls_back_to_the_id() {
        assert_eq!(checkpoint_label("english"), bundle::ENGLISH.name);
        assert_eq!(checkpoint_label("mystery"), "mystery");
    }

    #[test]
    fn quant_labels_are_distinct() {
        assert_ne!(quant_label(Quant::Fp32), quant_label(Quant::Fp16));
        assert_ne!(quant_label(Quant::Fp16), quant_label(Quant::Int8));
    }
}
