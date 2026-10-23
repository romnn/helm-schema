//! The admission suite's reading, obligation consistency, classification and re-recording, over
//! a two-cell synthetic suite.

use std::fs;
use std::path::Path;

use color_eyre::eyre;
use helm_schema_test_support::admission::{Admission, Verdict, classify, read_suite, refresh};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

const OBLIGATIONS_HEADER: &str = "cell_id\tlane\tdeclared_types\thelm_class\tmain\terror\towner\n";

/// A suite whose annotate file holds `lane.ok.0` (main right) and `lane.bad.0` (main wrong), an
/// empty assert file, and `obligations`.
fn suite(root: &Path, obligations: &str) -> eyre::Result<()> {
    fs::create_dir_all(root.join("charts/c"))?;
    fs::write(
        root.join("charts/c/Chart.yaml"),
        "apiVersion: v2\nname: c\nversion: 0.1.0\n",
    )?;
    fs::write(
        root.join("cells.tsv"),
        indoc! {"
            cell_id\tchart\tvalues\tkube_version\texpect
            lane.bad.0\tcharts/c\t{}\t1.29.0\thelm_class=reject main=accept
            lane.ok.0\tcharts/c\t{}\t1.29.0\thelm_class=pass main=accept
        "},
    )?;
    fs::write(
        root.join("cells-assert.tsv"),
        "cell_id\tchart\tvalues\tkube_version\texpect\n",
    )?;
    fs::write(
        root.join("obligations.tsv"),
        format!("{OBLIGATIONS_HEADER}{obligations}"),
    )?;
    Ok(())
}

const BAD_ROW: &str = "lane.bad.0\tlane\tannotate\treject\taccept\tfalse-accept\tADR-0042\n";

#[test]
fn obligations_must_be_exactly_the_cells_main_gets_wrong() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    let root = scratch.path();
    suite(root, BAD_ROW)?;
    sim_assert_eq!(have: read_suite(root)?.obligation_mismatches(), want: Vec::<String>::new());
    suite(root, "")?;
    sim_assert_eq!(
        have: read_suite(root)?.obligation_mismatches(),
        want: vec!["lane.bad.0: main disagrees with Helm, but obligations.tsv omits it".to_string()],
    );
    suite(
        root,
        &format!("{BAD_ROW}lane.ok.0\tlane\tannotate\tpass\taccept\tfalse-accept\tR1-R3\n"),
    )?;
    sim_assert_eq!(
        have: read_suite(root)?.obligation_mismatches(),
        want: vec!["lane.ok.0: listed in obligations.tsv, but main agrees with Helm".to_string()],
    );
    suite(
        root,
        "lane.bad.0\tlane\tannotate\treject\treject\tfalse-reject\tR1-R3\n",
    )?;
    eyre::ensure!(
        read_suite(root)?.obligation_mismatches().len() == 1,
        "a stale row restating another verdict is a mismatch"
    );
    Ok(())
}

#[test]
fn a_verdict_away_from_helm_is_new_and_one_toward_helm_is_closed() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    suite(scratch.path(), BAD_ROW)?;
    let suite = read_suite(scratch.path())?;
    let [bad, ok] = suite.cells.as_slice() else {
        eyre::bail!("two cells");
    };
    sim_assert_eq!(have: classify(ok, Verdict::Accept), want: Admission::Kept);
    sim_assert_eq!(have: classify(ok, Verdict::Reject), want: Admission::New);
    sim_assert_eq!(have: classify(bad, Verdict::Accept), want: Admission::Kept);
    sim_assert_eq!(have: classify(bad, Verdict::Reject), want: Admission::Closed);
    Ok(())
}

/// Re-recording a closure rewrites the expectation and drops the obligation; a re-recorded
/// obligation keeps its owner.
#[test]
fn refresh_rewrites_expectations_and_obligations() -> eyre::Result<()> {
    let scratch = ScratchDir::new("admission")?;
    let root = scratch.path().join("suite");
    suite(&root, BAD_ROW)?;
    let matrix = scratch.path().join("matrix.tsv");
    let header =
        "cell_id\tprep_sha256\tvalues_src\tmain\thelm_rc\thelm_class\thelm_abort\tk8s\texpect_ok";
    fs::write(
        &matrix,
        format!(
            "# cell_matrix v1\n{header}\nlane.bad.0\tx\thelm\treject\t1\treject\t-\t-\tno\n\
             lane.ok.0\tx\thelm\treject\t0\tpass\t-\tvalid\tno\n"
        ),
    )?;
    refresh(&root, std::slice::from_ref(&matrix))?;
    sim_assert_eq!(
        have: fs::read_to_string(root.join("cells.tsv"))?,
        want: indoc! {"
            cell_id\tchart\tvalues\tkube_version\texpect
            lane.bad.0\tcharts/c\t{}\t1.29.0\thelm_class=reject main=reject
            lane.ok.0\tcharts/c\t{}\t1.29.0\thelm_class=pass main=reject
        "},
    );
    sim_assert_eq!(
        have: fs::read_to_string(root.join("obligations.tsv"))?,
        want: format!(
            "{OBLIGATIONS_HEADER}lane.ok.0\tlane\tannotate\tpass\treject\tfalse-reject\tR1-R3\n"
        ),
    );
    sim_assert_eq!(have: read_suite(&root)?.obligation_mismatches(), want: Vec::<String>::new());
    fs::write(
        &matrix,
        format!("{header}\nlane.ok.0\tx\thelm\taccept\t0\tpass\t-\tvalid\tyes\n"),
    )?;
    eyre::ensure!(
        refresh(&root, std::slice::from_ref(&matrix)).is_err(),
        "a table that misses a cell is refused"
    );
    Ok(())
}
