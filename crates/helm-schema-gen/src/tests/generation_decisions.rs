use serde_json::Value;
use test_util::prelude::sim_assert_eq;

use super::{
    ContractValuePathFacts, SchemaNode, ValuePathSchemaFacts, ValuePathSchemaInputs,
    ValuesYamlPathFacts,
};
use crate::generation_decisions::{
    ChannelDisposition, IndependentChannels, IndependentQualification, MergeBase, PolicyEvaluation,
};
use crate::path_resolver::independent_base_contract;

fn provider_only_inputs(provider_schema: Value) -> ValuePathSchemaInputs {
    ValuePathSchemaInputs::Complete {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts::default(),
            ValuesYamlPathFacts::default(),
        ),
        provider_schema: SchemaNode::from_value(provider_schema),
        values_yaml_schema: SchemaNode::empty(),
        guard_predicate_schema: SchemaNode::empty(),
        type_hint_schema: SchemaNode::empty(),
        guarded_type_hint_schema: SchemaNode::empty(),
        fallback_type_hint_schema: SchemaNode::empty(),
    }
}

fn provider_only_evaluation() -> PolicyEvaluation {
    PolicyEvaluation {
        provider: ChannelDisposition::Applied {
            adjustments: Vec::new(),
        },
        declared_default: ChannelDisposition::Absent,
        guard_domain: ChannelDisposition::Absent,
        type_hints: ChannelDisposition::Absent,
        guarded_type_hints: ChannelDisposition::Absent,
        fallback_type_hints: ChannelDisposition::Absent,
        merge_base: MergeBase::Provider,
        rules: Vec::new(),
    }
}

#[test]
fn independent_contract_qualifies_a_constraining_provider_slot() {
    let (contract, qualification) = independent_base_contract(
        &helm_schema_core::ValuesPath::parse("service.port"),
        &provider_only_inputs(serde_json::json!({ "type": "integer" })),
    );

    sim_assert_eq!(
        have: contract.map(|contract| contract.schema().into_value()),
        want: Some(serde_json::json!({ "type": "integer" }))
    );
    sim_assert_eq!(
        have: qualification,
        want: IndependentQualification::Qualified {
            channels: IndependentChannels {
                strict_string: false,
                provider: true,
            },
            evaluation: provider_only_evaluation(),
        }
    );
}

#[test]
fn independent_contract_rejects_an_unconstraining_provider_slot() {
    let (contract, qualification) = independent_base_contract(
        &helm_schema_core::ValuesPath::parse("service.port"),
        &provider_only_inputs(Value::Bool(true)),
    );

    sim_assert_eq!(have: contract, want: None);
    sim_assert_eq!(
        have: qualification,
        want: IndependentQualification::RejectedEmpty {
            channels: IndependentChannels {
                strict_string: false,
                provider: true,
            },
            evaluation: provider_only_evaluation(),
        }
    );
}

#[test]
fn independent_contract_requires_a_literal_path_and_a_consumer() {
    let (_, wildcard) = independent_base_contract(
        &helm_schema_core::ValuesPath::parse("ports.*.port"),
        &provider_only_inputs(serde_json::json!({ "type": "integer" })),
    );
    let (_, no_consumer) = independent_base_contract(
        &helm_schema_core::ValuesPath::parse("service.port"),
        &provider_only_inputs(serde_json::json!({})),
    );

    sim_assert_eq!(have: wildcard, want: IndependentQualification::WildcardPath);
    sim_assert_eq!(have: no_consumer, want: IndependentQualification::NoIndependentConsumer);
}
