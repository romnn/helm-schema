use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{
    ConditionalGuard, GuardConjunction, Predicate, RequirementAtom, RequirementBdd,
    RequirementConditionBuilder, ValuesPath,
};
use serde_json::{Value, json};
use serde_yaml::Value as YamlValue;
use test_util::prelude::sim_assert_eq;

use super::append_terminal_clause;
use crate::RootPolicy;
use crate::condition_encoding::{AbsenceDefaults, helm_truthy_definition_schema};
use crate::schema_tree::SchemaDocument;
use crate::terminal_conditions::TerminalEmissions;
use crate::uncoalesced_root::UncoalescedRootGate;

#[test]
fn unrepresentable_decisions_never_turn_terminals_into_unconditional_rejections() -> eyre::Result<()>
{
    let predicate = Predicate::truthy_path("flag");
    let atom = RequirementAtom::Value(helm_schema_core::Guard::Truthy {
        path: ValuesPath::parse("flag"),
    });
    let atoms = [atom];
    let mut remaining = RequirementBdd::WORK_BUDGET;
    let mut bdd =
        RequirementBdd::new(&atoms, &mut remaining).ok_or_eyre("decision graph inventory")?;
    let root = bdd
        .predicate(&predicate)
        .ok_or_eyre("decision graph predicate")?;
    let guard = bdd.finish(root).ok_or_eyre("decision graph output")?;
    let row = GuardConjunction::from(vec![guard]).canonicalized();
    let coalesced = json!({});
    let values = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &values,
        dependency_refill: &values,
        dependency_roots: &roots,
    };
    for unsupported in [
        None,
        Some(json!({"$id":"urn:terminal-test"})),
        Some(json!({"$ref":"file:///values.schema.json#/allOf/1/if"})),
    ] {
        let definitions = BTreeMap::new();
        let mut gate = UncoalescedRootGate::new(None, &coalesced, &definitions);
        if let Some(schema) = &unsupported {
            gate.reserve_condition_names(schema);
        }
        let mut document = SchemaDocument::new_root_object(RootPolicy::Open);
        let mut emissions = TerminalEmissions::default();
        append_terminal_clause(&mut emissions, &mut gate, &row, &[], &values, absence);
        emissions.append(&mut document, &gate);
        let schema = document.into_value();
        let generated = gate.take_condition_definitions();
        if unsupported.is_some() {
            sim_assert_eq!(have:&schema,want:&json!({"type":"object","properties":{}}));
            sim_assert_eq!(have:&generated,want:&BTreeMap::<String,Value>::new());
        } else {
            sim_assert_eq!(have:&schema,want:&json!({"type":"object","properties":{},"allOf":[{
                "if":{"$ref":"#/definitions/helm-condition-0"},"then":false,
            }]}));
            sim_assert_eq!(have:&generated,want:&std::collections::BTreeMap::from([(
                "helm-condition-0".to_owned(),json!({"if":{
                    "type":"object","properties":{"flag":{"$ref":"#/definitions/t"}},"required":["flag"],
                },"then":true,"else":false}),
            )]));
        }
        let mut complete = schema;
        complete["definitions"] = serde_json::to_value(generated)?;
        complete["definitions"]["t"] = helm_truthy_definition_schema();
        let validator = jsonschema::validator_for(&complete)?;
        sim_assert_eq!(have:validator.is_valid(&json!({"flag":false})),want:true);
        sim_assert_eq!(have:validator.is_valid(&json!({"flag":true})),want:unsupported.is_some());
    }
    Ok(())
}

#[test]
fn a_requirement_dag_shares_children_and_has_a_whole_fact_scope_barrier() -> eyre::Result<()> {
    let mut graph = RequirementConditionBuilder::default();
    let suffix = graph.guard(ConditionalGuard::Truthy {
        path: ValuesPath::parse("blocked"),
    });
    let mut root = suffix;
    let depth = 10;
    for index in 0..depth {
        root = graph.split(
            ConditionalGuard::Truthy {
                path: ValuesPath::parse(&format!("branch{index}")),
            },
            root,
            root,
        );
    }
    let row = GuardConjunction::from(vec![ConditionalGuard::Requirement(
        graph.freeze().root(root, true),
    )]);
    let values = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &values,
        dependency_refill: &values,
        dependency_roots: &roots,
    };
    let coalesced = json!({});
    let definitions = BTreeMap::new();
    for shared in [true, false] {
        let mut gate = UncoalescedRootGate::new(None, &coalesced, &definitions);
        if !shared {
            gate.reserve_condition_names(&json!({"$ref":"file:///another-values.json#/type"}));
        }
        let mut document = SchemaDocument::new_root_object(RootPolicy::Open);
        let mut emissions = TerminalEmissions::default();
        append_terminal_clause(&mut emissions, &mut gate, &row, &[], &values, absence);
        emissions.append(&mut document, &gate);
        let mut schema = document.into_value();
        let private = gate.take_condition_definitions();
        schema["definitions"] = serde_json::to_value(private)?;
        schema["definitions"]["t"] = helm_truthy_definition_schema();
        let truthy = |path: &str| {
            json!({
                "type":"object","properties":{path:{"$ref":"#/definitions/t"}},"required":[path],
            })
        };
        let mut expected = json!({
            "type":"object","properties":{},"definitions":{"t":helm_truthy_definition_schema()},
        });
        if shared {
            let mut successor = truthy("blocked");
            for index in 0..depth {
                let name = format!("helm-condition-{index}");
                expected["definitions"][&name] = json!({
                    "if":truthy(&format!("branch{index}")),"then":successor,"else":successor,
                });
                successor = json!({"$ref":format!("#/definitions/{name}")});
            }
            expected["allOf"] = json!([{"if":successor,"then":false}]);
        }
        sim_assert_eq!(have: &schema, want: &expected);
        let validator = jsonschema::validator_for(&schema)?;
        sim_assert_eq!(have: validator.is_valid(&json!({"blocked":true})), want: !shared);
        sim_assert_eq!(have: validator.is_valid(&json!({"blocked":false})), want: true);
    }
    Ok(())
}
