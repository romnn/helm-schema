use std::ffi::OsString;
use std::path::PathBuf;

use clap::Args;
use helm_schema::helm::{HelmCommand, HelmRunOptions};

/// Arguments of `helm-schema lint` and `helm-schema template`.
#[derive(Args, Debug, Clone)]
pub struct HelmArgs {
    /// The Helm executable to run.
    #[arg(long, env = "HELM", value_name = "PATH", default_value = "helm")]
    pub helm: PathBuf,

    /// The chart directory, then Helm arguments, passed verbatim.
    ///
    /// Everything after the chart goes to Helm, including `--help`; one `--`
    /// right after the chart is dropped.
    #[arg(
        value_name = "CHART [HELM_ARGS]",
        required = true,
        num_args = 1..,
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub chart_and_args: Vec<OsString>,
}

impl HelmArgs {
    /// The Helm run these arguments describe; `None` without a chart.
    pub(crate) fn run_options(
        &self,
        command: HelmCommand,
        scratch_root: PathBuf,
    ) -> Option<HelmRunOptions> {
        let (chart, args) = self.chart_and_args.split_first()?;
        let args = match args {
            [separator, rest @ ..] if *separator == "--" => rest,
            _ => args,
        };
        Some(HelmRunOptions {
            command,
            helm: self.helm.clone(),
            chart: PathBuf::from(chart),
            args: args.to_vec(),
            scratch_root,
        })
    }
}
