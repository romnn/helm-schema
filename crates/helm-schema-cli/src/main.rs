//! Executable entry point for the `helm-schema` command.

use clap::Parser;

#[cfg(not(target_os = "windows"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::result::Result<(), helm_schema_cli::CliError> {
    let cli = helm_schema_cli::Cli::parse();
    helm_schema_cli::run(cli)
}
