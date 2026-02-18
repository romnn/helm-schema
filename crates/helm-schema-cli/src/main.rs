use clap::Parser;
use color_eyre::eyre;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let cli = helm_schema_cli::Cli::parse();
    helm_schema_cli::run(cli).map_err(Into::into)
}
