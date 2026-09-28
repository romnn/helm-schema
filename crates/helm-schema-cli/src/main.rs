//! Executable entry point for the `helm-schema` command.

use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use clap::Parser;
use helm_schema_cli::cli::Command;
use signal_hook::consts::TERM_SIGNALS;

#[cfg(not(target_os = "windows"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::result::Result<(), helm_schema_cli::CliError> {
    let cli = helm_schema_cli::Cli::parse();
    // A termination signal, or a console Ctrl-C, stops the Helm that `lint`
    // and `template` run; once its chart copy is gone, the process ends by
    // that signal.
    let interrupt = Arc::new(AtomicUsize::new(0));
    if matches!(cli.command, Some(Command::Lint(_) | Command::Template(_))) {
        for &signal in TERM_SIGNALS {
            let number = usize::try_from(signal).unwrap_or_default();
            signal_hook::flag::register_usize(signal, Arc::clone(&interrupt), number)?;
        }
    }
    let outcome = helm_schema_cli::run(cli, &interrupt);
    if let Ok(signal @ 1..) = i32::try_from(interrupt.load(Ordering::SeqCst)) {
        exit_by_signal(signal);
    }
    match outcome? {
        None => Ok(()),
        Some(status) => exit_like(status),
    }
}

/// Ends the process the way Helm ended: with its exit code, or on Unix by
/// its signal.
fn exit_like(status: ExitStatus) -> ! {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        exit_by_signal(signal);
    }
    std::process::exit(status.code().unwrap_or(1))
}

/// Ends the process by `signal`'s default action, as if it had not been
/// caught; exits with `128 + signal` where that action does not terminate.
fn exit_by_signal(signal: i32) -> ! {
    let _ = signal_hook::low_level::emulate_default_handler(signal);
    std::process::exit(128 + signal)
}
