//! Replays saved Helm documents against a verified producer dump using pinned Draft 7.

use std::path::PathBuf;

use helm_schema_test_support::corpus_replay::{self, Baseline, Options, ReplayError};

const USAGE: &str = "corpus_replay --candidate <dump> (--baseline-dump <dump> | --baseline-ref <ref>) \
    --evidence <preview-root> --out <directory> [--census <placement-census.json>] \
    [--jobs <n>] [--include-unchanged]";

fn main() -> Result<(), ReplayError> {
    let mut candidate = None;
    let mut baseline = None;
    let mut evidence = None;
    let mut census = None;
    let mut out = None;
    let mut jobs = 4;
    let mut include_unchanged = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!("{USAGE}");
            return Ok(());
        }
        if arg == "--include-unchanged" {
            include_unchanged = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| ReplayError::Input(USAGE.into()))?;
        match arg.as_str() {
            "--candidate" => candidate = Some(PathBuf::from(value)),
            "--baseline-dump" if baseline.is_none() => {
                baseline = Some(Baseline::Dump(value.into()));
            }
            "--baseline-ref" if baseline.is_none() => baseline = Some(Baseline::Git(value)),
            "--evidence" => evidence = Some(PathBuf::from(value)),
            "--census" => census = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            "--jobs" => {
                jobs = value
                    .parse()
                    .map_err(|_| ReplayError::Input("invalid jobs".into()))?;
            }
            _ => return Err(ReplayError::Input(USAGE.into())),
        }
    }
    let missing = || ReplayError::Input(USAGE.into());
    let options = Options {
        candidate: candidate.ok_or_else(missing)?,
        baseline: baseline.ok_or_else(missing)?,
        evidence: evidence.ok_or_else(missing)?,
        census,
        include_unchanged,
        jobs,
    };
    let out = out.ok_or_else(missing)?;
    let report = corpus_replay::run(&options)?;
    std::fs::create_dir_all(&out).map_err(|source| ReplayError::Io {
        path: out.clone(),
        source,
    })?;
    let path = out.join("replay.json");
    let mut bytes = serde_json::to_vec_pretty(&report).map_err(|source| ReplayError::Json {
        path: path.clone(),
        source,
    })?;
    bytes.push(b'\n');
    std::fs::write(&path, bytes).map_err(|source| ReplayError::Io { path, source })?;
    let path = out.join("replay.md");
    std::fs::write(&path, report.markdown()).map_err(|source| ReplayError::Io { path, source })?;
    println!(
        "charts={} documents={} fa_closed={} fa_reopened={} new_fr_candidates={}",
        report.charts.len(),
        report.counts.documents,
        report.counts.fa_closed,
        report.counts.fa_reopened,
        report.counts.new_fr_candidates
    );
    Ok(())
}
