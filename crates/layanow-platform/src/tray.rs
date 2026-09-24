//! System-tray icon (ADR-20, ADR-35).
//!
//! `layanow` shows a tray icon with **Toggle overlay** and **Quit**; a left
//! click also toggles. It uses `tray-icon`: on Linux the `ksni` backend speaks
//! StatusNotifierItem over D-Bus (no GTK), while Windows and macOS use the
//! native backend. Menu and click activations post to the same
//! [`Command`](crate::control::Command) channel as the `layanow toggle` CLI.
//!
//! Creation is best-effort: the caller logs a failure and keeps running, so the
//! applet still works from the CLI when no StatusNotifier host is present.

use crossbeam_channel::Sender;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::control::Command;

/// The 32x32 app icon (`layanow.png`), embedded at build time.
const ICON_PNG: &[u8] = include_bytes!("../assets/layanow.png");

/// Menu id for "Toggle overlay".
const TOGGLE_ID: &str = "toggle";
/// Menu id for "Settings…".
const SETTINGS_ID: &str = "settings";
/// Menu id for "Quit".
const QUIT_ID: &str = "quit";

/// Errors from creating the tray icon.
#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    /// The tray icon could not be registered (often: no StatusNotifier host).
    #[error("tray icon error: {0}")]
    Tray(#[from] tray_icon::Error),
    /// The menu could not be built.
    #[error("tray menu error: {0}")]
    Menu(#[from] tray_icon::menu::Error),
    /// The embedded icon could not be decoded.
    #[error("tray icon decode error: {0}")]
    Icon(String),
}

/// Start the tray icon, forwarding activations to `commands`.
///
/// Keep the returned guard for the process lifetime; dropping it removes the
/// icon. The event handlers are installed once, globally.
///
/// # Errors
/// Returns [`TrayError`] if no tray host is available or the menu/icon is
/// invalid.
pub fn start(commands: Sender<Command>) -> Result<TrayIcon, TrayError> {
    let menu = Menu::new();
    let toggle = MenuItem::with_id(TOGGLE_ID, "Toggle overlay", true, None);
    let settings = MenuItem::with_id(SETTINGS_ID, "Settings…", true, None);
    let quit = MenuItem::with_id(QUIT_ID, "Quit", true, None);
    menu.append_items(&[&toggle, &settings, &quit])?;

    let menu_sender = commands.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id().0.as_str() {
        SETTINGS_ID => spawn_settings_window(),
        id => {
            if let Some(command) = command_for_id(id) {
                let _ = menu_sender.send(command);
            }
        }
    }));

    let click_sender = commands;
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = click_sender.send(Command::Toggle);
        }
    }));

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(icon()?)
        .with_tooltip("layanow")
        .build()?;
    Ok(tray)
}

/// Map a menu item id to the command it should post.
fn command_for_id(id: &str) -> Option<Command> {
    match id {
        TOGGLE_ID => Some(Command::Toggle),
        QUIT_ID => Some(Command::Quit),
        _ => None,
    }
}

/// Open the standalone settings window in a new process.
///
/// The settings window owns an eframe event loop, so it cannot share this
/// applet's layer-shell loop; its named control channel keeps it to a single
/// instance.
fn spawn_settings_window() {
    match std::env::current_exe() {
        Ok(executable) => {
            if let Err(error) = std::process::Command::new(executable).arg("settings").spawn() {
                tracing::warn!(%error, "could not open the settings window");
            }
        }
        Err(error) => tracing::warn!(%error, "could not locate the layanow executable"),
    }
}

/// Decode the embedded PNG into a tray [`Icon`].
fn icon() -> Result<Icon, TrayError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(ICON_PNG));
    let mut reader = decoder.read_info().map_err(|error| TrayError::Icon(error.to_string()))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| TrayError::Icon("icon dimensions overflow".to_string()))?;
    let mut buffer = vec![0; size];
    let info =
        reader.next_frame(&mut buffer).map_err(|error| TrayError::Icon(error.to_string()))?;
    if info.color_type != png::ColorType::Rgba {
        return Err(TrayError::Icon(format!("expected RGBA, got {:?}", info.color_type)));
    }
    buffer.truncate(info.buffer_size());
    Icon::from_rgba(buffer, info.width, info.height)
        .map_err(|error| TrayError::Icon(format!("{error:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_map_to_commands() {
        assert_eq!(command_for_id(TOGGLE_ID), Some(Command::Toggle));
        assert_eq!(command_for_id(QUIT_ID), Some(Command::Quit));
        assert_eq!(command_for_id("other"), None);
    }

    #[test]
    fn the_embedded_icon_is_32x32_rgba() {
        let decoder = png::Decoder::new(std::io::Cursor::new(ICON_PNG));
        let mut reader = decoder.read_info().expect("png info");
        let mut buffer = vec![0; reader.output_buffer_size().expect("buffer size")];
        let info = reader.next_frame(&mut buffer).expect("png frame");
        assert_eq!((info.width, info.height), (32, 32));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert!(icon().is_ok());
    }
}
