use nix_output_monitor::cli::run_app;
use nix_output_monitor::terminal::install_signal_handlers;
use std::process::ExitCode;

fn main() -> ExitCode {
    let _ = install_signal_handlers();
    run_app()
}
