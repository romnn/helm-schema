use clap::Parser;
use color_eyre::eyre;

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let cli = helm_schema_cli::Cli::parse();
    helm_schema_cli::run(cli).map_err(Into::into)
}
