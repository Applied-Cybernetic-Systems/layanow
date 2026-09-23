//! The overlay host: a Wayland layer-shell surface rendering egui.
//!
//! OS-specific windowing lives in this crate (ADR-20). `winit` (and therefore
//! `eframe`) cannot create `wlr-layer-shell` surfaces, so the host drives
//! `smithay-client-toolkit` directly and renders egui with `egui_glow` on an
//! EGL/GLES context bound to the layer surface. The UI is supplied through
//! [`OverlayApp`](crate::overlay::OverlayApp), which keeps the egui code in
//! `layanow-app`.
//!
//! The layer surface is anchored to every edge (fullscreen), sits on the
//! `Overlay` layer, and starts **hidden**: no buffer is attached and keyboard
//! interactivity is `None`, so it never grabs the keyboard until it is shown
//! (ADR-34). While hidden the input region stays empty, so the pointer passes
//! through to the application underneath (ADR-14). When
//! [`OverlayApp::visible`](crate::overlay::OverlayApp::visible) becomes true it
//! takes exclusive keyboard interactivity (ADR-26); hiding it again blanks the
//! surface to transparent while keeping it mapped (unmapping would leave the
//! layer surface unconfigured and break the next show, T-174). When
//! [`OverlayApp::interactive_rect`](crate::overlay::OverlayApp::interactive_rect)
//! returns a rectangle, only that region receives the pointer (so a click can
//! dismiss the results, ADR-15) and everything else stays click-through
//! (T-166).

use egui::Context;

/// The UI drawn by the overlay host.
pub trait OverlayApp {
    /// Called once before the first frame, e.g. to install a theme.
    fn configure(&mut self, _ctx: &Context) {}

    /// Process events that are not tied to a rendered frame, e.g. control
    /// commands from the applet socket (ADR-34). Called on every host tick,
    /// whether or not the overlay is visible. The default does nothing.
    fn poll(&mut self) {}

    /// Whether the overlay is currently shown. While `false` the host keeps the
    /// surface unrendered, click-through, and without keyboard interactivity
    /// (ADR-34). Defaults to `true` for hosts that do not implement a toggle.
    fn visible(&self) -> bool {
        true
    }

    /// Draw one frame.
    fn update(&mut self, ctx: &Context);

    /// The area of the surface that should receive pointer input this frame,
    /// in logical points, or `None` to stay fully click-through.
    ///
    /// The host sets the layer surface's input region to exactly this
    /// rectangle, so clicks and scrolls outside it pass through to the
    /// application underneath (ADR-14, T-166). The default is `None`, which
    /// leaves the whole surface click-through.
    fn interactive_rect(&self) -> Option<egui::Rect> {
        None
    }

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
