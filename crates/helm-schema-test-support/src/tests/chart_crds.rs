//! Native checker selection: only an explicit binary opts into live checking.

use std::fs;

use color_eyre::eyre;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

use crate::helm::chart_crds::Checker;
use crate::source_digest::sha256_hex;

#[test]
fn provisioned_binary_never_replaces_committed_replay_identity() -> eyre::Result<()> {
    let root = ScratchDir::new("native-selection")?;
    let pin = "6da0536ca4b82c7781cfabdb2b23a90286b926a6569f1276d2612538ebe82ffd";
    fs::create_dir_all(root.path().join("testdata/crdcheck"))?;
    fs::write(
        root.path().join("testdata/crdcheck/binary.sha256"),
        format!("{pin}\n"),
    )?;
    fs::create_dir_all(root.path().join("target"))?;
    let binary = root.path().join("target/crdcheck");
    fs::write(&binary, b"provisioned maintainer binary")?;
    let replay = Checker::select(root.path(), None, None);
    sim_assert_eq!(have: replay.identity().lines().last(), want: Some(pin));
    let live = Checker::select(root.path(), Some(binary), None);
    let binary_sha256 = sha256_hex(b"provisioned maintainer binary");
    sim_assert_eq!(have: live.identity().lines().last(), want: Some(binary_sha256.as_str()));
    Ok(())
}
