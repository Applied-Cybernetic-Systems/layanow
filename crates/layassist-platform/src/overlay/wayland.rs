//! Linux (Wayland) layer-shell host.
//!
//! This module is FFI/GL glue, so it is allowed a few `clippy` lints that make
//! no sense for raw-pointer and pixel-size casts.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

use std::num::NonZeroU32;
use std::os::fd::AsRawFd;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use glutin::context::{ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
};

use super::{OverlayApp, OverlayError};

/// Fallback surface size when the compositor does not propose one.
const FALLBACK_SIZE: u32 = 1920;
/// How long to wait for Wayland events before re-checking for repaints.
const POLL_MILLIS: libc::c_int = 16;

/// Run the layer-shell overlay event loop.
pub fn run(app: impl OverlayApp + 'static) -> Result<(), OverlayError> {
    let conn =
        Connection::connect_to_env().map_err(|error| OverlayError::NoWayland(error.to_string()))?;
    let (globals, mut event_queue) =
        registry_queue_init(&conn).map_err(|error| OverlayError::Wayland(error.to_string()))?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|error| OverlayError::Wayland(error.to_string()))?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|error| OverlayError::Wayland(error.to_string()))?;

    let surface = compositor.create_surface(&qh);
    let layer =
        layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("layassist"), None);
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.set_exclusive_zone(-1);
    layer.set_size(0, 0);
    layer.commit();

    let egui_ctx = egui::Context::default();
    let dirty = Arc::new(AtomicBool::new(true));
    {
        let dirty = Arc::clone(&dirty);
        egui_ctx.set_request_repaint_callback(move |_info| dirty.store(true, Ordering::Relaxed));
    }

    let mut host = Host {
        app: Box::new(app),
        conn: conn.clone(),
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        layer,
        keyboard: None,
        pointer: None,
        egui_ctx,
        raw_input: egui::RawInput::default(),
        modifiers: egui::Modifiers::NONE,
        dirty,
        // Start as if captured so the first frame installs the empty input
        // region (click-through, ADR-14).
        pointer_captured: true,
        width: 0,
        height: 0,
        gl: None,
        exit: false,
    };
    host.app.configure(&host.egui_ctx);

    loop {
        event_queue.flush().map_err(|error| OverlayError::Wayland(error.to_string()))?;
        event_queue
            .dispatch_pending(&mut host)
            .map_err(|error| OverlayError::Wayland(error.to_string()))?;

        if host.exit || host.app.should_exit() {
            break;
        }
        if host.dirty.swap(false, Ordering::Relaxed) && host.width > 0 && host.height > 0 {
            host.render()?;
        }

        // Wake at least every `POLL_MILLIS` so repaint requests are serviced.
        // The read guard must be created *before* polling the socket.
        if let Some(guard) = event_queue.prepare_read() {
            let mut pollfd = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: `pollfd` points at a valid descriptor for the duration of
            // the call.
            unsafe { libc::poll(&raw mut pollfd, 1, POLL_MILLIS) };
            if pollfd.revents & libc::POLLIN != 0 {
                guard.read().map_err(|error| OverlayError::Wayland(error.to_string()))?;
            }
            // Otherwise the guard is dropped, cancelling the pending read.
        } else {
            // Events are already queued for dispatch; yield rather than spin.
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    Ok(())
}

/// A live EGL/GLES context and egui painter bound to the layer surface.
struct Gl {
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    painter: egui_glow::Painter,
}

/// The overlay host state.
struct Host {
    app: Box<dyn OverlayApp>,
    conn: Connection,
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    layer: LayerSurface,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    egui_ctx: egui::Context,
    raw_input: egui::RawInput,
    modifiers: egui::Modifiers,
    dirty: Arc<AtomicBool>,
    pointer_captured: bool,
    width: u32,
    height: u32,
    gl: Option<Gl>,
    exit: bool,
}

impl Host {
    fn ensure_gl(&mut self) -> Result<(), OverlayError> {
        if self.gl.is_some() {
            return Ok(());
        }
        let display_handle = wayland_display_handle(&self.conn)?;
        let window_handle = wayland_window_handle(self.layer.wl_surface())?;

        // SAFETY: `display_handle` points at the live `wl_display` owned by
        // `self.conn`; both it and the `wl_surface` behind `window_handle`
        // outlive the display/surface/context created here.
        let gl_display = unsafe { Display::new(display_handle, DisplayApiPreference::Egl) }
            .map_err(|error| OverlayError::Gl(error.to_string()))?;

        let template = glutin::config::ConfigTemplateBuilder::new()
            .with_transparency(true)
            .with_alpha_size(8)
            .with_depth_size(0)
            .with_stencil_size(0)
            .build();
        // SAFETY: `gl_display` is a live EGL display.
        let gl_config = unsafe { gl_display.find_configs(template) }
            .map_err(|error| OverlayError::Gl(error.to_string()))?
            .next()
            .ok_or_else(|| OverlayError::Gl("no matching EGL config".to_string()))?;

        let attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            window_handle,
            NonZeroU32::new(self.width).unwrap_or(NonZeroU32::MIN),
            NonZeroU32::new(self.height).unwrap_or(NonZeroU32::MIN),
        );
        // SAFETY: `gl_config` belongs to `gl_display`; `attributes` carries the
        // live `wl_surface`; glutin creates and later destroys the
        // `wl_egl_window`.
        let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &attributes) }
            .map_err(|error| OverlayError::Gl(error.to_string()))?;

        let context_attributes = ContextAttributesBuilder::new().build(Some(window_handle));
        // SAFETY: same live display and config as above.
        let not_current = unsafe { gl_display.create_context(&gl_config, &context_attributes) }
            .map_err(|error| OverlayError::Gl(error.to_string()))?;
        let context = not_current
            .make_current(&gl_surface)
            .map_err(|error| OverlayError::Gl(error.to_string()))?;

        // SAFETY: `get_proc_address` is this display's EGL symbol loader; the
        // returned function pointers are only used while the context is current.
        let glow_ctx = unsafe {
            glow::Context::from_loader_function_cstr(|symbol| {
                gl_display.get_proc_address(symbol).cast()
            })
        };
        let painter = egui_glow::Painter::new(Arc::new(glow_ctx), "", None, false)
            .map_err(|error| OverlayError::Gl(error.to_string()))?;

        self.gl = Some(Gl { context, surface: gl_surface, painter });
        Ok(())
    }

    fn render(&mut self) -> Result<(), OverlayError> {
        if self.gl.is_none() {
            return Ok(());
        }
        self.raw_input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(self.width as f32, self.height as f32),
        ));

        let input = std::mem::take(&mut self.raw_input);
        let output = self.egui_ctx.run(input, |ctx| self.app.update(ctx));
        let primitives = self.egui_ctx.tessellate(output.shapes, output.pixels_per_point);

        if let Some(gl) = &mut self.gl {
            gl.painter.clear([self.width, self.height], [0.0, 0.0, 0.0, 0.0]);
            gl.painter.paint_and_update_textures(
                [self.width, self.height],
                output.pixels_per_point,
                &primitives,
                &output.textures_delta,
            );
            gl.surface
                .swap_buffers(&gl.context)
                .map_err(|error| OverlayError::Gl(error.to_string()))?;
        }

        self.apply_input_region()?;
        Ok(())
    }

    /// Mirror [`OverlayApp::wants_pointer`] onto the surface's input region.
    fn apply_input_region(&mut self) -> Result<(), OverlayError> {
        let wants = self.app.wants_pointer();
        if wants == self.pointer_captured {
            return Ok(());
        }
        self.pointer_captured = wants;
        if wants {
            // `None` restores the whole surface as the input region.
            self.layer.set_input_region(None);
        } else {
            // An empty region makes the surface click-through (ADR-14).
            let region = Region::new(&self.compositor)
                .map_err(|error| OverlayError::Wayland(error.to_string()))?;
            self.layer.set_input_region(Some(region.wl_region()));
        }
        self.layer.wl_surface().commit();
        Ok(())
    }

    fn push_key(&mut self, event: &KeyEvent, repeat: bool) {
        self.dirty.store(true, Ordering::Relaxed);
        if let Some(key) = map_key(event.keysym) {
            self.raw_input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat,
                modifiers: self.modifiers,
            });
        } else if let Some(text) = event.utf8.as_deref().filter(|text| !text.is_empty()) {
            self.raw_input.events.push(egui::Event::Text(text.to_string()));
        }
    }
}

impl CompositorHandler for Host {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _factor: i32,
    ) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Host {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for Host {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        eprintln!("layassist: layer surface closed by compositor");
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let width = if configure.new_size.0 == 0 { FALLBACK_SIZE } else { configure.new_size.0 };
        let height = if configure.new_size.1 == 0 { FALLBACK_SIZE } else { configure.new_size.1 };
        if (width, height) != (self.width, self.height) {
            self.width = width;
            self.height = height;
            if let Some(gl) = &self.gl {
                gl.surface.resize(
                    &gl.context,
                    NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN),
                    NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN),
                );
            }
        }
        if let Err(error) = self.ensure_gl() {
            eprintln!("layassist: {error}");
            self.exit = true;
        }
        self.dirty.store(true, Ordering::Relaxed);
    }
}

impl SeatHandler for Host {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => eprintln!("layassist: keyboard unavailable: {error}"),
            }
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seat_state.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(error) => eprintln!("layassist: pointer unavailable: {error}"),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(keyboard) = self.keyboard.take() {
                keyboard.release();
            }
        }
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }

    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {
    }
}

impl KeyboardHandler for Host {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
        if self.layer.wl_surface() == surface {
            self.dirty.store(true, Ordering::Relaxed);
        }
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        self.push_key(&event, false);
    }

    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        self.push_key(&event, true);
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        _raw: RawModifiers,
        _layout: u32,
    ) {
        self.modifiers = egui::Modifiers {
            alt: modifiers.alt,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            mac_cmd: false,
            command: modifiers.ctrl,
        };
    }
}

impl PointerHandler for Host {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            self.dirty.store(true, Ordering::Relaxed);
            let position = egui::pos2(event.position.0 as f32, event.position.1 as f32);
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.raw_input.events.push(egui::Event::PointerMoved(position));
                }
                PointerEventKind::Leave { .. } => {
                    self.raw_input.events.push(egui::Event::PointerGone);
                }
                PointerEventKind::Press { button, .. } => {
                    self.raw_input.events.push(egui::Event::PointerButton {
                        pos: position,
                        button: map_button(button),
                        pressed: true,
                        modifiers: self.modifiers,
                    });
                }
                PointerEventKind::Release { button, .. } => {
                    self.raw_input.events.push(egui::Event::PointerButton {
                        pos: position,
                        button: map_button(button),
                        pressed: false,
                        modifiers: self.modifiers,
                    });
                }
                PointerEventKind::Axis { .. } => {}
            }
        }
    }
}

impl ProvidesRegistryState for Host {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Host);
delegate_output!(Host);
delegate_seat!(Host);
delegate_keyboard!(Host);
delegate_pointer!(Host);
delegate_layer!(Host);
delegate_registry!(Host);

/// The raw `wl_display` behind a connection, for EGL.
fn wayland_display_handle(conn: &Connection) -> Result<RawDisplayHandle, OverlayError> {
    let ptr = conn.backend().display_ptr();
    let ptr = NonNull::new(ptr.cast())
        .ok_or_else(|| OverlayError::Wayland("null wl_display".to_string()))?;
    Ok(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(ptr)))
}

/// The raw `wl_surface` behind a surface proxy, for EGL.
fn wayland_window_handle(surface: &wl_surface::WlSurface) -> Result<RawWindowHandle, OverlayError> {
    let ptr = surface.id().as_ptr();
    let ptr = NonNull::new(ptr.cast())
        .ok_or_else(|| OverlayError::Wayland("null wl_surface".to_string()))?;
    Ok(RawWindowHandle::Wayland(WaylandWindowHandle::new(ptr)))
}

/// Map the keys the overlay cares about; everything else arrives as text.
fn map_key(keysym: Keysym) -> Option<egui::Key> {
    Some(match keysym {
        Keysym::Escape => egui::Key::Escape,
        Keysym::Return => egui::Key::Enter,
        Keysym::Tab => egui::Key::Tab,
        Keysym::BackSpace => egui::Key::Backspace,
        Keysym::Delete => egui::Key::Delete,
        Keysym::Home => egui::Key::Home,
        Keysym::End => egui::Key::End,
        Keysym::Left => egui::Key::ArrowLeft,
        Keysym::Right => egui::Key::ArrowRight,
        Keysym::Up => egui::Key::ArrowUp,
        Keysym::Down => egui::Key::ArrowDown,
        _ => return None,
    })
}

/// Map a Linux input-event button code to an egui pointer button.
fn map_button(button: u32) -> egui::PointerButton {
    match button {
        0x111 => egui::PointerButton::Secondary,
        0x112 => egui::PointerButton::Middle,
        _ => egui::PointerButton::Primary,
    }
}
