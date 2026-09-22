//! The `layanow` applet binary.
//!
//! With no arguments it runs the **resident** applet: the overlay starts hidden
//! and the control socket listens for `layanow toggle`/`show`/`hide`/`quit`
//! (ADR-34). The tray, settings, and in-process hotkey land later in M5.
//! Selection capture uses the platform PRIMARY-selection backend
//! (`layanow-platform::selection`, M4).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use layanow_app::overlay::Overlay;
use layanow_app::worker::Worker;
use layanow_model::{Decider, Quant, bundle};
use layanow_platform::control::{self, Command, ControlError};

fn main() {
    if let Err(error) = run() {
        eprintln!("layanow: {error}");
        std::process::exit(1);
    }
}

/// What the command line asked for.
enum Invocation {
    /// Run the resident applet.
    Applet,
    /// Forward a command to a running applet.
    Command(Command),
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let Some(invocation) = parse_args(std::env::args().skip(1))? else {
        return Ok(());
    };
    match invocation {
        Invocation::Applet => run_applet(),
        Invocation::Command(command) => {
            control::send(command)?;
            Ok(())
        }
    }
}

/// Parse the command line. `Ok(None)` means help was printed and the process
/// should exit successfully.
fn parse_args(args: impl Iterator<Item = String>) -> std::io::Result<Option<Invocation>> {
    let args: Vec<String> = args.collect();
    match args.as_slice() {
        [] => Ok(Some(Invocation::Applet)),
        [flag] if flag == "-h" || flag == "--help" => {
            print_help();
            Ok(None)
        }
        [word] => Command::parse(word)
            .map(|command| Some(Invocation::Command(command)))
            .map_err(|_| invalid(format!("unknown command {word:?}; try --help"))),
        _ => Err(invalid("too many arguments; try --help".to_string())),
    }
}

/// Build an `InvalidInput` error for the CLI boundary.
fn invalid(message: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}

fn print_help() {
    println!(
        "layanow — local multiple-choice assistant\n\
         \n\
         Usage:\n\
           layanow            run the resident applet (overlay starts hidden)\n\
           layanow toggle     show/hide the overlay\n\
           layanow show       show the overlay\n\
           layanow hide       hide the overlay\n\
           layanow quit       stop the running applet\n\
           layanow --help     print this help"
    );
}

/// Run the resident applet until it quits.
fn run_applet() -> Result<(), Box<dyn std::error::Error>> {
    // Bind the control socket first: it is the single-instance lock (T-153) and
    // makes `layanow toggle` responsive while the model loads.
    let commands = match control::start() {
        Ok(commands) => commands,
        Err(ControlError::AlreadyRunning) => {
            eprintln!("layanow: already running");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let worker = load_worker()?;
    let resolver = layanow_platform::selection::resolver();
    let result = layanow_platform::overlay::run(Overlay::new(worker, resolver, commands));
    // Process exit does not run the control thread's destructor, so unlink the
    // socket explicitly on a clean exit.
    control::cleanup();
    result?;
    Ok(())
}

/// Load the default checkpoint and start the inference worker.
///
/// Weights are downloaded on first use when `LAYANOW_ALLOW_MODEL_DOWNLOAD`
/// is set (ADR-17); otherwise a cached bundle is required.
fn load_worker() -> Result<Worker, Box<dyn std::error::Error>> {
    let dir = bundle::ensure_bundle()?;
    eprintln!("layanow: using bundle at {}", dir.display());
    let checkpoint = bundle::checkpoint_from_dir(bundle::DEFAULT_REPO, &dir, Quant::Fp32)?;
    Ok(Worker::spawn(Decider::load(&checkpoint)?))
}
