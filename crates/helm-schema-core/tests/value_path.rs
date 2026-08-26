//! Structural and wire compatibility tests for the values-path carrier.

use std::collections::BTreeSet;

use helm_schema_core::ValuesPath;
use test_util::prelude::sim_assert_eq;

#[test]
fn path_construction_and_navigation_are_structural() {
    let parsed = ValuesPath::parse(r"service.annotations.prometheus\.io\\path.*");
    sim_assert_eq!(
        have: parsed.segments().collect::<Vec<_>>(),
        want: vec!["service", "annotations", "prometheus.io\\path", "*"]
    );
    sim_assert_eq!(
        have: parsed.encode(),
        want: r"service.annotations.prometheus\.io\\path.*"
    );

    let item_parent = parsed.item_parent().map(|path| path.encode());
    sim_assert_eq!(
        have: item_parent,
        want: Some(r"service.annotations.prometheus\.io\\path".to_string())
    );
    let item_parent = ValuesPath::parse(r"service.annotations.prometheus\.io\\path");
    assert!(parsed.is_descendant_of(&item_parent));
    assert!(!item_parent.is_descendant_of(&item_parent));

    let mut rebuilt = ValuesPath::from_segments(["service", "annotations"]);
    rebuilt.push("prometheus.io\\path");
    rebuilt.push("");
    rebuilt.push("*");
    sim_assert_eq!(have: rebuilt, want: parsed);
}

#[test]
fn parse_and_encode_preserve_legacy_canonical_spelling() {
    for (encoded, segments, canonical) in [
        ("", vec![], ""),
        (".a..b.", vec!["a", "b"], "a.b"),
        (r"a\.b.c", vec!["a.b", "c"], r"a\.b.c"),
        (r"a\\b.c", vec!["a\\b", "c"], r"a\\b.c"),
        (r"a\qb", vec![r"a\qb"], r"a\\qb"),
    ] {
        let path = ValuesPath::parse(encoded);
        sim_assert_eq!(have: path.segments().collect::<Vec<_>>(), want: segments);
        sim_assert_eq!(have: path.encode(), want: canonical);
    }
}

#[test]
fn serde_preserves_the_encoded_string_wire() -> serde_json::Result<()> {
    let path = ValuesPath::from_segments(["service", "prometheus.io/path", r"a\b"]);
    let encoded = serde_json::to_value(&path)?;
    sim_assert_eq!(
        have: encoded,
        want: serde_json::json!(r"service.prometheus\.io/path.a\\b")
    );
    let decoded: ValuesPath = serde_json::from_value(encoded)?;
    sim_assert_eq!(have: decoded, want: path);
    Ok(())
}

#[test]
fn ordering_matches_legacy_encoded_strings() {
    let paths = [
        ValuesPath::from_segments(["a", "z"]),
        ValuesPath::from_segments(["a.b"]),
        ValuesPath::from_segments([r"a\b"]),
        ValuesPath::from_segments(["a", "*"]),
        ValuesPath::from_segments(["a"]),
        ValuesPath::from_segments(["z"]),
    ];
    let have = paths.iter().cloned().collect::<BTreeSet<_>>();
    let have = have.iter().map(ValuesPath::encode).collect::<Vec<_>>();
    let want = paths
        .iter()
        .map(ValuesPath::encode)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    sim_assert_eq!(have: have, want: want);
}
