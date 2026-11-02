use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

use super::TextPreimage;

#[test]
fn literal_boundaries_and_expanding_characters_compose() -> eyre::Result<()> {
    let relation = TextPreimage {
        before: "pre".to_owned(),
        after: "post".to_owned(),
        characters: [(b'x', "yz".to_owned())].into_iter().collect(),
    };
    let pattern = relation
        .pattern("^pre(yz){2}post$")
        .ok_or_eyre("missing bounded language")?;
    let pattern = regress::Regex::new(&pattern)?;
    for (text, want) in [
        ("xx", true),
        ("x", false),
        ("xxx", false),
        ("yz", false),
        ("", false),
    ] {
        sim_assert_eq!(have: pattern.find(text).is_some(), want: want, "{text:?}");
    }
    sim_assert_eq!(have: relation.apply("xx", 13), want: Some("preyzyzpost".to_owned()));
    sim_assert_eq!(have: relation.apply("xx", 9), want: None);
    Ok(())
}

#[test]
fn composition_maps_literal_affixes_once() -> eyre::Result<()> {
    let inner = TextPreimage {
        before: "a".to_owned(),
        after: "b".to_owned(),
        ..TextPreimage::default()
    };
    let outer = TextPreimage {
        before: "[".to_owned(),
        after: "]".to_owned(),
        characters: [(b'a', "A".to_owned()), (b'b', "B".to_owned())]
            .into_iter()
            .collect(),
    };
    let relation = inner.then(&outer).ok_or_eyre("missing composed relation")?;
    sim_assert_eq!(have: relation.apply("x", 32), want: Some("[AxB]".to_owned()));
    let pattern = relation
        .pattern(r"^\[AxB\]$")
        .ok_or_eyre("missing composed language")?;
    let pattern = regress::Regex::new(&pattern)?;
    sim_assert_eq!(have: pattern.find("x").is_some(), want: true);
    sim_assert_eq!(have: pattern.find("xx").is_some(), want: false);
    Ok(())
}

#[test]
fn suffixes_after_a_comment_keep_the_prefix_language() -> eyre::Result<()> {
    let relation = TextPreimage {
        after: ":tag".to_owned(),
        ..TextPreimage::default()
    };
    let pattern = relation
        .pattern(r"^true[ \t]+#[\s\S]*$")
        .ok_or_eyre("missing comment language")?;
    let pattern = regress::Regex::new(&pattern)?;
    for (text, want) in [
        ("true #b", true),
        ("true", false),
        ("true#b", false),
        ("a #b", false),
    ] {
        sim_assert_eq!(have: pattern.find(text).is_some(), want: want, "{text:?}");
    }
    Ok(())
}

#[test]
fn unsupported_regexes_and_unicode_sources_remain_unknown() {
    let relation = TextPreimage::default();
    sim_assert_eq!(have: relation.schema_preimage(&serde_json::json!({"pattern": "(?=x)"})), want: serde_json::json!({}));
    sim_assert_eq!(have: relation.apply("ß", 32), want: None);
    for pattern in [
        r"\Avalue\z",
        r"\a",
        "[a&&b]",
        "[[:alpha:]]",
        r"\x{41}",
        "(?s:.)",
    ] {
        sim_assert_eq!(have: relation.schema_preimage(&serde_json::json!({"pattern": pattern})), want: serde_json::json!({}), "{pattern}");
    }
}

#[test]
fn pattern_preimages_preserve_dot_line_break_semantics() -> eyre::Result<()> {
    let relation = TextPreimage::default();
    let pattern = relation
        .pattern("^a.b$")
        .ok_or_eyre("missing dot language")?;
    let pattern = regress::Regex::new(&pattern)?;
    for (text, want) in [
        ("axb", true),
        ("a b", true),
        ("a\nb", false),
        ("a\rb", false),
    ] {
        sim_assert_eq!(have: pattern.find(text).is_some(), want: want, "{text:?}");
    }
    sim_assert_eq!(have: relation.pattern("(?m)^x$"), want: None);
    Ok(())
}

#[test]
fn finite_preimages_and_reference_siblings_preserve_schema_algebra() {
    let relation = TextPreimage::default();
    let empty = serde_json::json!({"anyOf": [false,
        {"type": "string", "pattern": r"[^\x00-\x7f]"},
        {"not": {"type": "string"}}]});
    for schema in [
        serde_json::json!({"enum": [7, null, false]}),
        serde_json::json!({"const": 7}),
    ] {
        sim_assert_eq!(have: relation.schema_preimage(&schema), want: empty);
    }
    sim_assert_eq!(have: relation.schema_preimage(&serde_json::json!({
        "$ref": "#/definitions/anything", "type": "integer", "pattern": "^never$",
        "definitions": {"anything": {}}
    })), want: serde_json::json!({}));
}
