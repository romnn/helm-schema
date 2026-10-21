//! Pinned generator output for charts absent from a comparison commit's corpus.

use std::fs;
use std::path::Path;
use std::process::Command;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_test_support::manifest::InputDigester;
use helm_schema_test_support::registry::{self, ArtifactTarget};
use helm_schema_test_support::source_digest::sha256_hex;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    reference: String,
    fixture: String,
    file: String,
    sha256: String,
    inputs_sha256: String,
    recipe: Value,
    generator_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Intake {
    version: u32,
    entries: Vec<Entry>,
}

pub(super) fn read_baseline(reference: &str, fixture: &str) -> eyre::Result<Vec<u8>> {
    let object = format!("{reference}:{fixture}");
    let output = Command::new("git").args(["show", &object]).output()?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    // Only a genuinely absent path may use intake data; invalid refs and Git failures remain errors.
    let tree = Command::new("git")
        .args(["ls-tree", "--name-only", reference, "--", fixture])
        .output()?;
    eyre::ensure!(
        tree.status.success() && tree.stdout.is_empty(),
        "cannot read baseline {object}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commit = Command::new("git")
        .args(["rev-parse", "--verify", &format!("{reference}^{{commit}}")])
        .output()?;
    eyre::ensure!(
        commit.status.success(),
        "invalid baseline commit {reference}"
    );
    let commit = std::str::from_utf8(&commit.stdout)?.trim();
    let root = test_util::workspace_root();
    let intake = load(&root.join("testdata/acceptance-intake/manifest.json"))?;
    let entry = select(&intake.entries, commit, fixture)?;
    read_entry(entry, &root)
}

fn load(path: &Path) -> eyre::Result<Intake> {
    let intake: Intake = serde_json::from_slice(
        &fs::read(path).wrap_err_with(|| format!("read intake baselines {}", path.display()))?,
    )?;
    eyre::ensure!(
        intake.version == 1,
        "unsupported intake baseline version {}",
        intake.version
    );
    Ok(intake)
}

fn select<'a>(entries: &'a [Entry], reference: &str, fixture: &str) -> eyre::Result<&'a Entry> {
    let mut matches = entries
        .iter()
        .filter(|entry| entry.reference == reference && entry.fixture == fixture);
    let entry = matches
        .next()
        .ok_or_else(|| eyre::eyre!("no pinned intake baseline for {reference}:{fixture}"))?;
    eyre::ensure!(
        matches.next().is_none(),
        "duplicate intake baseline for {reference}:{fixture}"
    );
    Ok(entry)
}

fn read_entry(entry: &Entry, root: &Path) -> eyre::Result<Vec<u8>> {
    let Entry {
        reference,
        fixture,
        file,
        sha256,
        inputs_sha256,
        recipe,
        generator_sha256,
    } = entry;
    eyre::ensure!(
        reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "intake baseline must name a full commit"
    );
    eyre::ensure!(
        generator_sha256.len() == 64
            && generator_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
        "intake baseline must pin the generator binary"
    );
    eyre::ensure!(
        registry::is_safe_relative_path(file) && file.starts_with("testdata/acceptance-intake/"),
        "unsafe intake baseline path {file}"
    );
    let spec = registry::registry()
        .into_iter()
        .find(|spec| spec.target == ArtifactTarget::Fixture(fixture.clone()))
        .ok_or_eyre("intake baseline fixture is not registered")?;
    eyre::ensure!(
        spec.recipe.describe() == *recipe,
        "stale intake baseline recipe for {fixture}"
    );
    let inputs = InputDigester::new(root.join("testdata")).digest(&spec.recipe)?;
    eyre::ensure!(
        inputs == *inputs_sha256,
        "stale intake baseline inputs for {fixture}"
    );
    let bytes = fs::read(root.join(file))?;
    eyre::ensure!(
        sha256_hex(&bytes) == *sha256,
        "intake baseline bytes changed for {fixture}"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests;
