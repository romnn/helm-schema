// use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use color_eyre::eyre;
use helm_schema_chart::{LoadOptions, load_chart};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "helmtsg",
    version,
    about = "Helm Template Schema Generator (Phase 1: chart scan)"
)]
struct Cli {
    /// Path to the chart root
    #[arg(global = true, short, long, default_value = ".")]
    path: PathBuf,

    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Scan chart and print a summary as JSON
    Scan {
        /// Include files under tests/
        #[arg(long)]
        include_tests: bool,
    },
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt().with_env_filter("info").init();

    let cli = Cli::parse();

    let fs = vfs::PhysicalFS::new(cli.path);
    let root = vfs::VfsPath::new(fs);
    // let root = fs.join(cli.path.to_string_lossy().as_ref())?;
    // let root =
    //     Utf8PathBuf::try_from(cli.path).wrap_err_with(|| eyre::eyre!("path must be UTF-8"))?;

    match cli.cmd {
        Command::Scan { include_tests } => {
            let summary = load_chart(
                &root,
                &LoadOptions {
                    include_tests,
                    ..Default::default()
                },
            )?;
            println!("{summary:?}");
        }
    }
    Ok(())
}
