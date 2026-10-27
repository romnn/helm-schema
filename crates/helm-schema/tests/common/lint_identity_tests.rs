//! Shipped schema and values-file identities used by the battery and sweep.

use std::collections::BTreeSet;
use std::fs;

use color_eyre::eyre;
use helm_schema_test_support::source_digest::sha256_hex;
use indoc::indoc;
use serde_json::json;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

use super::lint_probes::{ImportedRoster, LintSchemas};

#[test]
fn baseline_binds_the_normalized_bytes_installed_for_helm() -> eyre::Result<()> {
    let baseline =
        br#" { "pattern": "^\\u0000\\u00FF$", "patternProperties": {"^\\u007F$": {}} } "#;
    let expected = br#"{"pattern":"^\\x00\\xFF$","patternProperties":{"^\\x7F$":{}}}"#;
    let schemas = LintSchemas::compile_bytes(baseline, &json!({}), None)?;
    sim_assert_eq!(have: sha256_hex(&schemas.baseline), want: sha256_hex(expected));
    sim_assert_eq!(have: schemas.baseline, want: expected.to_vec());
    Ok(())
}

#[test]
fn roster_distinguishes_original_file_hash_from_canonical_overlay_hash() -> eyre::Result<()> {
    let directory = ScratchDir::new("lint-identity")?;
    let path = directory.path().join("roster.json");
    let values_file = indoc! {r#"
        {
          "redis-ha": {"enabled": true}
        }
    "#};
    let overlay = json!({"redis-ha": {"enabled": true}});
    let canonical_sha256 = sha256_hex(&serde_json::to_vec(&overlay)?);
    let file_sha256 = sha256_hex(values_file.as_bytes());
    assert_ne!(canonical_sha256, file_sha256);
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version": 2,
            "rows": [{
                "id": "activation",
                "chart": "oauth2-proxy",
                "overlay": overlay,
                "canonical_overlay_sha256": canonical_sha256,
                "values_file": values_file,
                "values_file_sha256": file_sha256,
                "context_overlay": {},
                "context_overlay_sha256": sha256_hex(b"{}"),
                "transport": "values-file",
                "kubernetes_version": "1.29.0",
                "chart_sha256": "bound-at-screening",
                "baseline_sha256": "bound-at-screening",
                "candidate_sha256": "bound-at-screening"
            }]
        }))?,
    )?;
    let selected = BTreeSet::from(["oauth2-proxy".to_string()]);
    let imported = ImportedRoster::read(&path, &selected)?;
    sim_assert_eq!(have: imported.rows.len(), want: 1);
    Ok(())
}

#[test]
fn baseline_shortens_only_above_the_sweep_helm_file_limit() -> eyre::Result<()> {
    let mut exact_limit = vec![b' '; 5 * 1024 * 1024 - 2];
    exact_limit.extend_from_slice(b"{}");
    let exact = LintSchemas::compile_bytes(&exact_limit, &json!({}), None)?;
    sim_assert_eq!(have: exact.baseline, want: b"{}".to_vec());
    exact_limit.push(b'\n');
    let oversized = LintSchemas::compile_bytes(&exact_limit, &json!({}), None)?;
    sim_assert_eq!(have: oversized.baseline, want: b"{}\n".to_vec());
    Ok(())
}

#[test]
fn original_file_hash_and_parsed_overlay_must_both_match() -> eyre::Result<()> {
    let directory = ScratchDir::new("lint-identity")?;
    let path = directory.path().join("roster.json");
    let overlay = json!({"enabled": true});
    let original = "enabled: true\n";
    let row = json!({
        "id": "enabled", "chart": "control", "overlay": overlay,
        "canonical_overlay_sha256": sha256_hex(&serde_json::to_vec(&overlay)?),
        "values_file": original, "values_file_sha256": sha256_hex(original.as_bytes()),
        "context_overlay": {}, "context_overlay_sha256": sha256_hex(b"{}"),
        "transport": "values-file", "kubernetes_version": "1.29.0",
        "chart_sha256": "bound-at-screening", "baseline_sha256": "bound-at-screening",
        "candidate_sha256": "bound-at-screening"
    });
    let selected = BTreeSet::from(["control".to_string()]);
    fs::write(
        &path,
        serde_json::to_vec(&json!({"version": 2, "rows": [&row]}))?,
    )?;
    ImportedRoster::read(&path, &selected)?;

    // Identical parsed values cannot authorize different original file bytes under the old hash.
    let mut changed = row.clone();
    changed["values_file"] = json!("enabled: true\n\n");
    fs::write(
        &path,
        serde_json::to_vec(&json!({"version": 2, "rows": [changed]}))?,
    )?;
    assert!(ImportedRoster::read(&path, &selected).is_err());

    // A valid file hash cannot authorize a file whose parsed values differ from the claimed overlay.
    let mut changed = row;
    let different = "enabled: false\n";
    changed["values_file"] = json!(different);
    changed["values_file_sha256"] = json!(sha256_hex(different.as_bytes()));
    fs::write(
        &path,
        serde_json::to_vec(&json!({"version": 2, "rows": [changed]}))?,
    )?;
    assert!(ImportedRoster::read(&path, &selected).is_err());
    Ok(())
}
