//! Small clippy shim

fn main() -> std::process::ExitCode {
    clippy_shim::run(std::env::args_os())
}

