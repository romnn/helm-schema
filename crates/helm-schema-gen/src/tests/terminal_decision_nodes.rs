use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{
    ConditionalGuard, Guard, Predicate, RequirementAtom, RequirementBdd, ValuesPath,
};
use serde_json::{Value, json};
use serde_yaml::Value as YamlValue;
use test_util::prelude::sim_assert_eq;

use crate::condition_encoding::{
    AbsenceDefaults, ConditionFragmentCache, ConditionPolarity, helm_truthy_definition_schema,
};

fn decision(paths: &[String]) -> eyre::Result<ConditionalGuard> {
    let atoms = paths
        .iter()
        .map(|path| {
            RequirementAtom::Value(Guard::Truthy {
                path: ValuesPath::parse(path),
            })
        })
        .collect::<Vec<_>>();
    let predicate = Predicate::all(paths.iter().map(Predicate::truthy_path).collect());
    let mut work = RequirementBdd::WORK_BUDGET;
    let mut bdd = RequirementBdd::new(&atoms, &mut work).ok_or_eyre("decision inventory")?;
    let root = bdd
        .predicate(&predicate)
        .ok_or_eyre("decision conjunction")?;
    bdd.finish(root).ok_or_eyre("decision output")
}

fn encode(
    cache: &mut ConditionFragmentCache,
    guard: ConditionalGuard,
    absence: AbsenceDefaults<'_>,
) -> Value {
    let clauses = cache.clauses(
        &[guard],
        &[],
        absence.dependency_refill,
        absence,
        ConditionPolarity::Narrow,
    );
    crate::schema_node::SchemaNode::all_of(clauses).into_value()
}

#[test]
fn terminal_decisions_share_prior_nodes_without_changing_conditional_identities() -> eyre::Result<()>
{
    let empty = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &roots,
    };
    let mut pooled = ConditionFragmentCache::new();
    let mut legacy = ConditionFragmentCache::new();
    // Equal bodies keep distinct names while conditional host identities are being chosen.
    for cache in [&mut pooled, &mut legacy] {
        let first = encode(
            cache,
            ConditionalGuard::Not(Arc::new(ConditionalGuard::Truthy {
                path: ValuesPath::parse("q"),
            })),
            absence,
        );
        let second = encode(
            cache,
            ConditionalGuard::Not(Arc::new(ConditionalGuard::With {
                path: ValuesPath::parse("q"),
            })),
            absence,
        );
        sim_assert_eq!(have:first,want:json!({"$ref":"#/definitions/helm-condition-0"}));
        sim_assert_eq!(have:second,want:json!({"$ref":"#/definitions/helm-condition-1"}));
        encode(cache, decision(&["z".to_owned()])?, absence);
    }
    let prior = pooled.definitions.clone();
    pooled.begin_terminal_conditions();
    let mut pooled_conditions = Vec::new();
    let mut legacy_conditions = Vec::new();
    for path in ["a", "b"] {
        let guard = decision(&[path.to_owned(), "z".to_owned()])?;
        pooled_conditions.push(encode(&mut pooled, guard.clone(), absence));
        legacy_conditions.push(encode(&mut legacy, guard, absence));
    }
    let previous = pooled
        .definitions
        .iter()
        .filter(|(name, _)| prior.contains_key(*name))
        .map(|(name, body)| (name.clone(), body.clone()))
        .collect::<BTreeMap<_, _>>();
    sim_assert_eq!(have:previous,want:prior);
    let z = json!({"$ref":"#/definitions/helm-condition-2"});
    let mut expected = prior;
    for (index, path) in [(3, "a"), (4, "b")] {
        expected.insert(format!("helm-condition-{index}"),json!({"if":{"type":"object","properties":{path:{"$ref":"#/definitions/t"}},"required":[path]},"then":z,"else":false}));
    }
    sim_assert_eq!(have:&pooled.definitions,want:&expected);
    sim_assert_eq!(have:&pooled_conditions,want:&vec![json!({"$ref":"#/definitions/helm-condition-3"}),json!({"$ref":"#/definitions/helm-condition-4"})]);
    let schema = |cache: ConditionFragmentCache, conditions: Vec<Value>| {
        let mut definitions = cache.definitions;
        definitions.insert("t".to_owned(), helm_truthy_definition_schema());
        json!({"definitions":definitions,"allOf":conditions.into_iter().map(|condition|json!({"if":condition,"then":false})).collect::<Vec<_>>()})
    };
    let actual = schema(pooled, pooled_conditions);
    let oracle = schema(legacy, legacy_conditions);
    let pooled = jsonschema::validator_for(&actual)?;
    let legacy = jsonschema::validator_for(&oracle)?;
    for (document, valid) in [
        (json!({"a":true,"z":true}), false),
        (json!({"b":true,"z":true}), false),
        (json!({"a":true,"b":true,"z":false}), true),
        (json!({"z":true}), true),
    ] {
        sim_assert_eq!(have:pooled.is_valid(&document),want:valid);
        sim_assert_eq!(have:legacy.is_valid(&document),want:valid);
    }
    Ok(())
}

#[test]
fn terminal_decision_storage_follows_distinct_nodes_and_full_encoding_context() -> eyre::Result<()>
{
    let empty = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &roots,
    };
    let mut cache = ConditionFragmentCache::new();
    cache.begin_terminal_conditions();
    let suffix = (0..32)
        .map(|index| format!("z{index:02}"))
        .collect::<Vec<_>>();
    // Every independently allocated graph has a distinct root and the same 32-node suffix.
    for index in 0..2_000 {
        let mut paths = vec![format!("a{index:04}")];
        paths.extend(suffix.iter().cloned());
        encode(&mut cache, decision(&paths)?, absence);
    }
    sim_assert_eq!(have:cache.definitions.len(),want:2_032);
    let guard = decision(&["z00".to_owned()])?;
    let ordinary = encode(&mut cache, guard.clone(), absence);
    let refilled = serde_yaml::to_value(json!({"z00":true}))?;
    let refilled_absence = AbsenceDefaults {
        runtime_defaults: &refilled,
        ..absence
    };
    let changed = encode(&mut cache, guard, refilled_absence);
    eyre::ensure!(
        ordinary != changed,
        "runtime defaults must keep distinct decision bodies"
    );
    cache
        .definitions
        .insert("t".to_owned(), helm_truthy_definition_schema());
    let schema = json!({"definitions":cache.definitions,"if":ordinary,"then":false});
    let changed = json!({"definitions":schema["definitions"],"if":changed,"then":false});
    sim_assert_eq!(have:jsonschema::validator_for(&schema)?.is_valid(&json!({})),want:true);
    sim_assert_eq!(have:jsonschema::validator_for(&changed)?.is_valid(&json!({})),want:false);
    Ok(())
}
