//! Re-records the W1 admission suite; see [`helm_schema_test_support::admission`].
//!
//! ```text
//! admission_refresh <matrix.tsv>...
//! admission_refresh --coalesced <helm>
//! ```
//!
//! The first form re-records every cell's `helm_class=` and `main=` and `obligations.tsv` from
//! `cell_matrix` tables, each from a run over one of the suite's cells files with the binary to
//! record labelled `main` (the suite's README.md names the commands); together they must measure
//! every cell. The second records pinned Helm's coalesced document for every cell the Rust values
//! port does not model.

use std::path::PathBuf;

use color_eyre::eyre;
use helm_schema_test_support::admission::{SUITE_DIR, record_helm_coalesced, refresh};
use test_util::scratch::ScratchDir;

fn main() -> eyre::Result<()> {
    let root = test_util::workspace_testdata().join(SUITE_DIR);
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, helm] if flag == "--coalesced" => {
            let scratch = ScratchDir::new("admission-refresh")?;
            for id in record_helm_coalesced(&root, &PathBuf::from(helm), scratch.path())? {
                println!("recorded {id}");
            }
            Ok(())
        }
        [] => eyre::bail!("usage: admission_refresh <matrix.tsv>... | --coalesced <helm>"),
        matrices => refresh(
            &root,
            &matrices.iter().map(PathBuf::from).collect::<Vec<_>>(),
        ),
    }
}
