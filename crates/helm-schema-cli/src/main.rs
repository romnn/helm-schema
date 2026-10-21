//! Executable entry point for the `helm-schema` command.

use clap::Parser;

#[cfg(not(target_os = "windows"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<(), helm_schema_cli::CliError> {
    helm_schema_cli::execute(helm_schema_cli::Cli::parse())
}
