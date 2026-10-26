//! The value-algebra differential gate; see
//! [`helm_schema_test_support::value_algebra_diff`].
//!
//! ```text
//! value_algebra_diff capture --checkout DIR --commit SHA --helmsweep PATH --helm PATH
//!                            --build-target DIR --controls DIR --contexts FILE
//!                            [--synthetic] --out NEW_DIR
//! value_algebra_diff compare --self-test --main CAPTURE --candidate CAPTURE --out NEW_DIR
//!                            [--helmsweep PATH] [--chart LABEL]... [--certificates FILE]
//! value_algebra_diff init --main CAPTURE --plan HISTORY_PLAN --out NEW_HISTORY_DIR
//!                         [--helmsweep PATH]
//! value_algebra_diff compare --main CAPTURE --candidate CAPTURE --history DIR --previous RECEIPT
//!                            --ledger FILE --out NEW_DIR --helmsweep PATH [--seed-review FILE]
//!                            [--certificates FILE]
//! value_algebra_diff amend --history DIR --previous RECEIPT --witness-batch SHA --review FILE
//!                          --out NEW_DIR --helmsweep PATH
//! value_algebra_diff verify (--capture DIR | --history DIR --receipt FILE)
//! value_algebra_diff ground --cases FILE --helm PATH --out NEW_REGISTRY_DIR
//! ```
//!
//! `ground` builds a control registry from a JSON list of
//! `{"id", "chart", "query"}` cases, each grounded by the pinned Helm.
//!
//! Exit codes: 0 complete and policy-satisfied; 1 semantic, unadjudicated
//! or monotonicity failure; 2 invalid request, identity or chain; 3
//! execution, provenance or coverage infrastructure failure. Every
//! construction success receipt says NON-LANDABLE. `--landing` and
//! `migrate` are refused: final gates and history migration are not built.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::ExitCode;

use helm_schema_test_support::value_algebra_diff::commands::{
    self, CompareRequest, LiveOptions, Verdict,
};
use helm_schema_test_support::value_algebra_diff::controls;
use helm_schema_test_support::value_algebra_diff::revision::{self, CaptureRequest};
use helm_schema_test_support::value_algebra_diff::{DiffError, EquivalenceCertificate};

const USAGE: &str = "usage: value_algebra_diff (capture|compare|init|amend|verify|ground) [options]; \
     see the binary's documentation";

fn main() -> ExitCode {
    let _guard = test_util::builder().with_tracing(false).build();
    match run() {
        Ok(Verdict::Pass) => ExitCode::SUCCESS,
        Ok(Verdict::Fail(problems)) => {
            for problem in problems {
                eprintln!("value_algebra_diff: {problem}");
            }
            ExitCode::from(1)
        }
        Err(error @ (DiffError::Request(_) | DiffError::Identity(_))) => {
            eprintln!("value_algebra_diff: {error}");
            ExitCode::from(2)
        }
        Err(error @ DiffError::Infrastructure(_)) => {
            eprintln!("value_algebra_diff: {error}");
            ExitCode::from(3)
        }
    }
}

/// Parsed options: flags and repeated values by name.
struct Options {
    command: String,
    values: BTreeMap<String, Vec<String>>,
    flags: BTreeSet<String>,
}

const FLAGS: &[&str] = &["--self-test", "--synthetic", "--landing"];

fn options() -> Result<Options, DiffError> {
    let mut args = std::env::args().skip(1);
    let command = args
        .next()
        .ok_or_else(|| DiffError::Request(USAGE.to_string()))?;
    let mut values = BTreeMap::<String, Vec<String>>::new();
    let mut flags = BTreeSet::new();
    while let Some(arg) = args.next() {
        if FLAGS.contains(&arg.as_str()) {
            flags.insert(arg);
        } else if arg.starts_with("--") {
            let value = args
                .next()
                .ok_or_else(|| DiffError::Request(format!("{arg} needs a value; {USAGE}")))?;
            values.entry(arg).or_default().push(value);
        } else {
            return Err(DiffError::Request(format!("unexpected {arg:?}; {USAGE}")));
        }
    }
    Ok(Options {
        command,
        values,
        flags,
    })
}

impl Options {
    fn optional(&mut self, name: &str) -> Result<Option<String>, DiffError> {
        match self.values.remove(name).as_deref() {
            None => Ok(None),
            Some([value]) => Ok(Some(value.clone())),
            Some(_) => Err(DiffError::Request(format!("{name} given twice"))),
        }
    }

    fn path(&mut self, name: &str) -> Result<PathBuf, DiffError> {
        self.optional_path(name)?
            .ok_or_else(|| DiffError::Request(format!("{name} is required; {USAGE}")))
    }

    /// The named path made absolute, so child processes that run in another
    /// directory resolve it the same way.
    fn optional_path(&mut self, name: &str) -> Result<Option<PathBuf>, DiffError> {
        self.optional(name)?
            .map(|value| {
                std::path::absolute(&value)
                    .map_err(|error| DiffError::Request(format!("{name} {value:?}: {error}")))
            })
            .transpose()
    }

    fn finish(self) -> Result<(), DiffError> {
        match self.values.keys().next() {
            Some(name) => Err(DiffError::Request(format!(
                "{name} does not apply to {}",
                self.command
            ))),
            None => Ok(()),
        }
    }

    fn live(&mut self, out: &std::path::Path) -> Result<LiveOptions, DiffError> {
        let charts = self.values.remove("--chart").unwrap_or_default();
        Ok(LiveOptions {
            helmsweep: self.optional_path("--helmsweep")?,
            selection: (!charts.is_empty()).then(|| charts.into_iter().collect()),
            scratch: out.join("scratch"),
        })
    }

    fn capture_request(&mut self) -> Result<CaptureRequest, DiffError> {
        Ok(CaptureRequest {
            checkout: self.path("--checkout")?,
            commit: self
                .optional("--commit")?
                .ok_or_else(|| DiffError::Request("--commit is required".into()))?,
            helmsweep: self.path("--helmsweep")?,
            helm: self.path("--helm")?,
            build_target: self.path("--build-target")?,
            controls: self.path("--controls")?,
            contexts: self.path("--contexts")?,
            out: self.path("--out")?,
            synthetic: self.flags.contains("--synthetic"),
        })
    }

    fn certificates(&mut self) -> Result<Vec<EquivalenceCertificate>, DiffError> {
        let Some(path) = self.optional_path("--certificates")? else {
            return Ok(Vec::new());
        };
        let bytes = std::fs::read(&path)
            .map_err(|error| DiffError::Request(format!("{}: {error}", path.display())))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| DiffError::Request(format!("{}: {error}", path.display())))
    }
}

fn run() -> Result<Verdict, DiffError> {
    let mut options = options()?;
    let verdict = match options.command.as_str() {
        "capture" => {
            let request = options.capture_request()?;
            options.finish()?;
            revision::capture(&request).map(|_| Verdict::Pass)?
        }
        "compare" if options.flags.contains("--self-test") => {
            let (main, candidate, out) = (
                options.path("--main")?,
                options.path("--candidate")?,
                options.path("--out")?,
            );
            let live = options.live(&out)?;
            let certificates = options.certificates()?;
            options.finish()?;
            commands::self_test(&main, &candidate, &out, &live, &certificates)?
        }
        "compare" => {
            if options.flags.contains("--landing") {
                return Err(DiffError::Request(
                    "--landing needs the final gate receipts; not available yet".into(),
                ));
            }
            let out = options.path("--out")?;
            let request = CompareRequest {
                main: options.path("--main")?,
                candidate: options.path("--candidate")?,
                history: options.path("--history")?,
                previous: options.path("--previous")?,
                ledger: options.path("--ledger")?,
                seed_review: options.optional_path("--seed-review")?,
                certificates: options.certificates()?,
                live: options.live(&out)?,
                out,
            };
            options.finish()?;
            commands::compare(&request)?
        }
        "init" => {
            let (main, plan, out) = (
                options.path("--main")?,
                options.path("--plan")?,
                options.path("--out")?,
            );
            let live = options.live(&out)?;
            options.finish()?;
            commands::init(&main, &plan, &out, &live)?
        }
        "amend" => {
            let (history, previous, review, out) = (
                options.path("--history")?,
                options.path("--previous")?,
                options.path("--review")?,
                options.path("--out")?,
            );
            let batch = options
                .optional("--witness-batch")?
                .ok_or_else(|| DiffError::Request("--witness-batch is required".into()))?;
            let live = options.live(&out)?;
            options.finish()?;
            commands::amend(&history, &previous, &batch, &review, &out, &live)?
        }
        "verify" => {
            let capture = options.optional_path("--capture")?;
            let history = options.optional_path("--history")?;
            let receipt = options.optional_path("--receipt")?;
            options.finish()?;
            match (capture, history, receipt) {
                (Some(capture), None, None) => commands::verify_capture(&capture)?,
                (None, Some(history), Some(receipt)) => {
                    commands::verify_history(&history, &receipt)?
                }
                _ => return Err(DiffError::Request(USAGE.into())),
            }
        }
        "ground" => {
            let (cases, helm, out) = (
                options.path("--cases")?,
                options.path("--helm")?,
                options.path("--out")?,
            );
            options.finish()?;
            controls::ground_registry(&cases, &helm, &out).map(|_| Verdict::Pass)?
        }
        "migrate" => {
            return Err(DiffError::Request(
                "migrate (history migration) is not available yet".into(),
            ));
        }
        other => {
            return Err(DiffError::Request(format!(
                "unknown command {other:?}; {USAGE}"
            )));
        }
    };
    Ok(verdict)
}
