mod common;

use color_eyre::eyre::{OptionExt as _, WrapErr as _};

#[test]
fn signoz_signoz_values_yaml_and_fragments_match() -> color_eyre::eyre::Result<()> {
    let schema = common::generate_chart_schema("signoz-signoz")?;
    let values_json = common::values_yaml_as_json("signoz-signoz")?;
    common::assert_values_json_validates(&values_json, &schema);
    assert_schema_description(
        &schema,
        "/properties/alertmanager/properties/ingress/properties/enabled/description",
        "Enable ingress for Alertmanager",
    );
    assert_schema_description(
        &schema,
        "/properties/alertmanager/properties/ingress/properties/className/description",
        "Ingress Class Name to be used to identify ingress controllers",
    );
    assert_schema_description(
        &schema,
        "/properties/alertmanager/properties/ingress/properties/annotations/description",
        "Annotations to Alertmanager Ingress\nkubernetes.io/ingress.class: nginx\nkubernetes.io/tls-acme: \"true\"\ncert-manager.io/cluster-issuer: letsencrypt-prod",
    );
    assert_schema_description(
        &schema,
        "/properties/alertmanager/properties/ingress/properties/hosts/description",
        "Alertmanager Ingress Host names with their path details",
    );
    common::assert_chart_values_comments_apply_to_existing_schema_paths(
        "signoz-signoz",
        &schema,
        50,
    )?;
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/chart_signoz_signoz.fragments.json"))
            .wrap_err("parse signoz fixture")?;

    let mut actual_keys: Vec<String> = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .ok_or_eyre("schema.properties must be an object")?
        .keys()
        .cloned()
        .collect();
    actual_keys.sort();

    let mut expected_keys: Vec<String> = serde_json::from_value(
        fixture
            .get("top_level_keys")
            .ok_or_eyre("fixture missing top_level_keys")?
            .clone(),
    )
    .wrap_err("parse fixture top_level_keys")?;
    expected_keys.sort();

    similar_asserts::assert_eq!(actual_keys, expected_keys);

    let pointers = fixture
        .get("pointers")
        .and_then(serde_json::Value::as_object)
        .ok_or_eyre("fixture missing pointers object")?;

    for (pointer, expected) in pointers {
        let mut actual = schema
            .pointer(pointer)
            .ok_or_eyre(format!("schema missing pointer {pointer}"))?
            .clone();
        strip_description_annotations(&mut actual);
        similar_asserts::assert_eq!(&actual, expected, "schema mismatch at {pointer}");
    }

    Ok(())
}

fn assert_schema_description(schema: &serde_json::Value, pointer: &str, expected: &str) {
    assert_eq!(
        schema.pointer(pointer).and_then(serde_json::Value::as_str),
        Some(expected),
        "schema description mismatch at {pointer}"
    );
}

fn strip_description_annotations(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            object.remove("description");
            for child in object.values_mut() {
                strip_description_annotations(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strip_description_annotations(item);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}
