use color_eyre::eyre;
use test_util::prelude::sim_assert_eq;

use crate::{Reader, ReaderControl};

fn control_after(input: &str) -> eyre::Result<ReaderControl> {
    let mut reader = Reader::default();
    for character in input.chars() {
        reader.step(character, &mut |_| Ok(()))?;
    }
    Ok(reader.control())
}

#[test]
fn scalar_content_and_prior_offsets_do_not_distinguish_control() -> eyre::Result<()> {
    // Both readers await the same unconsumed suffix after different scalar payloads.
    sim_assert_eq!(
        have: control_after("a: x\nb: left")?,
        want: control_after(&format!("#{}\na: y\nb: raft", " ".repeat(510)))?,
    );
    Ok(())
}

#[test]
fn held_flow_scalar_payload_is_not_part_of_control() -> eyre::Result<()> {
    sim_assert_eq!(
        have: control_after("[left")?,
        want: control_after("[raft")?,
    );
    Ok(())
}
