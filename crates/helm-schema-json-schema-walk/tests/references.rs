//! Reference parsing integration tests.

use helm_schema_json_schema_walk::local_reference_fragment;
use test_util::prelude::sim_assert_eq;

/// The fragment of a same-document reference as the URI parser decodes it.
fn parsed_fragment(reference: &str) -> Option<String> {
    let parsed = fluent_uri::UriRef::parse(reference).ok()?;
    if parsed.has_scheme()
        || parsed.has_authority()
        || !parsed.path().is_empty()
        || parsed.has_query()
    {
        return None;
    }
    Some(parsed.fragment()?.decode().to_string().ok()?.into_owned())
}

#[test]
fn every_fragment_of_one_or_two_ascii_characters_decodes_as_the_uri_parser_does() {
    let characters = (0u8..=127).map(char::from).collect::<Vec<_>>();
    let mut references = vec!["#".to_string()];
    for first in &characters {
        references.push(format!("#{first}"));
        references.push(format!("{first}#"));
        for second in &characters {
            references.push(format!("#{first}{second}"));
        }
    }
    for reference in references {
        sim_assert_eq!(
            have: local_reference_fragment(&reference),
            want: parsed_fragment(&reference)
        );
    }
}

#[test]
fn definition_references_decode_as_the_uri_parser_does() {
    for reference in [
        "#/$defs/helm-condition-12",
        "#/$defs/when~1a.b=1/then/a?",
        "#/$defs/a%20b",
        "#/$defs/caf%C3%A9",
        "#/$defs/café",
        "#/$defs/a b",
        "#/$defs/a#b",
        "#/$defs/%",
        "#/$defs/%zz",
        "other.json#/$defs/a",
        "#/$defs/values.items@items/properties/x",
    ] {
        sim_assert_eq!(
            have: local_reference_fragment(reference),
            want: parsed_fragment(reference)
        );
    }
}
