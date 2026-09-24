//! The applet control channel (ADR-34).
//!
//! `layanow toggle` (and `show`/`hide`/`quit`) talk to the resident applet
//! over a local socket: a Unix domain socket on Unix/macOS and a named pipe on
//! Windows. [`interprocess`] dispatches on the name type, so the same code
//! serves every platform.
//!
//! The socket doubles as the single-instance lock: the first applet
//! binds it; a later applet notices a live owner and exits.
//! [`start`](crate::control::start) rejects a live instance and reclaims a stale
//! Unix socket file left by a crash.
//!
//! The protocol is one newline-terminated [`Command`](crate::control::Command)
//! per connection. The name is user-scoped: on Unix it lives under
//! `$XDG_RUNTIME_DIR` (a mode-0700 directory); if that is unset it falls back to
//! a per-uid subdirectory created mode 0700 with the socket itself restricted
//! to 0600, so the control channel is never exposed through a shared temp dir.
//! [`cleanup`](crate::control::cleanup) unlinks it on a clean exit
//! (process exit skips the listener's own name reclamation).

use std::io::{BufRead, BufReader, Write};
use std::thread;

use crossbeam_channel::{Receiver, Sender};
use interprocess::local_socket::{ListenerOptions, Name, prelude::*};

#[cfg(unix)]
use interprocess::local_socket::GenericFilePath;
#[cfg(windows)]
use interprocess::local_socket::GenericNamespaced;

/// The default control-channel identifier.
pub const DEFAULT_ID: &str = "layanow";

/// A command sent from the CLI to the resident applet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Show the overlay if hidden, hide it if visible.
    Toggle,
    /// Show the overlay.
    Show,
    /// Hide the overlay.
    Hide,
    /// Quit the applet.
    Quit,
    /// Re-read the settings file and apply it (checkpoint,
    /// confidence threshold, unload policy).
    Reload,
}

impl Command {
    /// The wire form of the command (without the trailing newline).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::Show => "show",
            Self::Hide => "hide",
            Self::Quit => "quit",
            Self::Reload => "reload",
        }
    }

    /// Parse a wire-form command.
    ///
    /// # Errors
    /// Returns [`ControlError::InvalidCommand`] for an unrecognised line.
    pub fn parse(line: &str) -> Result<Self, ControlError> {
        match line.trim() {
            "toggle" => Ok(Self::Toggle),
            "show" => Ok(Self::Show),
            "hide" => Ok(Self::Hide),
            "quit" => Ok(Self::Quit),
            "reload" => Ok(Self::Reload),
            other => Err(ControlError::InvalidCommand(other.to_string())),
        }
    }
}

/// Errors from the applet control channel.
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    /// Another applet already owns the control socket.
    #[error("another layanow instance is already running")]
    AlreadyRunning,
    /// No applet is listening on the control socket.
    #[error("no running layanow instance ({0})")]
    NotRunning(String),
    /// The socket could not be created or used.
    #[error("control IPC error: {0}")]
    Io(#[from] std::io::Error),
    /// The peer sent an unknown command.
    #[error("unknown control command: {0:?}")]
    InvalidCommand(String),
}

/// A running control server: the socket listener plus the command channel it
/// feeds.
pub struct Control {
    /// Sender for additional command producers such as the system tray.
    pub sender: Sender<Command>,
    /// Receiver the applet drains (handed to the overlay).
    pub receiver: Receiver<Command>,
}

/// Start the control server for the default identifier.
///
/// The listener runs on a dedicated thread and forwards commands through the
/// returned [`Control::receiver`].
///
/// # Errors
/// Returns [`ControlError::AlreadyRunning`] if another applet owns the socket.
pub fn start() -> Result<Control, ControlError> {
    start_with(DEFAULT_ID)
}

/// Send `command` to the applet listening on the default identifier.
///
/// # Errors
/// Returns [`ControlError::NotRunning`] if no applet is listening.
pub fn send(command: Command) -> Result<(), ControlError> {
    send_to(DEFAULT_ID, command)
}

/// Remove the control socket for the default identifier.
///
/// The listener reclaims its name on drop, but the applet's control thread is
/// blocked on `accept`, so process exit does not run its destructor. The applet
/// calls this after its event loop returns. A no-op on Windows, where the named
/// pipe disappears with the process.
pub fn cleanup() {
    cleanup_named(DEFAULT_ID);
}

/// Acquire the single-instance lock for a named control channel.
///
/// Used by the settings window, which needs the lock but does not consume
/// commands. Keep the returned [`Control`] alive for the process lifetime.
///
/// # Errors
/// Returns [`ControlError::AlreadyRunning`] if another process owns `id`.
pub fn claim(id: &str) -> Result<Control, ControlError> {
    start_with(id)
}

/// Remove a named control socket (see [`cleanup`]).
pub fn cleanup_named(id: &str) {
    #[cfg(unix)]
    if let Ok(path) = socket_path(id) {
        drop(std::fs::remove_file(path));
    }
}

/// Start the control server for `id` (the seam used by tests).
fn start_with(id: &str) -> Result<Control, ControlError> {
    let listener = bind(id)?;
    let (sender, receiver) = crossbeam_channel::unbounded();
    let server_sender = sender.clone();
    thread::Builder::new()
        .name("layanow-control".to_string())
        .spawn(move || serve(&listener, &server_sender))?;
    Ok(Control { sender, receiver })
}

/// Send `command` to the applet listening on `id`.
fn send_to(id: &str, command: Command) -> Result<(), ControlError> {
    let name = socket_name(id)?;
    let mut stream = LocalSocketStream::connect(name.borrow())
        .map_err(|error| ControlError::NotRunning(error.to_string()))?;
    stream.write_all(command.as_str().as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

/// Bind the control socket for `id`, rejecting a live owner and reclaiming a
/// stale Unix socket file left by a crash.
fn bind(id: &str) -> Result<LocalSocketListener, ControlError> {
    let name = socket_name(id)?;
    // A successful connect means a live instance owns the name. This
    // also tells a live owner apart from a stale socket file.
    if LocalSocketStream::connect(name.borrow()).is_ok() {
        return Err(ControlError::AlreadyRunning);
    }
    #[cfg(unix)]
    {
        // The connect above failed, so any file at the path is stale; remove it
        // so the bind can succeed. Named pipes leave nothing behind.
        drop(std::fs::remove_file(socket_path(id)?));
    }
    match ListenerOptions::new().name(name).create_sync() {
        Ok(listener) => {
            // Defence in depth: the containing directory is already user-private,
            // but make the socket itself 0600 as well.
            #[cfg(unix)]
            restrict_socket(&socket_path(id)?)?;
            Ok(listener)
        }
        // Another instance won the race between connect and bind.
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            Err(ControlError::AlreadyRunning)
        }
        Err(error) => Err(error.into()),
    }
}

/// Accept connections until the listener fails; each connection carries one
/// newline-terminated [`Command`].
fn serve(listener: &LocalSocketListener, tx: &Sender<Command>) {
    loop {
        let stream = match listener.accept() {
            Ok(stream) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let mut line = String::new();
        if BufReader::new(stream).read_line(&mut line).is_err() {
            continue;
        }
        let Ok(command) = Command::parse(&line) else {
            continue;
        };
        // A dropped receiver means the applet is shutting down.
        if tx.send(command).is_err() {
            break;
        }
    }
}

/// Build a local-socket name for `id`.
///
/// Unix uses a filesystem path under `$XDG_RUNTIME_DIR`; Windows uses a named
/// pipe (`interprocess` prepends `\\.\pipe\`).
fn socket_name(id: &str) -> Result<Name<'static>, ControlError> {
    #[cfg(unix)]
    {
        Ok(socket_path(id)?.to_fs_name::<GenericFilePath>()?)
    }
    #[cfg(windows)]
    {
        Ok(id.to_owned().to_ns_name::<GenericNamespaced>()?)
    }
}

/// The Unix socket path for `id` (Unix only).
///
/// Prefers `$XDG_RUNTIME_DIR`; when it is unset, falls back to a user-private
/// directory under the temp dir (see [`private_runtime_dir`]).
#[cfg(unix)]
fn socket_path(id: &str) -> Result<std::path::PathBuf, ControlError> {
    Ok(private_runtime_dir()?.join(format!("{id}.sock")))
}

/// The user-private directory that holds the control socket.
///
/// `$XDG_RUNTIME_DIR` is already a per-user, mode-0700 directory, so it is
/// trusted as-is. When it is unset we must not put the socket directly in a
/// shared temp dir: instead it lives in a per-uid subdirectory created mode
/// 0700, and [`bind`] restricts the socket itself to 0600, so another user
/// cannot connect to the applet's control channel.
#[cfg(unix)]
fn private_runtime_dir() -> Result<std::path::PathBuf, ControlError> {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty()) {
        return Ok(std::path::PathBuf::from(dir));
    }
    let dir = std::env::temp_dir().join(format!("layanow-{}", user_id()));
    secure_private_dir(&dir)?;
    Ok(dir)
}

/// Create `dir` if absent and force it to mode 0700, refusing anything that is
/// not a real directory (e.g. a symlink or a file another user planted).
#[cfg(unix)]
fn secure_private_dir(dir: &std::path::Path) -> Result<(), ControlError> {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::symlink_metadata(dir) {
        Ok(meta) if !meta.file_type().is_dir() => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} exists and is not a private directory", dir.display()),
            )
            .into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(dir)?;
        }
        Err(error) => return Err(error.into()),
    }
    // set_permissions fails with EPERM if another user owns the directory,
    // which is exactly the case we must refuse.
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// Restrict a Unix socket to the current user.
#[cfg(unix)]
fn restrict_socket(path: &std::path::Path) -> Result<(), ControlError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// The current user's numeric id.
#[cfg(unix)]
fn user_id() -> u32 {
    // SAFETY: `getuid` takes no arguments, cannot fail, and has no side effects.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    /// A per-process, per-call identifier so parallel tests never share a
    /// socket.
    fn test_id(label: &str) -> String {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        format!(
            "layanow-test-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }

    #[test]
    fn commands_round_trip_through_the_wire_form() {
        for command in
            [Command::Toggle, Command::Show, Command::Hide, Command::Quit, Command::Reload]
        {
            assert_eq!(Command::parse(command.as_str()).ok(), Some(command));
        }
        assert!(matches!(Command::parse("bogus"), Err(ControlError::InvalidCommand(_))));
    }

    #[test]
    fn a_sent_command_reaches_the_server() {
        let id = test_id("recv");
        let listener = bind(&id).expect("bind");
        let server = thread::spawn(move || {
            let stream = listener.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).expect("read");
            line
        });
        send_to(&id, Command::Toggle).expect("send");
        assert_eq!(Command::parse(&server.join().expect("join")).ok(), Some(Command::Toggle));
    }

    #[test]
    fn a_second_bind_reports_already_running() {
        let id = test_id("single");
        let _listener = bind(&id).expect("bind");
        assert!(matches!(bind(&id), Err(ControlError::AlreadyRunning)));
    }

    #[test]
    fn sending_without_a_server_is_not_running() {
        assert!(matches!(
            send_to(&test_id("absent"), Command::Quit),
            Err(ControlError::NotRunning(_))
        ));
    }

    #[test]
    fn the_server_forwards_a_command_to_the_receiver() {
        let id = test_id("start");
        let listener = bind(&id).expect("bind");
        let (tx, rx) = crossbeam_channel::unbounded();
        let server = thread::spawn(move || serve(&listener, &tx));
        send_to(&id, Command::Hide).expect("send");
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).ok(), Some(Command::Hide));
        // Nudge the server once more so it observes the closed channel and
        // drops the listener (which unlinks the socket).
        drop(rx);
        send_to(&id, Command::Quit).expect("send");
        server.join().expect("join");
    }

    #[cfg(unix)]
    #[test]
    fn the_fallback_directory_is_private_and_rejects_a_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(test_id("private-dir"));
        drop(std::fs::remove_dir_all(&dir));
        secure_private_dir(&dir).expect("create private dir");
        let mode = std::fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "the fallback dir must be mode 0700");
        drop(std::fs::remove_dir_all(&dir));

        std::fs::write(&dir, b"not a directory").expect("write");
        assert!(secure_private_dir(&dir).is_err(), "a squatting file must be refused");
        drop(std::fs::remove_file(&dir));
    }
}
