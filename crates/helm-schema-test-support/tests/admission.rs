//! The admission suite's reading, roster, obligation and coalesced-document consistency,
//! classification and re-recording, over a two-cell synthetic suite.

use std::fs;
use std::path::Path;

use color_eyre::eyre;
use helm_schema_test_support::admission::{
    Admission, Suite, Verdict, classify, compose, document_sha256, inputs_sha256, read_suite,
    refresh,
};
use indoc::indoc;
use serde_json::{Value, json};
use test_util::helm_values::coalesce_chart_values;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

const OBLIGATIONS_HEADER: &str = "cell_id\tlane\tdeclared_types\thelm_class\tmain\terror\towner\n";
const CELLS: &str = indoc! {"
    cell_id\tchart\tvalues\tkube_version\texpect
    lane.bad.0\tcharts/c\t{}\t1.29.0\thelm_class=reject main=accept
    lane.ok.0\tcharts/c\t{}\t1.29.0\thelm_class=pass main=accept
"};
const ASSERT_CELLS: &str = indoc! {"
    cell_id\tchart\tvalues\tkube_version\texpect
    lane.strict.0\tcharts/c\t{}\t1.29.0\thelm_class=pass main=accept
"};
const ROSTER: &str = indoc! {"
    cell_id\tdeclared_types
    lane.bad.0\tannotate
    lane.ok.0\tannotate
    lane.strict.0\tassert
"};
const BAD_ROW: &str = "lane.bad.0\tlane\tannotate\treject\taccept\tfalse-accept\tADR-0042\n";
const MATRIX_HEADER: &str =
    "cell_id\tprep_sha256\tvalues_src\tmain\thelm_rc\thelm_class\thelm_abort\tk8s\texpect_ok";

/// A suite with `lane.ok.0` (main right) and `lane.bad.0` (main wrong) in the annotate file,
/// `lane.strict.0` in the assert file, the given cells and obligations, and a `coalesced.tsv`
/// recording the Rust port's documents as Helm's.
fn suite(root: &Path, cells: &str, obligations: &str) -> eyre::Result<Suite> {
    fs::create_dir_all(root.join("charts/c"))?;
    fs::write(
        root.join("charts/c/Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: c
            version: 0.1.0
        "},
    )?;
    fs::write(root.join("charts/c/values.yaml"), "a: 1\n")?;
    fs::write(root.join("cells.tsv"), cells)?;
    fs::write(root.join("cells-assert.tsv"), ASSERT_CELLS)?;
    fs::write(root.join("roster.tsv"), ROSTER)?;
    fs::write(
        root.join("obligations.tsv"),
        format!("{OBLIGATIONS_HEADER}{obligations}"),
    )?;
    let mut table = String::from("cell_id\tinputs_sha256\tdocument_sha256\n");
    let document = coalesce_chart_values(&root.join("charts/c"), json!({}))?;
    for cell in read_suite(root)?.cells {
        let row = format!(
            "{}\t{}\t{}\n",
            cell.id,
            inputs_sha256(root, &cell)?,
            document_sha256(&document)?
        );
        table.push_str(&row);
    }
    fs::write(root.join("coalesced.tsv"), table)?;
    read_suite(root)
}

/// `text` without the lines starting with `prefix`.
fn without_rows(text: &str, prefix: &str) -> String {
    let mut out = String::new();
    for line in text.lines().filter(|line| !line.starts_with(prefix)) {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[test]
fn obligations_must_be_exactly_the_cells_main_gets_wrong() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    let root = scratch.path();
    sim_assert_eq!(have: suite(root, CELLS, BAD_ROW)?.obligation_mismatches(), want: Vec::<String>::new());
    sim_assert_eq!(
        have: suite(root, CELLS, "")?.obligation_mismatches(),
        want: vec!["lane.bad.0: main disagrees with Helm, but obligations.tsv omits it".to_string()],
    );
    let extra = format!("{BAD_ROW}lane.ok.0\tlane\tannotate\tpass\taccept\tfalse-accept\tR1-R3\n");
    sim_assert_eq!(
        have: suite(root, CELLS, &extra)?.obligation_mismatches(),
        want: vec!["lane.ok.0: listed in obligations.tsv, but main agrees with Helm".to_string()],
    );
    let stale = "lane.bad.0\tlane\tannotate\treject\treject\tfalse-reject\tR1-R3\n";
    eyre::ensure!(
        suite(root, CELLS, stale)?.obligation_mismatches().len() == 1,
        "a stale row restating another verdict is a mismatch"
    );
    Ok(())
}

#[test]
fn a_verdict_away_from_helm_is_new_and_one_toward_helm_is_closed() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    let suite = suite(scratch.path(), CELLS, BAD_ROW)?;
    let [bad, ok, _] = suite.cells.as_slice() else {
        eyre::bail!("three cells");
    };
    sim_assert_eq!(have: classify(ok, Verdict::Accept), want: Admission::Kept);
    sim_assert_eq!(have: classify(ok, Verdict::Reject), want: Admission::New);
    sim_assert_eq!(have: classify(bad, Verdict::Accept), want: Admission::Kept);
    sim_assert_eq!(have: classify(bad, Verdict::Reject), want: Admission::Closed);
    Ok(())
}

/// Removing a cell, or emptying the tables, is a roster mismatch until `roster.tsv` changes too.
#[test]
fn the_cells_must_be_exactly_the_frozen_roster() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    let root = scratch.path();
    sim_assert_eq!(have: suite(root, CELLS, BAD_ROW)?.roster_mismatches(), want: Vec::<String>::new());
    let without_ok = without_rows(CELLS, "lane.ok.0");
    sim_assert_eq!(
        have: suite(root, &without_ok, BAD_ROW)?.roster_mismatches(),
        want: vec!["lane.ok.0 (annotate): in roster.tsv, missing from the cells files".to_string()],
    );
    let header = "cell_id\tchart\tvalues\tkube_version\texpect\n";
    let empty = suite(root, header, "")?;
    fs::write(root.join("cells-assert.tsv"), header)?;
    fs::write(root.join("roster.tsv"), "cell_id\tdeclared_types\n")?;
    let empty_roster = read_suite(root)?;
    eyre::ensure!(
        empty
            .roster_mismatches()
            .contains(&"cells.tsv holds no cells".to_string()),
        "an empty table is refused: {:?}",
        empty.roster_mismatches()
    );
    eyre::ensure!(
        empty_roster
            .roster_mismatches()
            .contains(&"the suite holds no cells".to_string()),
        "an empty suite is refused even with an empty roster"
    );
    Ok(())
}

/// A zero-byte file anywhere in the suite is refused.
#[test]
fn the_suite_holds_no_zero_byte_file() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    let root = scratch.path();
    sim_assert_eq!(have: suite(root, CELLS, BAD_ROW)?.empty_files()?, want: Vec::<String>::new());
    fs::create_dir_all(root.join("charts/c/templates"))?;
    fs::write(root.join("charts/c/templates/_helpers.tpl"), "")?;
    sim_assert_eq!(
        have: read_suite(root)?.empty_files()?,
        want: vec!["charts/c/templates/_helpers.tpl: a zero-byte file; delete it".to_string()],
    );
    Ok(())
}

/// The composed document must be the one Helm coalesced from the same inputs: changed values, a
/// tampered snapshot, a diverging port and a missing row all fail.
#[test]
fn composed_documents_are_bound_to_helms_recorded_document() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    let root = scratch.path().join("suite");
    let work = scratch.path().join("work");
    fs::create_dir_all(&work)?;
    let suite = suite(&root, CELLS, BAD_ROW)?;
    let [bad, ..] = suite.cells.as_slice() else {
        eyre::bail!("cells");
    };
    sim_assert_eq!(have: compose(&suite, bad, &work)?, want: json!({ "a": 1 }));
    sim_assert_eq!(have: suite.coalesced_mismatches()?, want: Vec::<String>::new());

    fs::write(root.join("charts/c/values.yaml"), "a: 2\n")?;
    eyre::ensure!(
        compose(&suite, bad, &work).is_err(),
        "changed chart defaults fail"
    );
    fs::write(root.join("charts/c/values.yaml"), "a: 1\n")?;

    fs::create_dir_all(root.join("coalesced"))?;
    fs::write(root.join("coalesced/lane.bad.0.json"), "{\"a\": 3}\n")?;
    eyre::ensure!(
        compose(&suite, bad, &work).is_err(),
        "a tampered snapshot fails"
    );
    fs::write(root.join("coalesced/lane.gone.0.json"), "{}\n")?;
    eyre::ensure!(
        suite.coalesced_mismatches()?.len() == 1,
        "a snapshot naming no cell is a mismatch"
    );
    fs::remove_dir_all(root.join("coalesced"))?;

    let other: Value = json!({ "a": 1, "b": true });
    let table = fs::read_to_string(root.join("coalesced.tsv"))?.replace(
        &document_sha256(&json!({ "a": 1 }))?,
        &document_sha256(&other)?,
    );
    fs::write(root.join("coalesced.tsv"), &table)?;
    let diverging = read_suite(&root)?;
    eyre::ensure!(
        compose(&diverging, bad, &work).is_err(),
        "a diverging port fails"
    );

    let first_row_dropped = without_rows(&table, "lane.bad.0");
    fs::write(root.join("coalesced.tsv"), first_row_dropped)?;
    sim_assert_eq!(
        have: read_suite(&root)?.coalesced_mismatches()?,
        want: vec!["lane.bad.0: no coalesced.tsv row".to_string()],
    );
    Ok(())
}

fn matrix(root: &Path, rows: &[&str]) -> eyre::Result<std::path::PathBuf> {
    let path = root.join("matrix.tsv");
    let mut text = format!("# cell_matrix v1\n{MATRIX_HEADER}\n");
    for row in rows {
        text.push_str(row);
        text.push('\n');
    }
    fs::write(&path, text)?;
    Ok(path)
}

const STRICT_KEPT: &str = "lane.strict.0\tx\thelm\taccept\t0\tpass\t-\tvalid\tyes";

/// A closure re-records the expectation and drops the obligation.
#[test]
fn refresh_records_closures() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    let root = scratch.path().join("suite");
    suite(&root, CELLS, BAD_ROW)?;
    let table = matrix(
        scratch.path(),
        &[
            "lane.bad.0\tx\thelm\treject\t1\treject\t-\t-\tno",
            "lane.ok.0\tx\thelm\taccept\t0\tpass\t-\tvalid\tyes",
            STRICT_KEPT,
        ],
    )?;
    refresh(&root, std::slice::from_ref(&table))?;
    sim_assert_eq!(
        have: fs::read_to_string(root.join("cells.tsv"))?,
        want: CELLS.replace("helm_class=reject main=accept", "helm_class=reject main=reject"),
    );
    sim_assert_eq!(
        have: fs::read_to_string(root.join("obligations.tsv"))?,
        want: OBLIGATIONS_HEADER.to_string(),
    );
    Ok(())
}

/// A NEW failure, a changed Helm class, a table missing a cell and cells beyond the roster are
/// refused, and nothing is written.
#[test]
fn refresh_refuses_to_bless_a_regression() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    let root = scratch.path().join("suite");
    suite(&root, CELLS, BAD_ROW)?;
    let bad_kept = "lane.bad.0\tx\thelm\taccept\t1\treject\t-\t-\tyes";
    for (case, rows) in [
        (
            "NEW",
            vec![
                bad_kept,
                "lane.ok.0\tx\thelm\treject\t0\tpass\t-\tvalid\tno",
                STRICT_KEPT,
            ],
        ),
        (
            "Helm class changed",
            vec![
                bad_kept,
                "lane.ok.0\tx\thelm\taccept\t1\treject\t-\t-\tno",
                STRICT_KEPT,
            ],
        ),
        ("missing cell", vec![bad_kept, STRICT_KEPT]),
    ] {
        let table = matrix(scratch.path(), &rows)?;
        eyre::ensure!(
            refresh(&root, std::slice::from_ref(&table)).is_err(),
            "{case} is refused"
        );
        sim_assert_eq!(have: fs::read_to_string(root.join("cells.tsv"))?, want: CELLS, "{case}");
        sim_assert_eq!(
            have: fs::read_to_string(root.join("obligations.tsv"))?,
            want: format!("{OBLIGATIONS_HEADER}{BAD_ROW}"),
            "{case}"
        );
    }
    let unlisted = format!("{CELLS}lane.new.0\tcharts/c\t{{}}\t1.29.0\t-\n");
    fs::write(root.join("cells.tsv"), &unlisted)?;
    let table = matrix(
        scratch.path(),
        &[
            bad_kept,
            "lane.ok.0\tx\thelm\taccept\t0\tpass\t-\tvalid\tyes",
            "lane.new.0\tx\thelm\treject\t0\tpass\t-\tvalid\tno",
            STRICT_KEPT,
        ],
    )?;
    eyre::ensure!(
        refresh(&root, std::slice::from_ref(&table)).is_err(),
        "a cell missing from the roster is refused"
    );
    fs::write(
        root.join("roster.tsv"),
        format!("{ROSTER}lane.new.0\tannotate\n"),
    )?;
    refresh(&root, std::slice::from_ref(&table))?;
    eyre::ensure!(
        fs::read_to_string(root.join("obligations.tsv"))?
            .contains("lane.new.0\tlane\tannotate\tpass\treject\tfalse-reject\tR1-R3"),
        "a new cell listed in the roster takes what was measured"
    );
    Ok(())
}
