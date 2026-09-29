//! Writes (or `--check`s) a reproducible verdict matrix; see
//! [`helm_schema_test_support::cell_matrix`].
//!
//! ```text
//! cell_matrix --cells <cells.tsv> --bin <LABEL>=<PATH>@<COMMIT>... --helm <helm> --helmsweep <helmsweep>
//!             --out <dir> [--bundle <dir>] [--gen-k8s-version <v>] [--profile <p>]
//!             [--validate-release <r>] [--timeout <seconds>] [--scratch <dir>] [--check]
//!             [--require-expected]
//! ```
//!
//! `--scratch <dir>` puts every scratch file, the Helm replay store included, in `dir` instead of the
//! target's `scratch/`: with it, `--check` writes nothing under `--out` or the checkout, so a reviewer
//! can run it from a read-only checkout. `dir` must already exist and be writable (for example
//! `$TMPDIR/cm-check`, made with `mkdir` by whoever prepares the sandbox) and must not lie under `--out`;
//! the tool never creates it. `--validate-release` and
//! `--gen-k8s-version` must name a release directory of the bundle's Kubernetes cache.
//!
//! Exit codes: 0 written or reproduced with every expectation holding; 1 `--check` found
//! other bytes or an expectation failed; 2 refused input (nothing written); 3 harness failure
//! (a crashing binary, an uncompilable schema, a Helm execution failure, a generation, Helm or
//! helmsweep execution exceeding `--timeout`, 600 seconds by default, named with its command, chart
//! and cells; nothing written).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use color_eyre::eyre;
use helm_schema_test_support::cell_matrix::{self, Bin, Options, Outcome, Refusal};
use helm_schema_test_support::helm::adjudication::KUBERNETES_RELEASE;
use helm_schema_test_support::registry::{BUNDLE_K8S_VERSION, PROVIDER_BUNDLE};

const USAGE: &str = "usage: cell_matrix --cells <cells.tsv> --bin <LABEL>=<PATH>@<COMMIT>... \
     --helm <helm> --helmsweep <helmsweep> --out <dir> [--bundle <dir>] [--gen-k8s-version <v>] \
     [--profile <p>] [--validate-release <r>] [--timeout <seconds>] [--scratch <dir>] [--check] \
     [--require-expected]";

fn main() -> ExitCode {
    let result = options().and_then(|options| cell_matrix::run(&options));
    match result {
        Ok(Outcome::Ok) => ExitCode::SUCCESS,
        Ok(Outcome::Mismatch(reasons)) => {
            for reason in reasons {
                eprintln!("cell_matrix: {reason}");
            }
            ExitCode::from(1)
        }
        Err(error) if error.downcast_ref::<Refusal>().is_some() => {
            eprintln!("cell_matrix: {error:#}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("cell_matrix: harness failure: {error:?}");
            ExitCode::from(3)
        }
    }
}

fn options() -> eyre::Result<Options> {
    let mut cells = None;
    let mut bins = Vec::new();
    let mut helm = None;
    let mut helmsweep = None;
    let mut out = None;
    let mut bundle = test_util::workspace_testdata().join(PROVIDER_BUNDLE);
    let mut gen_k8s_version = BUNDLE_K8S_VERSION.to_string();
    let mut profile = "full".to_string();
    let mut validate_release = KUBERNETES_RELEASE.to_string();
    let mut check = false;
    let mut require_expected = false;
    let mut timeout = Duration::from_secs(600);
    let mut scratch = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| eyre::Report::new(Refusal(USAGE.to_string())))
        };
        match arg.as_str() {
            "--cells" => cells = Some(PathBuf::from(value()?)),
            "--bin" => bins.push(parse_bin(&value()?)?),
            "--helm" => helm = Some(PathBuf::from(value()?)),
            "--helmsweep" => helmsweep = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--bundle" => bundle = PathBuf::from(value()?),
            "--gen-k8s-version" => gen_k8s_version = value()?,
            "--profile" => profile = value()?,
            "--validate-release" => validate_release = value()?,
            "--timeout" => {
                let seconds = value()?;
                timeout = match seconds.parse::<u64>() {
                    Ok(seconds) if seconds > 0 => Duration::from_secs(seconds),
                    _ => {
                        return Err(Refusal(format!(
                            "--timeout {seconds:?} is not a positive number of seconds"
                        ))
                        .into());
                    }
                };
            }
            "--scratch" => scratch = Some(PathBuf::from(value()?)),
            "--check" => check = true,
            "--require-expected" => require_expected = true,
            other => return Err(Refusal(format!("unknown argument {other:?}; {USAGE}")).into()),
        }
    }
    let missing = || eyre::Report::new(Refusal(USAGE.to_string()));
    Ok(Options {
        cells: cells.ok_or_else(missing)?,
        bins,
        helm: helm.ok_or_else(missing)?,
        helmsweep: helmsweep.ok_or_else(missing)?,
        bundle,
        gen_k8s_version,
        profile,
        validate_release,
        out: out.ok_or_else(missing)?,
        check,
        require_expected,
        timeout,
        scratch,
    })
}

/// `LABEL=PATH@COMMIT`.
fn parse_bin(spec: &str) -> eyre::Result<Bin> {
    let refused = || eyre::Report::new(Refusal(format!("--bin {spec:?} is not LABEL=PATH@COMMIT")));
    let (label, rest) = spec.split_once('=').ok_or_else(refused)?;
    let (path, commit) = rest.rsplit_once('@').ok_or_else(refused)?;
    if label.is_empty()
        || !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(refused());
    }
    Ok(Bin {
        label: label.to_string(),
        path: PathBuf::from(path),
        commit: commit.to_string(),
    })
}
