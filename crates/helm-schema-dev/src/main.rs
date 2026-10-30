//! Development entry point extending the published CLI with Perfetto tracing.
//!
//! `HELM_SCHEMA_TRACE_FILTER` restricts tracing targets, as in the verification harness.

use std::fs::File;
use std::path::{Path, PathBuf};

use clap::{Arg, CommandFactory as _, FromArgMatches as _};
use helm_schema_cli::{Cli, CliError};

#[cfg(not(target_os = "windows"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<(), CliError> {
    let mut matches = Cli::command()
        .name("helm-schema-dev")
        .arg(
            Arg::new("trace-output")
                .long("trace-output")
                .value_name("FILE")
                .value_parser(clap::value_parser!(PathBuf))
                .global(true)
                .help("Write a Perfetto-readable trace file for the run"),
        )
        .get_matches();
    let trace_output = matches.remove_one::<PathBuf>("trace-output");
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|err| err.exit());
    if let Some(path) = trace_output {
        let dispatch = trace_dispatch(&path)?;
        return tracing::dispatcher::with_default(&dispatch, || helm_schema_cli::execute(cli));
    }
    helm_schema_cli::execute(cli)
}

fn trace_dispatch(path: &Path) -> Result<tracing::Dispatch, CliError> {
    let file = create_trace_file(path)?;
    match std::env::var("HELM_SCHEMA_TRACE_FILTER") {
        Ok(filter) => {
            let targets = filter
                .parse::<tracing_subscriber::filter::Targets>()
                .map_err(|err| {
                    CliError::CliValidation(format!("invalid perfetto target filter: {err}"))
                })?;
            Ok(helm_schema_dev::trace::perfetto_dispatch_with_targets(
                file, targets,
            ))
        }
        Err(std::env::VarError::NotPresent) => Ok(helm_schema_dev::trace::perfetto_dispatch(file)),
        Err(std::env::VarError::NotUnicode(_)) => Err(CliError::CliValidation(
            "perfetto target filter is not unicode".into(),
        )),
    }
}

fn create_trace_file(path: &Path) -> Result<File, CliError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CliError::CreateOutputDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    File::create(path).map_err(|source| CliError::WriteOutput {
        path: path.to_path_buf(),
        source,
    })
}
