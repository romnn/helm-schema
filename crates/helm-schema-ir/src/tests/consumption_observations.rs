use std::collections::BTreeSet;

use ahash::AHashMap;
use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_core::{Guard, Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::{ConsumptionDomain, FrozenConsumption, FrozenRequirementGuard};

impl FrozenConsumption {
    pub(crate) fn test_condition(&self) -> Predicate {
        raw_condition(&self.guard, &mut AHashMap::new()).normalize_boolean()
    }
}

fn raw_condition(
    guard: &FrozenRequirementGuard,
    memo: &mut AHashMap<*const FrozenRequirementGuard, Predicate>,
) -> Predicate {
    let key = std::ptr::from_ref(guard);
    if let Some(predicate) = memo.get(&key) {
        return predicate.clone();
    }
    let predicate = match guard {
        FrozenRequirementGuard::False => Predicate::False,
        FrozenRequirementGuard::True => Predicate::True,
        FrozenRequirementGuard::Predicate(predicate) => predicate.clone(),
        FrozenRequirementGuard::Choice(choice) => Predicate::approximate(
            format!(
                "runtime choice {}:{}",
                choice.scope.address(),
                choice.decision
            ),
            BTreeSet::new(),
        ),
        FrozenRequirementGuard::Select { test, yes, no } => {
            let test = raw_condition(test, memo);
            let yes = raw_condition(yes, memo);
            let no = raw_condition(no, memo);
            match (&yes, &no) {
                _ if yes == no => yes,
                _ if yes == Predicate::True && no == Predicate::False => test,
                _ if yes == Predicate::False && no == Predicate::True => test.negated(),
                _ if no == Predicate::False => Predicate::all(vec![test, yes]),
                _ if yes == Predicate::False => Predicate::all(vec![test.negated(), no]),
                _ => Predicate::Or(vec![
                    Predicate::all(vec![test.clone(), yes]),
                    Predicate::all(vec![test.negated(), no]),
                ]),
            }
        }
    };
    memo.insert(key, predicate.clone());
    predicate
}

fn observations(source: &str) -> Vec<FrozenConsumption> {
    let db = crate::analysis_db::IrAnalysisDb::new(&helm_schema_ast::DefineIndex::new());
    crate::value::tests::execute_document(source, None, &db)
        .observed_facts()
        .consumptions
        .clone()
}

#[test]
fn grouping_payload_exhaustion_omits_the_emitted_requirement() -> eyre::Result<()> {
    use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
    use serde_json::json;

    let path = ValuesPath::parse("config");
    let uses = vec![crate::ContractUse::new(
        path,
        crate::YamlPath(Vec::new()),
        crate::ValueKind::Serialized,
        Vec::new(),
        None,
    )];
    let roots = BTreeSet::new();
    let memo = helm_schema_core::PredicateMemo::new();
    let schema_for = |facts: &crate::observed_facts::ObservedFacts| {
        let signals = crate::contract_signal_builder::derive_schema_signals_from_contract_parts(
            &uses, facts, &roots, &memo,
        );
        generate_values_schema(ValuesSchemaInput::new(&signals, &NoProvider))
    };
    let mut facts = crate::observed_facts::ObservedFacts {
        consumptions: observations(
            r#"{{ if .Release.IsUpgrade }}{{ $_ := hasKey .Values.config "x" }}{{ else }}{{ $_ := hasKey .Values.config "x" }}{{ end }}"#,
        ),
        ..crate::observed_facts::ObservedFacts::default()
    };
    let active = schema_for(&facts);
    assert!(!jsonschema::validator_for(&active)?.is_valid(&json!({"config": 7})));
    for consumption in &mut facts.consumptions {
        consumption.scope = "s".repeat(8193);
    }
    let capped = schema_for(&facts);
    let expected = schema_for(&crate::observed_facts::ObservedFacts::default());
    sim_assert_eq!(have: &capped, want: &expected);
    assert!(jsonschema::validator_for(&capped)?.is_valid(&json!({"config": 7})));
    Ok(())
}

#[test]
fn range_header_domain_is_recorded_before_the_empty_body() {
    let consumptions = observations("{{ range .Values.items }}{{ end }}");
    assert!(
        consumptions
            .iter()
            .any(|consumption| consumption.path == ValuesPath::parse("items")
                && matches!(
                    consumption.domain,
                    ConsumptionDomain::Known {
                        preimage: helm_schema_core::FailValueRequirement::Iterable {
                            allow_integer: true
                        },
                        requires_presence: false
                    }
                ))
    );
}

#[test]
fn strict_argument_and_comparison_domains_have_their_own_observations() {
    let strict = observations("{{ upper .Values.text }}");
    assert!(strict.iter().any(|consumption| consumption.path == ValuesPath::parse("text") && matches!(&consumption.domain, ConsumptionDomain::Known { preimage: helm_schema_core::FailValueRequirement::SchemaTypeEvenNull(kind), requires_presence: true } if kind == "string")));
    let comparison = observations("{{ eq .Values.enabled true }}");
    assert!(
        comparison
            .iter()
            .any(|consumption| matches!(consumption.domain, ConsumptionDomain::KnownFailure))
    );
}

#[test]
fn quoting_is_known_any_and_opaque_text_requires_a_positive_placement() {
    let quote = observations("{{ quote .Values.debug }}");
    assert!(
        quote
            .iter()
            .any(|consumption| consumption.path == ValuesPath::parse("debug")
                && matches!(consumption.domain, ConsumptionDomain::KnownAny))
    );
    assert!(
        !quote
            .iter()
            .any(|consumption| matches!(consumption.domain, ConsumptionDomain::Opaque { .. }))
    );
    let opaque = observations(indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config: |
            value={{ .Values.value }}
    "});
    assert!(
        opaque
            .iter()
            .any(|consumption| consumption.path == ValuesPath::parse("value")
                && matches!(consumption.domain, ConsumptionDomain::Opaque { .. }))
    );
}

#[test]
fn selected_value_and_ambient_guards_survive_recording() {
    let consumptions = observations(
        "{{ if .Values.enabled }}{{ upper (ternary .Values.left .Values.right .Values.choose) }}{{ end }}",
    );
    let left = consumptions.iter().find(|consumption| {
        consumption.path == ValuesPath::parse("left")
            && matches!(consumption.domain, ConsumptionDomain::Known { .. })
    });
    assert!(left.is_some(), "missing selected raw operand");
    if let Some(left) = left {
        let guard = left.test_condition();
        let expected = Predicate::all(vec![
            Predicate::from(Guard::Truthy {
                path: ValuesPath::parse("enabled"),
            }),
            Predicate::from(Guard::Truthy {
                path: ValuesPath::parse("choose"),
            }),
            Predicate::from(Guard::TypeIs {
                path: ValuesPath::parse("choose"),
                schema_type: "boolean".to_owned(),
            }),
        ]);
        sim_assert_eq!(have: guard, want: expected.normalize_boolean());
    }
}

#[test]
fn unrelated_runtime_choices_keep_distinct_identities() -> eyre::Result<()> {
    let consumptions = observations(
        "{{ if .Release.IsUpgrade }}{{ .Values.config.value }}{{ end }}{{ if not .Release.IsInstall }}{{ .Values.config.value }}{{ end }}",
    );
    let mut choices = BTreeSet::new();
    let mut identities = super::RequirementIdentities::default();
    for consumption in &consumptions {
        let mut atoms = BTreeSet::new();
        let mut work = 16_384;
        consumption
            .atoms(&mut atoms, &mut work, &mut identities)
            .ok_or_eyre("guard inventory exhausted")?;
        for atom in atoms {
            if let helm_schema_core::RequirementAtom::Choice(choice) = atom {
                choices.insert(choice);
            }
        }
    }
    assert!(
        choices.len() >= 2,
        "independent runtime decisions were correlated: {choices:?}"
    );
    Ok(())
}

#[test]
fn independent_interpreters_do_not_share_choice_identities() -> eyre::Result<()> {
    let source = "{{ if .Release.IsUpgrade }}{{ .Values.config.value }}{{ end }}";
    let left = observations(source);
    let right = observations(source);
    let mut identities = super::RequirementIdentities::default();
    let mut sets = Vec::new();
    for consumptions in [&left, &right] {
        let mut choices = BTreeSet::new();
        for consumption in consumptions {
            let mut atoms = BTreeSet::new();
            let mut work = 16_384;
            consumption
                .atoms(&mut atoms, &mut work, &mut identities)
                .ok_or_eyre("guard inventory exhausted")?;
            choices.extend(atoms.into_iter().filter_map(|atom| match atom {
                helm_schema_core::RequirementAtom::Choice(choice) => Some(choice),
                helm_schema_core::RequirementAtom::Value(_) => None,
            }));
        }
        assert!(!choices.is_empty(), "missing unresolved runtime decision");
        sets.push(choices);
    }
    let [left, right] = sets.as_slice() else {
        eyre::bail!("missing interpreter choices");
    };
    assert!(
        left.is_disjoint(right),
        "independent interpreter choices were correlated"
    );
    Ok(())
}

#[derive(Debug)]
struct NoProvider;

impl helm_schema_core::ResourceSchemaOracle for NoProvider {
    fn schema_fragment_for_use(
        &self,
        _: &helm_schema_core::ProviderSchemaUse,
    ) -> Option<helm_schema_core::ProviderSchemaFragment> {
        None
    }
}

fn export_validator_case(
    name: &str,
    schema: &serde_json::Value,
    document: &serde_json::Value,
) -> eyre::Result<()> {
    let Some(directory) = std::env::var_os("DECL1_VALIDATOR_EXPORT_DIR") else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).wrap_err("create validator export directory")?;
    std::fs::write(
        directory.join(format!("{name}.schema.json")),
        serde_json::to_vec_pretty(schema)?,
    )
    .wrap_err_with(|| format!("export {name} schema"))?;
    std::fs::write(
        directory.join(format!("{name}.document.json")),
        serde_json::to_vec_pretty(document)?,
    )
    .wrap_err_with(|| format!("export {name} document"))?;
    Ok(())
}

#[test]
fn opaque_complementary_cones_reach_the_emitted_schema() -> eyre::Result<()> {
    use crate::value::guard::RuntimeGuard;
    use crate::value::{Coverage, DecisionEdge, DocumentExecution, FileOutput, State, Value};
    use helm_schema_core::{
        ConditionalGuard, ContractRequirementImplication, ContractRequirementTarget,
        ContractSchemaSignals, FailValueRequirement,
    };
    use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
    use serde_json::json;
    let state = State::new(Value::nil());
    let arena = std::rc::Rc::clone(&state.heap.arena);
    let mut parity = RuntimeGuard::False;
    for _ in 0..10 {
        let test = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
        let atom = arena.runtime_path(&[DecisionEdge { test, taken: true }]);
        let opposite = arena.complement_runtime_guard(&parity);
        parity = arena.select_runtime_guard(&atom, &opposite, &parity);
    }
    let test = arena.budget_test(parity);
    let path = ValuesPath::parse("config");
    let mut exits = Vec::new();
    for taken in [true, false] {
        let mut branch = state.fork(std::rc::Rc::clone(&test), taken);
        branch.record_requirement(
            &Value::input(path.clone()),
            &Predicate::True,
            crate::value::query::RequirementKind::MapArgument { allows_nil: false },
        );
        exits.extend(crate::value::eval::normal(branch, Value::nil()));
    }
    let execution = DocumentExecution::new(exits, FileOutput::Manifest);
    let uses = vec![crate::ContractUse::new(
        path.clone(),
        crate::YamlPath(Vec::new()),
        crate::ValueKind::Serialized,
        Vec::new(),
        None,
    )];
    let roots = BTreeSet::new();
    let memo = helm_schema_core::PredicateMemo::new();
    let signals = crate::contract_signal_builder::derive_schema_signals_from_contract_parts(
        &uses,
        execution.observed_facts(),
        &roots,
        &memo,
    );
    let schema = generate_values_schema(ValuesSchemaInput::new(&signals, &NoProvider));
    let baseline = crate::contract_signal_builder::derive_schema_signals_from_contract_parts(
        &uses,
        &crate::observed_facts::ObservedFacts::default(),
        &roots,
        &memo,
    );
    let mut expected_evidence = baseline.schema_evidence_by_value_path().clone();
    expected_evidence
        .get_mut(&path)
        .ok_or_eyre("missing config metadata")?
        .requirement_implications
        .push(ContractRequirementImplication::new(
            Vec::new(),
            ContractRequirementTarget::Value,
            vec![FailValueRequirement::MemberHost {
                handled_kinds: Vec::new(),
                complete_domain: true,
            }],
        ));
    let expected = ContractSchemaSignals::new(
        expected_evidence,
        vec![vec![ConditionalGuard::Absent { path }]],
    );
    let expected = generate_values_schema(ValuesSchemaInput::new(&expected, &NoProvider));
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&json!({"config": {}})));
    for document in [
        json!({}),
        json!({"config": null}),
        json!({"config": 7}),
        json!({"config": true}),
    ] {
        assert!(
            !validator.is_valid(&document),
            "opaque-arm aggregation failed: {document}"
        );
    }
    sim_assert_eq!(have: &schema, want: &expected);
    export_validator_case("fold-complement", &schema, &json!({"config": {}}))?;
    Ok(())
}

#[test]
fn a_capped_fold_omits_the_requirement_in_the_emitted_schema() -> eyre::Result<()> {
    use crate::value::{Coverage, DocumentExecution, FileOutput, State, Value};
    use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
    use serde_json::json;

    let flags = (0..130)
        .map(|index| ValuesPath::parse(&format!("flag{index:03}")))
        .collect::<Vec<_>>();
    let active = Predicate::all(
        flags
            .iter()
            .map(|path| Predicate::from(Guard::Truthy { path: path.clone() }))
            .collect(),
    );
    let state = State::new(Value::nil());
    let test = state
        .heap
        .arena
        .decision(active.clone(), active.negated(), Coverage::Exact);
    let mut state = state.fork(test, true);
    let config = ValuesPath::parse("config");
    state.record_requirement(
        &Value::input(config.clone()),
        &Predicate::True,
        crate::value::query::RequirementKind::MapArgument { allows_nil: false },
    );
    let execution = DocumentExecution::new(
        crate::value::eval::normal(state, Value::nil()),
        FileOutput::Manifest,
    );
    let observed = crate::observed_facts::ObservedFacts {
        consumptions: execution.observed_facts().consumptions.clone(),
        ..crate::observed_facts::ObservedFacts::default()
    };
    assert!(
        !observed.consumptions.is_empty(),
        "capture must precede fold exhaustion"
    );
    let uses = std::iter::once(config)
        .chain(flags)
        .map(|path| {
            crate::ContractUse::new(
                path,
                crate::YamlPath(Vec::new()),
                crate::ValueKind::Serialized,
                Vec::new(),
                None,
            )
        })
        .collect::<Vec<_>>();
    let roots = BTreeSet::new();
    let memo = helm_schema_core::PredicateMemo::new();
    let schema = |observed| {
        let signals = crate::contract_signal_builder::derive_schema_signals_from_contract_parts(
            &uses, observed, &roots, &memo,
        );
        generate_values_schema(ValuesSchemaInput::new(&signals, &NoProvider))
    };
    let actual = schema(&observed);
    let expected = schema(&crate::observed_facts::ObservedFacts::default());
    sim_assert_eq!(have: &actual, want: &expected);
    let mut document = serde_json::Map::from_iter([("config".to_owned(), json!(7))]);
    for index in 0..130 {
        document.insert(format!("flag{index:03}"), json!(true));
    }
    let document = serde_json::Value::Object(document);
    assert!(jsonschema::validator_for(&actual)?.is_valid(&document));
    export_validator_case("cap-omission", &actual, &document)?;
    Ok(())
}
