//! Semantic assertions for bitnami-redis: values-comment descriptions must
//! land on the right schema nodes. Values validation and the full-schema pin
//! live in `chart_corpus.rs`.

use test_util::prelude::sim_assert_eq;
#[path = "common/descriptions.rs"]
mod descriptions;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn bitnami_redis_values_descriptions_apply() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("bitnami-redis")?;
    assert_schema_description(
        &schema,
        "/properties/auth/properties/enabled/description",
        "Enable password authentication",
    );
    // `image` flows wholesale through the vendored `common` library's
    // tplvalues rendering, so its members stay an open map and carry no
    // per-member description node; `architecture` pins a plain scalar
    // comment instead.
    assert_schema_description(
        &schema,
        "/properties/architecture/description",
        "Redis(R) architecture. Allowed values: `standalone` or `replication`",
    );
    assert_schema_description(
        &schema,
        "/properties/global/properties/imageRegistry/description",
        "Global Docker image registry",
    );
    descriptions::assert_chart_values_comments_apply_to_existing_schema_paths(
        "bitnami-redis",
        &schema,
        50,
    )?;

    Ok(())
}

/// Resolve a JSON pointer while following local `$defs` refs: output
/// interning may move any subtree into a root-level definition.
fn pointer_through_refs<'schema>(
    root: &'schema serde_json::Value,
    pointer: &str,
) -> Option<&'schema serde_json::Value> {
    let mut node = root;
    for segment in pointer.split('/').filter(|segment| !segment.is_empty()) {
        while let Some(name) = node
            .get("$ref")
            .and_then(serde_json::Value::as_str)
            .and_then(|reference| reference.strip_prefix("#/$defs/"))
        {
            node = root.get("$defs")?.get(name)?;
        }
        node = node.get(segment)?;
    }
    Some(node)
}

fn assert_schema_description(schema: &serde_json::Value, pointer: &str, expected: &str) {
    sim_assert_eq!(
        have: pointer_through_refs(schema, pointer).and_then(serde_json::Value::as_str),
        want: Some(expected),
        "schema description mismatch at {pointer}"
    );
}
