use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

use crate::path_resolver::ecma_compatible_pattern;

#[test]
fn leading_multiline_start_anchor_has_an_exact_ecma_form() -> eyre::Result<()> {
    let go_pattern = r"(?m)^name\s*=\s*value";
    let ecma_pattern = ecma_compatible_pattern(go_pattern).ok_or_eyre("pattern must lower")?;

    sim_assert_eq!(
        have: &ecma_pattern,
        want: r"(?:^|\n)name\s*=\s*value"
    );

    let go = regex::Regex::new(go_pattern)?;
    let ecma = regex::Regex::new(&ecma_pattern)?;
    for sample in [
        "",
        "name=value",
        "# comment\nname = value\n",
        "xname=value",
        "\n xname=value",
    ] {
        sim_assert_eq!(
            have: ecma.is_match(sample),
            want: go.is_match(sample),
            "sample={sample:?}"
        );
    }

    Ok(())
}

#[test]
fn multiline_without_anchors_drops_the_semantically_idle_flag() {
    sim_assert_eq!(
        have: ecma_compatible_pattern("(?m)literal"),
        want: Some("literal".to_string())
    );
}

#[test]
fn later_multiline_anchors_abstain() {
    sim_assert_eq!(
        have: ecma_compatible_pattern("(?m)prefix$"),
        want: None
    );
    sim_assert_eq!(
        have: ecma_compatible_pattern("(?m)^first$"),
        want: None
    );
}
