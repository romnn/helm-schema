use color_eyre::eyre::{self, OptionExt as _};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use helm_schema_core::re2_to_ecma_pattern;

#[test]
fn leading_multiline_start_anchor_has_an_exact_ecma_form() -> eyre::Result<()> {
    let go_pattern = r"(?m)^name\s*=\s*value";
    let ecma_pattern = re2_to_ecma_pattern(go_pattern).ok_or_eyre("pattern must lower")?;

    sim_assert_eq!(
        have: &ecma_pattern,
        want: r"(?:^|\n)name[\t\n\f\r ]*=[\t\n\f\r ]*value"
    );

    let go = regex::Regex::new(go_pattern)?;
    let ecma = regex::Regex::new(&ecma_pattern)?;
    for sample in [
        "",
        "name=value",
        indoc! {"
            # comment
            name = value
        "},
        "xname=value",
        indoc! {"

             xname=value"
        },
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
        have: re2_to_ecma_pattern("(?m)literal"),
        want: Some("literal".to_string())
    );
}

#[test]
fn later_multiline_anchors_abstain() {
    sim_assert_eq!(
        have: re2_to_ecma_pattern("(?m)prefix$"),
        want: None
    );
    sim_assert_eq!(
        have: re2_to_ecma_pattern("(?m)^first$"),
        want: None
    );
}

/// A chart's `regexMatch` pattern gets the same `u`-flag escape rewrite as a
/// provider pattern.
#[test]
fn chart_patterns_drop_unicode_mode_rejected_escapes() {
    sim_assert_eq!(
        have: re2_to_ecma_pattern(r"^\ \{x\}\-\d+$"),
        want: Some(r"^ \{x\}-[0-9]+$".to_string()),
    );
}
