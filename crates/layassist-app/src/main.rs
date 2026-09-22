//! The `layassist` applet binary.
//!
//! Load the default checkpoint, start the inference worker, and run the
//! click-through layer-shell overlay (`layassist-platform`). The tray icon,
//! settings, and hotkey wiring land in M5. Selection capture uses the platform
//! PRIMARY-selection backend (`layassist-platform::selection`, M4).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

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
    let resolver = layassist_platform::selection::resolver();
    layassist_platform::overlay::run(Overlay::new(worker, resolver))?;
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
