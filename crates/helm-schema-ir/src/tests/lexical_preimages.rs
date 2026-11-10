//! Laws: every lexical preimage is a sound subset, and an exact one equals the true preimage.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::ValuePattern;
use test_util::prelude::sim_assert_eq;

use super::{Lexical, text_preimage};
use crate::value::relationship::TextTransform;

const SEMVER: &str = r"^v?(0*[0-9]{1,20})(\.0*[0-9]{1,20})?(\.0*[0-9]{1,20})?(-(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(\.(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?(\+([0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*))?$";

const SAMPLES: &[&str] = &[
    "",
    "v",
    "1.2.3",
    "v1.2.3",
    "vv1.2.3",
    "1.2.3-jmx",
    "1.2.3-jmx-jmx",
    "junk-jmx",
    "v-jmx-7.80.1",
    "1.2.3@sha256:abc",
    "v1.2.3@sha256:abc",
    "@1.2.3",
    // Go's `@.*$` erases from the first `@` after the last newline: this keeps "1.2.3@x\n1.2.3".
    "1.2.3@x\n1.2.3@y",
    "1.2.3@",
    "1.2.3@a@b",
    "1.2.3@a\nb",
    "1.2\n@x",
    // A token before the newline and another after it: Go keeps "a@b\nx-jmx" and "a@b\nc.d".
    "a@b\nx-jmx@d",
    "a@b\nc.d@e",
    "garbage",
    "latest-1.2.3",
    "master",
    "1.2.3 ",
];

fn output_patterns() -> Vec<ValuePattern> {
    let semver = ValuePattern::String {
        pattern: SEMVER.to_owned(),
    };
    vec![
        semver.clone(),
        ValuePattern::Not(Box::new(semver.clone())),
        ValuePattern::AllOf(vec![
            semver,
            ValuePattern::String {
                pattern: r"^v?0*1\.".to_owned(),
            },
        ]),
        ValuePattern::String {
            pattern: "jmx$".to_owned(),
        },
        ValuePattern::String {
            pattern: r"\.".to_owned(),
        },
        ValuePattern::Not(Box::new(ValuePattern::String {
            pattern: "jmx$".to_owned(),
        })),
        ValuePattern::Not(Box::new(ValuePattern::String {
            pattern: r"\.".to_owned(),
        })),
    ]
}

fn transforms() -> Vec<TextTransform> {
    vec![
        TextTransform::TrimPrefix("v".to_owned()),
        TextTransform::TrimSuffix("-jmx".to_owned()),
        TextTransform::Replace {
            from: "latest-".to_owned(),
            to: String::new(),
        },
        TextTransform::Cut("@".to_owned()),
        TextTransform::RegexReplace {
            pattern: "@.*$".to_owned(),
            replacement: String::new(),
        },
    ]
}

/// The reference semantics of each transform, as Go executes it.
fn executed(transform: &TextTransform, text: &str) -> eyre::Result<String> {
    match transform {
        TextTransform::RegexReplace {
            pattern,
            replacement,
        } => Ok(regex::Regex::new(pattern)?
            .replace_all(text, replacement.as_str())
            .into_owned()),
        transform => transform.apply(text, 4096).ok_or_eyre("transform declined"),
    }
}

fn matches(pattern: &ValuePattern, text: &str) -> eyre::Result<bool> {
    pattern
        .matches(&serde_json::Value::String(text.to_owned()))
        .ok_or_eyre("pattern did not compile")
}

#[test]
fn preimages_are_sound_and_exact_ones_are_complete() -> eyre::Result<()> {
    let mut exact = 0_usize;
    for transform in transforms() {
        for output in output_patterns() {
            let Some(Lexical {
                pattern,
                exact: complete,
            }) = text_preimage(&transform, &output)
            else {
                continue;
            };
            exact += usize::from(complete);
            for sample in SAMPLES {
                let image = matches(&output, &executed(&transform, sample)?)?;
                let preimage = matches(&pattern, sample)?;
                assert!(
                    !preimage || image,
                    "unsound preimage: {transform:?} {output:?} at {sample:?} via {pattern:?}"
                );
                if complete {
                    sim_assert_eq!(have: preimage, want: image);
                }
            }
        }
    }
    // The line-tail regex erasure is a subset: multi-line sources with an erased tail stay undecided.
    sim_assert_eq!(have: exact, want: 21);
    Ok(())
}

#[test]
fn replacement_preimage_stays_a_subset() -> eyre::Result<()> {
    let output = ValuePattern::String {
        pattern: SEMVER.to_owned(),
    };
    let preimage = text_preimage(
        &TextTransform::Replace {
            from: "master".to_owned(),
            to: "1.20.0".to_owned(),
        },
        &output,
    )
    .ok_or_eyre("replace keeps its identity region")?;
    sim_assert_eq!(have: preimage.exact, want: false);
    sim_assert_eq!(have: matches(&preimage.pattern, "master")?, want: false);
    sim_assert_eq!(have: matches(&preimage.pattern, "1.2.3")?, want: true);
    Ok(())
}
