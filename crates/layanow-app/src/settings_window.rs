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
            .with_inner_size([460.0, 340.0])
            .with_min_inner_size([380.0, 280.0]),
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
            if let Some(spec) = bundle::checkpoint(&self.settings.checkpoint) {
                ui.small(spec.repo);
            } else {
                ui.small(format!("unknown checkpoint {:?}", self.settings.checkpoint));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_label_falls_back_to_the_id() {
        assert_eq!(checkpoint_label("english"), bundle::ENGLISH.name);
        assert_eq!(checkpoint_label("mystery"), "mystery");
    }
}
