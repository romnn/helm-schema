#![recursion_limit = "4096"]

mod common;

#[test]
fn schema_fixtures_match() {
    for case in common::cases::STANDARD_SCHEMA_CASES {
        common::assert_schema_fixture(case);
    }
}

#[test]
fn values_yaml_validates_against_generated_schemas() {
    for case in common::cases::VALUES_VALIDATION_CASES {
        common::assert_values_yaml_validates(case);
    }
}
