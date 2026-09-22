//! Gruvbox (medium contrast) theme and shadowed text for the overlay.
//!
//! The overlay floats over arbitrary desktop content, so text is drawn with a
//! soft drop shadow and in gruvbox's brightest foreground to stay legible. The
//! palette is the "medium" contrast variant (`bg0 = #282828`, `fg0 = #fbf1c7`).

use egui::{Color32, FontId, Stroke, Visuals};

// --- gruvbox medium palette -------------------------------------------------

/// `bg0`, the base background.
pub const BG0: Color32 = Color32::from_rgb(0x28, 0x28, 0x28);
/// `bg1`, one step up from the base.
pub const BG1: Color32 = Color32::from_rgb(0x3c, 0x38, 0x36);
/// `bg2`, widget backgrounds.
pub const BG2: Color32 = Color32::from_rgb(0x50, 0x49, 0x45);
/// `fg0`, the brightest foreground (used for primary text).
pub const FG0: Color32 = Color32::from_rgb(0xfb, 0xf1, 0xc7);
/// `fg1`, the default foreground.
pub const FG1: Color32 = Color32::from_rgb(0xeb, 0xdb, 0xb2);
/// `fg4`, a muted foreground.
pub const FG4: Color32 = Color32::from_rgb(0xa8, 0x99, 0x84);
/// `gray`, the neutral accent.
pub const GRAY: Color32 = Color32::from_rgb(0x92, 0x83, 0x74);
/// `red`, errors.
pub const RED: Color32 = Color32::from_rgb(0xfb, 0x49, 0x34);
/// `green`, answers.
pub const GREEN: Color32 = Color32::from_rgb(0xb8, 0xbb, 0x26);
/// `yellow`, warnings.
pub const YELLOW: Color32 = Color32::from_rgb(0xfa, 0xbd, 0x2f);
/// `blue`, the question.
pub const BLUE: Color32 = Color32::from_rgb(0x83, 0xa5, 0x98);
/// `aqua`, links and accents.
pub const AQUA: Color32 = Color32::from_rgb(0x8e, 0xc0, 0x7c);
/// `orange`, the highlight accent.
pub const ORANGE: Color32 = Color32::from_rgb(0xfe, 0x80, 0x19);

/// The translucent gruvbox backdrop drawn over the desktop.
///
/// Premultiplied `bg0` at ~59% opacity (the const constructor requires
/// premultiplied channels).
pub const OVERLAY_BG: Color32 = Color32::from_rgba_premultiplied(0x18, 0x18, 0x18, 150);
/// Text drop-shadow colour.
pub const SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 200);
/// Text drop-shadow offset, in points.
pub const SHADOW_OFFSET: egui::Vec2 = egui::vec2(1.5, 1.5);

/// Font size for titles.
pub const TITLE_SIZE: f32 = 26.0;
/// Font size for body text.
pub const BODY_SIZE: f32 = 18.0;

/// Install the gruvbox visuals into `ctx`.
pub fn install(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.window_fill = BG0;
    visuals.extreme_bg_color = BG0;
    visuals.faint_bg_color = BG1;
    visuals.window_stroke = Stroke::new(1.0, BG2);
    visuals.hyperlink_color = AQUA;
    visuals.warn_fg_color = YELLOW;
    visuals.error_fg_color = RED;
    visuals.selection.bg_fill = BLUE.gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0, FG0);

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = BG1;
    widgets.noninteractive.weak_bg_fill = BG1;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, BG2);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, FG0);
    widgets.inactive.bg_fill = BG1;
    widgets.inactive.weak_bg_fill = BG1;
    widgets.inactive.bg_stroke = Stroke::new(1.0, BG2);
    widgets.inactive.fg_stroke = Stroke::new(1.0, FG1);
    widgets.hovered.bg_fill = BG2;
    widgets.hovered.weak_bg_fill = BG2;
    widgets.hovered.bg_stroke = Stroke::new(1.0, FG4);
    widgets.hovered.fg_stroke = Stroke::new(1.0, FG0);
    widgets.active.bg_fill = BG2;
    widgets.active.weak_bg_fill = BG2;
    widgets.active.bg_stroke = Stroke::new(1.0, YELLOW);
    widgets.active.fg_stroke = Stroke::new(1.0, FG0);

    ctx.set_visuals(visuals);
}

/// Draw `text` with a drop shadow, in `color` at `size` points.
///
/// The galley is laid out once and painted twice: once offset in the shadow
/// colour, then in `color` on top.
pub fn shadowed_text(ui: &mut egui::Ui, text: &str, color: Color32, size: f32) {
    let font = FontId::proportional(size);
    let galley = ui.painter().layout(text.to_owned(), font, color, ui.available_width());
    let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    ui.painter().galley_with_override_text_color(rect.min + SHADOW_OFFSET, galley.clone(), SHADOW);
    ui.painter().galley_with_override_text_color(rect.min, galley, color);
}

/// Draw `label` followed by `text` on one wrapping line with a drop shadow,
/// using a separate colour for each part.
///
/// Useful for a muted prefix (`"Question: "`) in front of brighter content.
pub fn shadowed_labelled_text(
    ui: &mut egui::Ui,
    label: &str,
    label_color: Color32,
    text: &str,
    text_color: Color32,
    size: f32,
) {
    let font = FontId::proportional(size);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    job.append(
        label,
        0.0,
        egui::text::TextFormat { font_id: font.clone(), color: label_color, ..Default::default() },
    );
    job.append(
        text,
        0.0,
        egui::text::TextFormat { font_id: font, color: text_color, ..Default::default() },
    );

    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    // Shadow every glyph, then paint the galley with its own per-part colours.
    ui.painter().galley_with_override_text_color(rect.min + SHADOW_OFFSET, galley.clone(), SHADOW);
    ui.painter().galley(rect.min, galley, FG0);
}
