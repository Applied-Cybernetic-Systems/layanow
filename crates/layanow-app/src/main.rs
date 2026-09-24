//! The `layanow` applet binary.
//!
//! With no arguments it runs the **resident** applet: the overlay starts hidden
//! and the control socket listens for `layanow toggle`/`show`/`hide`/`quit`
//! (ADR-34), and a tray icon offers the same actions (ADR-35). The settings and
//! in-process hotkey land later in M5. Selection capture uses the platform
//! PRIMARY-selection backend (`layanow-platform::selection`, M4).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use layanow_app::overlay::Overlay;
use layanow_app::settings::{Settings, UnloadPolicy};
use layanow_app::worker::{EngineFactory, Worker};
use layanow_model::{Decider, bundle};
use layanow_platform::control::{self, Command, ControlError};

fn main() {
    init_tracing();
    if let Err(error) = run() {
        tracing::error!(%error, "fatal");
        std::process::exit(1);
    }
}

/// Install the `tracing` subscriber (E4).
///
/// Verbosity is controlled by `LAYANOW_LOG`, falling back to `RUST_LOG`, using
/// an `EnvFilter` directive (e.g. `LAYANOW_LOG=layanow=debug`); the default is
/// `warn`. `LAYANOW_DEBUG` is kept as a shorthand for `layanow=debug`. Captured
/// text is never logged — only lengths and metadata.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = match std::env::var("LAYANOW_LOG").or_else(|_| std::env::var("RUST_LOG")) {
        Ok(value) => EnvFilter::new(value),
        Err(_) if std::env::var_os("LAYANOW_DEBUG").is_some() => EnvFilter::new("layanow=debug"),
        Err(_) => EnvFilter::new("warn"),
    };
    drop(tracing_subscriber::fmt().with_env_filter(filter).with_target(false).try_init());
}

/// What the command line asked for.
enum Invocation {
    /// Run the resident applet.
    Applet,
    /// Open the standalone settings window.
    Settings,
    /// Forward a command to a running applet.
    Command(Command),
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let Some(invocation) = parse_args(std::env::args().skip(1))? else {
        return Ok(());
    };
    match invocation {
        Invocation::Applet => run_applet(),
        Invocation::Settings => layanow_app::settings_window::run(),
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
        [word] if word == "settings" => Ok(Some(Invocation::Settings)),
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
           layanow settings   open the settings window\n\
           layanow --help     print this help"
    );
}

/// Run the resident applet until it quits.
fn run_applet() -> Result<(), Box<dyn std::error::Error>> {
    // Bind the control socket first: it is the single-instance lock (T-153) and
    // makes `layanow toggle` responsive while the model loads.
    let control = match control::start() {
        Ok(control) => control,
        Err(ControlError::AlreadyRunning) => {
            tracing::info!("already running");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    // Best-effort tray: without a StatusNotifier host the applet still works
    // from the `layanow` CLI (ADR-35).
    let _tray = match layanow_platform::tray::start(control.sender.clone()) {
        Ok(tray) => Some(tray),
        Err(error) => {
            tracing::warn!(%error, "tray unavailable");
            None
        }
    };
    let settings = Settings::load();
    let worker = build_worker(&settings);
    let resolver = layanow_platform::selection::resolver();
    let mut overlay = Overlay::new(worker, resolver, control.receiver);
    overlay.set_threshold(settings.confidence_threshold);
    let result = layanow_platform::overlay::run(overlay);
    // Process exit does not run the control thread's destructor, so unlink the
    // socket explicitly on a clean exit.
    control::cleanup();
    result?;
    Ok(())
}

/// Build the inference worker.
///
/// The engine is built lazily on the worker thread (via the factory), so the
/// applet starts immediately even before a multi-gigabyte model is ready and a
/// later settings change can swap the checkpoint without freezing the overlay
/// (T-113/T-117).
fn build_worker(settings: &Settings) -> Worker {
    let factory: EngineFactory = Box::new(|checkpoint_id, quant| {
        let spec = bundle::checkpoint(checkpoint_id)
            .or_else(|| bundle::checkpoint(bundle::DEFAULT_ID))
            .ok_or_else(|| "no checkpoints are registered".to_string())?;
        if spec.id != checkpoint_id {
            tracing::warn!(
                requested = checkpoint_id,
                using = spec.id,
                "unknown checkpoint; using the default"
            );
        }
        let variant = bundle::variant_or_default(spec, quant)
            .ok_or_else(|| format!("checkpoint {} has no graph variants", spec.id))?;
        if variant.quant != quant {
            tracing::warn!(
                requested = ?quant,
                using = ?variant.quant,
                "precision unavailable for this checkpoint; using the default"
            );
        }
        let dir = bundle::ensure_bundle(spec, variant).map_err(|error| error.to_string())?;
        tracing::info!(
            checkpoint = spec.id,
            quant = ?variant.quant,
            path = %dir.display(),
            "using bundle"
        );
        let checkpoint =
            bundle::checkpoint_from_dir(spec, variant, &dir).map_err(|error| error.to_string())?;
        let decider = Decider::load(&checkpoint).map_err(|error| error.to_string())?;
        Ok(Box::new(decider))
    });
    Worker::spawn(
        factory,
        settings.checkpoint.clone(),
        settings.quant,
        settings.unload == UnloadPolicy::OnDemand,
    )
}
