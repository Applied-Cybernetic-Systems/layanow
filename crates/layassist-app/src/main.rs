//! The `layassist` tray applet binary.
//!
//! M3: load the default checkpoint, start the inference worker, and run the
//! click-through overlay. The tray icon, settings, and hotkey wiring land in
//! M5; the platform selection resolver lands in M4.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use eframe::egui;
use layassist_app::overlay::Overlay;
use layassist_app::worker::Worker;
use layassist_model::{Decider, Quant, bundle};

fn main() {
    if let Err(error) = run() {
        eprintln!("layassist: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let worker = load_worker()?;

    // Transparent, undecorated, always-on-top, click-through (ADR-14/25). True
    // Wayland layer-shell keyboard interactivity is wired in M4 alongside the
    // selection backend; on X11/XWayland eframe's window already receives keys.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("layassist")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_fullscreen(true)
            .with_mouse_passthrough(true),
        ..Default::default()
    };

    eframe::run_native(
        "layassist",
        options,
        Box::new(|_creation| Ok(Box::new(Overlay::new(worker)))),
    )?;
    Ok(())
}

/// Load the default checkpoint and start the inference worker.
///
/// Weights are downloaded on first use when `LAYASSIST_ALLOW_MODEL_DOWNLOAD`
/// is set (ADR-17); otherwise a cached bundle is required.
fn load_worker() -> Result<Worker, Box<dyn std::error::Error>> {
    let dir = bundle::ensure_bundle()?;
    eprintln!("layassist: using bundle at {}", dir.display());
    let checkpoint = bundle::checkpoint_from_dir(bundle::DEFAULT_REPO, &dir, Quant::Fp32)?;
    Ok(Worker::spawn(Decider::load(&checkpoint)?))
}
