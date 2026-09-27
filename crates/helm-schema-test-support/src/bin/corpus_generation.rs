//! Writes every registered corpus artifact and the manifest describing them.
//!
//! - `corpus_generation --out <dir> [--jobs <n>]` generates the whole registry
//!   and publishes `<dir>/manifest.json` last. Point tests at it with
//!   `HELM_SCHEMA_CORPUS_ARTIFACTS=<dir>`.
//! - `corpus_generation --out <dir> [--jobs <n>] --only <key> [--only <key>]...`
//!   generates just those artifacts and prints their manifest entries as JSON
//!   lines. It never writes a manifest; it exists for timing comparisons.
//! - `--helm-ready` also writes, for every schema artifact Helm reads, the
//!   Helm-ready copy `internal/<name>.helm.schema.json` (short `$defs` keys,
//!   compact JSON) and its name map `internal/<name>.defs-map.json`, recorded
//!   in the artifact's manifest entry. `<name>` is the artifact file name
//!   without `.schema.json`.
//! - `corpus_generation --verify <dir>` re-checks a published manifest
//!   completely: provenance, registry membership, every recipe's inputs and
//!   every artifact's bytes. The landing runner runs it before adoption.

use std::path::PathBuf;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_test_support::manifest::{self, BuildProvenance, MANIFEST_FILE};
use helm_schema_test_support::{generate, registry};

const USAGE: &str = "usage: corpus_generation (--out <dir> [--jobs <n>] [--helm-ready] \
     [--only <key>]... | --verify <dir>)";

fn main() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let mut out = None;
    let mut verify = None;
    let mut only = Vec::new();
    let mut helm_ready = false;
    let mut jobs = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_eyre(USAGE);
        match arg.as_str() {
            "--out" => out = Some(PathBuf::from(value()?)),
            "--verify" => verify = Some(PathBuf::from(value()?)),
            "--only" => only.push(value()?),
            "--jobs" => jobs = value()?.parse().wrap_err("parse --jobs")?,
            "--helm-ready" => helm_ready = true,
            other => eyre::bail!("unknown argument {other:?}; {USAGE}"),
        }
    }

    if let Some(dir) = verify {
        let manifest = manifest::load(&dir)?;
        manifest::verify_all(&dir, &manifest)?;
        println!(
            "verified {} artifacts in {}",
            manifest.artifacts.len(),
            dir.display()
        );
        return Ok(());
    }
    let out = out.ok_or_eyre(USAGE)?;
    if !only.is_empty() {
        let specs = registry::registry()
            .into_iter()
            .filter(|spec| only.contains(&spec.id.key()))
            .collect::<Vec<_>>();
        eyre::ensure!(
            specs.len() == only.len(),
            "unknown or repeated --only key among {only:?}"
        );
        BuildProvenance::compiled().check_current()?;
        for entry in generate::produce_entries(&specs, &out, jobs, helm_ready)? {
            println!("{}", serde_json::to_string(&entry)?);
        }
        return Ok(());
    }
    let manifest = generate::produce(&out, jobs, &BuildProvenance::compiled(), helm_ready)?;
    println!(
        "wrote {} artifacts and {}",
        manifest.artifacts.len(),
        out.join(MANIFEST_FILE).display()
    );
    Ok(())
}
