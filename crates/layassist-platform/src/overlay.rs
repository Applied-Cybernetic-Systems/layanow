//! The overlay host: a Wayland layer-shell surface rendering egui.
//!
//! OS-specific windowing lives in this crate (ADR-20). `winit` (and therefore
//! `eframe`) cannot create `wlr-layer-shell` surfaces, so the host drives
//! `smithay-client-toolkit` directly and renders egui with `egui_glow` on an
//! EGL/GLES context bound to the layer surface. The UI is supplied through
//! [`OverlayApp`](crate::overlay::OverlayApp), which keeps the egui code in
//! `layassist-app`.
//!
//! The layer surface is anchored to every edge (fullscreen), sits on the
//! `Overlay` layer, and takes exclusive keyboard interactivity (ADR-26) while
//! its input region is empty, so the pointer passes through to the application
//! underneath (ADR-14). When
//! [`OverlayApp::wants_pointer`](crate::overlay::OverlayApp::wants_pointer) is
//! true the input region is restored so a click can dismiss the results
//! (ADR-15).

use egui::Context;

/// The UI drawn by the overlay host.
pub trait OverlayApp {
    /// Draw one frame.
    fn update(&mut self, ctx: &Context);

    /// Whether the overlay should capture the pointer this frame.
    ///
    /// While `false` the layer surface has an empty input region, so clicks and
    /// scrolls pass through to the application underneath (ADR-14).
    fn wants_pointer(&self) -> bool;

    /// Whether the host should stop its event loop.
    fn should_exit(&self) -> bool {
        false
    }
}

/// Errors from the overlay host.
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    /// No Wayland compositor (or `libwayland`) is available.
    #[error("no Wayland compositor: {0}")]
    NoWayland(String),
    /// The overlay is not implemented on this platform yet.
    #[error("the overlay is not implemented on this platform yet")]
    Unsupported,
    /// A Wayland protocol or setup call failed.
    #[error("Wayland error: {0}")]
    Wayland(String),
    /// The EGL/OpenGL context could not be created or used.
    #[error("OpenGL error: {0}")]
    Gl(String),
}

#[cfg(target_os = "linux")]
mod wayland;

/// Run the overlay until `app` asks to exit.
///
/// # Errors
/// Returns [`OverlayError`] if no compositor is available or the GL/layer-shell
/// setup fails.
#[cfg(target_os = "linux")]
pub fn run(app: impl OverlayApp + 'static) -> Result<(), OverlayError> {
    wayland::run(app)
}

/// Run the overlay until `app` asks to exit.
///
/// # Errors
/// Always returns [`OverlayError::Unsupported`] off Linux; v1 is Linux-first
/// (ADR-21/D1).
#[cfg(not(target_os = "linux"))]
pub fn run(_app: impl OverlayApp + 'static) -> Result<(), OverlayError> {
    Err(OverlayError::Unsupported)
}
