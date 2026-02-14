use clap::Parser;
use color_eyre::eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = helm_schema_cli::Cli::parse();
    helm_schema_cli::run(cli)
}
