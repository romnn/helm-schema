use std::collections::BTreeSet;
use std::fmt::Write as _;

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
        FrozenRequirementGuard::False | FrozenRequirementGuard::Unknown(_) => Predicate::False,
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

/// (6a1 rework item 4, ADR 0072 §10) A consumption whose ordered grouping inventory exceeds its
/// allowance keeps its demand: same-key occurrences still fold together, so the complementary
/// `IsUpgrade` arms still prove the unconditional map demand Helm's `hasKey` makes.
#[test]
fn grouping_payload_exhaustion_keeps_the_emitted_requirement() -> eyre::Result<()> {
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
    sim_assert_eq!(have: &capped, want: &active);
    assert!(!jsonschema::validator_for(&capped)?.is_valid(&json!({"config": 7})));
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
            .atoms(
                &mut atoms,
                &mut work,
                &mut crate::value::GuardWalk::default(),
                &mut identities,
            )
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
                .atoms(
                    &mut atoms,
                    &mut work,
                    &mut crate::value::GuardWalk::default(),
                    &mut identities,
                )
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

struct GuardedMapCase {
    actual: serde_json::Value,
    unconstrained: serde_json::Value,
}

fn guarded_map_case(flag_count: usize) -> GuardedMapCase {
    use crate::value::{Coverage, DocumentExecution, FileOutput, State, Value};
    use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};

    let flags = (0..flag_count)
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
        .chain(flags.iter().cloned())
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
    GuardedMapCase {
        actual: schema(&observed),
        unconstrained: schema(&crate::observed_facts::ObservedFacts::default()),
    }
}

/// Exact cofactors retain a recoverable demand; only an exhausted requirement widens.
#[test]
fn capped_folds_keep_exact_cofactors_and_omit_only_exhausted_requirements() -> eyre::Result<()> {
    use serde_json::json;

    for flag_count in [130, 512] {
        let GuardedMapCase {
            actual,
            unconstrained: mut expected,
        } = guarded_map_case(flag_count);
        if flag_count == 130 {
            // Helm consumes `config` only when every flag is truthy.
            // This oracle is independent of the fold's graph encoding and cofactor partition.
            let truthy = json!({"anyOf": [
                {"const": true},
                {"type": "number", "not": {"const": 0}},
                {"type": "string", "minLength": 1},
                {"type": "array", "minItems": 1},
                {"type": "object", "minProperties": 1}
            ]});
            let names = (0..flag_count)
                .map(|index| format!("flag{index:03}"))
                .collect::<Vec<_>>();
            let properties = names
                .iter()
                .map(|name| (name.clone(), truthy.clone()))
                .collect::<serde_json::Map<_, _>>();
            let condition = json!([{
                "if": {"type": "object", "properties": properties, "required": names},
                "then": {"type": "object", "properties": {"config": {"type": "object"}}, "required": ["config"]}
            }]);
            expected
                .as_object_mut()
                .ok_or_eyre("expected schema object")?
                .insert("allOf".to_owned(), condition);
        } else {
            // A larger conjunction still exceeds the bounded cofactor refinement.
            // Its own requirement widens; the complete open schema remains the expected result.
            sim_assert_eq!(have: &actual, want: &expected);
        }
        let validator = jsonschema::validator_for(&actual)?;
        let oracle = jsonschema::validator_for(&expected)?;
        let check = |document: &serde_json::Value, accepted: bool| {
            sim_assert_eq!(have: validator.is_valid(document), want: oracle.is_valid(document));
            sim_assert_eq!(have: validator.is_valid(document), want: accepted);
        };
        let mut document = serde_json::Map::from_iter([("config".to_owned(), json!(7))]);
        for index in 0..flag_count {
            document.insert(format!("flag{index:03}"), json!(true));
        }
        let active_bad = serde_json::Value::Object(document.clone());
        check(&active_bad, flag_count != 130);
        export_validator_case(
            if flag_count == 130 {
                "cap-cofactor-recovery"
            } else {
                "cap-omission"
            },
            &actual,
            &active_bad,
        )?;
        document.insert("config".to_owned(), json!({}));
        check(&serde_json::Value::Object(document.clone()), true);
        document.insert("config".to_owned(), json!(7));
        for index in 0..flag_count {
            let flag = format!("flag{index:03}");
            document.insert(flag.clone(), json!(false));
            check(&serde_json::Value::Object(document.clone()), true);
            document.insert(flag, json!(true));
        }
        document.remove("flag000");
        check(&serde_json::Value::Object(document), true);
    }
    Ok(())
}

/// One requirement group's rows never depend on the groups folded before it or on the order the
/// consumptions arrive in (ADR 0072 §10). Three hundred member hosts read under eight exclusive pairs
/// each exhaust their union and fold per occurrence. Under one chart-wide allowance they spent
/// it all before the later `zz` group, which then lost its rows.
#[test]
fn a_requirement_group_keeps_its_rows_after_exhausted_groups_in_any_order() -> eyre::Result<()> {
    const HOSTS: usize = 300;
    const PAIRS: usize = 8;
    let mut hosts = String::new();
    let mut victim = String::new();
    for pair in 1..=PAIRS {
        for host in 1..=HOSTS {
            write!(
                hosts,
                "{{{{- if and .Values.a{pair} .Values.b{pair} }}}}{{{{ $_ := .Values.h{host:03}.port }}}}{{{{ end }}}}"
            )?;
        }
        write!(
            victim,
            "{{{{- if and .Values.a{pair} .Values.b{pair} }}}}{{{{ $_ := .Values.zz | trunc 3 }}}}{{{{ end }}}}"
        )?;
    }
    let signals = |consumptions: Vec<FrozenConsumption>| {
        let facts = crate::observed_facts::ObservedFacts {
            consumptions,
            ..crate::observed_facts::ObservedFacts::default()
        };
        crate::contract_signal_builder::derive_schema_signals_from_contract_parts(
            &[],
            &facts,
            &BTreeSet::new(),
            &helm_schema_core::PredicateMemo::new(),
        )
    };
    let zz = ValuesPath::parse("zz");
    let alone = signals(observations(&victim));
    let after = observations(&format!("{hosts}{victim}"));
    let mut reversed = after.clone();
    reversed.reverse();
    let after = signals(after);
    let reversed = signals(reversed);
    let rows = alone.evidence_for(&zz).ok_or_eyre("zz rows")?;
    assert!(!rows.requirement_implications.is_empty(), "{rows:?}");
    sim_assert_eq!(have: after.evidence_for(&zz), want: Some(rows));
    sim_assert_eq!(have: &reversed, want: &after);
    Ok(())
}

/// (6a1, Sol pre-landing item 5) Whether one consumption fits its grouping allowance never
/// depends on how many other consumptions the chart has (ADR 0072 §10). A member read at a path
/// of twelve long keys fits alone under a charge that grows with the chart's consumption count
/// and not beside a thousand unrelated reads; a short path fits in both.
#[test]
fn a_requirement_fits_its_allowance_whatever_else_the_chart_reads() -> eyre::Result<()> {
    let long =
        ValuesPath::from_segments((0..12).map(|index| format!("{index}{}", "k".repeat(8190))));
    let short = ValuesPath::parse("zz");
    let mut noise = String::new();
    for index in 0..1024 {
        write!(noise, "{{{{ $_ := .Values.h{index:04} | trunc 3 }}}}")?;
    }
    let victims = || {
        let mut consumptions =
            observations("{{ $_ := .Values.zz | trunc 3 }}{{ $_ := .Values.long | trunc 3 }}");
        for consumption in &mut consumptions {
            if consumption.path == ValuesPath::parse("long") {
                consumption.path = long.clone();
            }
        }
        consumptions
    };
    let signals = |consumptions: Vec<FrozenConsumption>| {
        let facts = crate::observed_facts::ObservedFacts {
            consumptions,
            ..crate::observed_facts::ObservedFacts::default()
        };
        crate::contract_signal_builder::derive_schema_signals_from_contract_parts(
            &[],
            &facts,
            &BTreeSet::new(),
            &helm_schema_core::PredicateMemo::new(),
        )
    };
    let alone = signals(victims());
    let mut crowded = observations(&noise);
    crowded.extend(victims());
    let crowded = signals(crowded);
    let rows = alone.evidence_for(&short).ok_or_eyre("zz rows")?;
    assert!(!rows.requirement_implications.is_empty(), "{rows:?}");
    sim_assert_eq!(have: crowded.evidence_for(&short), want: Some(rows));
    sim_assert_eq!(have: crowded.evidence_for(&long), want: alone.evidence_for(&long));
    Ok(())
}

/// (6a1, Sol pre-landing item 5) An opaque use whose grouping inventory exceeds its allowance
/// stays a use: the declared default it would assert is replaced, never restored (ADR 0072 §10).
#[test]
fn an_opaque_use_over_its_allowance_stays_a_use() {
    let source = indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config: |
            value={{ .Values.value }}
    "};
    let signals = |consumptions: Vec<FrozenConsumption>| {
        let facts = crate::observed_facts::ObservedFacts {
            consumptions,
            ..crate::observed_facts::ObservedFacts::default()
        };
        crate::contract_signal_builder::derive_schema_signals_from_contract_parts(
            &[],
            &facts,
            &BTreeSet::new(),
            &helm_schema_core::PredicateMemo::new(),
        )
    };
    let value = ValuesPath::parse("value");
    let mut exhausted = observations(source);
    assert!(exhausted.iter().any(|consumption| consumption.path == value
        && matches!(consumption.domain, ConsumptionDomain::Opaque { .. })));
    for consumption in &mut exhausted {
        consumption.scope = "s".repeat(8193);
    }
    let uses = |signals: &helm_schema_core::ContractSchemaSignals| {
        signals
            .evidence_for(&value)
            .and_then(|evidence| evidence.opaque_consumption_guards.as_ref())
            .is_some_and(|guards| !guards.is_empty())
    };
    assert!(uses(&signals(observations(source))));
    assert!(uses(&signals(exhausted)));
}

/// Exporting many earlier consumptions cannot spend the later consumption's freeze allowance.
#[test]
fn requirement_freezing_is_local_to_each_consumption() -> eyre::Result<()> {
    use crate::value::{Coverage, DocumentExecution, FileOutput, State, Value};

    let mut state = State::new(Value::unknown());
    for index in 0..40 {
        let predicate = Predicate::truthy_path(format!("flag{index:02}"));
        let test =
            state
                .heap
                .arena
                .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        state = state.fork(test, true);
    }
    state.record_requirement(
        &Value::input(ValuesPath::parse("noise")),
        &Predicate::True,
        crate::value::query::RequirementKind::MapArgument { allows_nil: false },
    );
    let noise = state
        .consumptions
        .iter()
        .next()
        .ok_or_eyre("noise consumption")?
        .clone();
    state.consumptions = std::iter::repeat_n(noise, 4000).collect();
    let victim = ValuesPath::parse("victim");
    state.record_requirement(
        &Value::input(victim.clone()),
        &Predicate::True,
        crate::value::query::RequirementKind::MapArgument { allows_nil: false },
    );
    let execution = DocumentExecution::new(
        crate::value::eval::normal(state, Value::nil()),
        FileOutput::Manifest,
    );
    assert!(
        execution
            .observed_facts()
            .consumptions
            .iter()
            .any(|item| item.path == victim)
    );
    Ok(())
}

/// Runtime decision IDs retain their ordinals when guards are inventoried in another order.
#[test]
fn choice_ordinals_do_not_follow_inventory_traversal() -> eyre::Result<()> {
    use super::{FrozenChoice, RequirementIdentities, RequirementScope};
    let scope = RequirementScope::default();
    let first = FrozenChoice {
        scope: scope.clone(),
        decision: 17,
    };
    let second = FrozenChoice {
        scope,
        decision: 31,
    };
    let mut forward = RequirementIdentities::default();
    let a = forward.choice(&first).ok_or_eyre("first forward")?;
    let b = forward.choice(&second).ok_or_eyre("second forward")?;
    let mut reverse = RequirementIdentities::default();
    let reverse_b = reverse.choice(&second).ok_or_eyre("second reverse")?;
    let reverse_a = reverse.choice(&first).ok_or_eyre("first reverse")?;
    sim_assert_eq!(have: a, want: reverse_a);
    sim_assert_eq!(have: b, want: reverse_b);
    Ok(())
}

/// An exhausted guard freeze preserves authoring use while withholding its runtime claim.
#[test]
fn a_failed_requirement_freeze_keeps_the_opaque_use() -> eyre::Result<()> {
    use crate::value::{State, Value};
    let mut state = State::new(Value::unknown());
    let condition = crate::value::output::RuntimeCondition::at_requirement(&state);
    state.record_consumption(
        ValuesPath::parse("value"),
        ConsumptionDomain::Opaque {
            reason: "unknown authoring use".to_owned(),
        },
        &[],
        &Predicate::True,
        &condition,
        &[],
    );
    let consumption = state.consumptions.iter().next().ok_or_eyre("opaque use")?;
    let frozen = consumption.freeze(&mut 0, &mut AHashMap::new());
    sim_assert_eq!(have: frozen.domain, want: consumption.domain.clone());
    sim_assert_eq!(have: frozen.guard.as_ref(), want: &FrozenRequirementGuard::Unknown(consumption.scope_identity.clone()));
    Ok(())
}

/// Remapping an earlier consumption cannot erase a later consumption when its allowance expires.
#[test]
fn requirement_path_mapping_is_local_to_each_consumption() -> eyre::Result<()> {
    use std::sync::Arc;
    let mut base = observations("{{ $_ := upper .Values.noise }}")
        .into_iter()
        .find(|item| matches!(item.domain, ConsumptionDomain::Known { .. }))
        .ok_or_eyre("noise requirement")?;
    let mut guard = Arc::new(FrozenRequirementGuard::True);
    for index in 0..100 {
        guard = Arc::new(FrozenRequirementGuard::Select {
            test: Arc::new(FrozenRequirementGuard::Predicate(Predicate::truthy_path(
                format!("flag{index:03}"),
            ))),
            yes: guard,
            no: Arc::new(FrozenRequirementGuard::False),
        });
    }
    base.guard = guard;
    let mut consumptions = std::iter::repeat_n(base, 5000).collect::<Vec<_>>();
    let victim = observations("{{ $_ := upper .Values.victim }}");
    let expected = consumptions.len() + victim.len();
    consumptions.extend(victim);
    super::map_consumption_paths(&mut consumptions, &mut |path| path, false);
    sim_assert_eq!(have: consumptions.len(), want: expected);
    assert!(
        consumptions
            .iter()
            .any(|item| item.path == ValuesPath::parse("victim"))
    );
    Ok(())
}

impl FrozenConsumption {
    pub(crate) fn test_complementary_guards(choice: Option<Predicate>) -> eyre::Result<Vec<Self>> {
        use std::sync::Arc;
        let base = observations("{{ $_ := upper .Values.bad }}")
            .into_iter()
            .find(|item| matches!(item.domain, ConsumptionDomain::Known { .. }))
            .ok_or_eyre("string consumption")?;
        let choice = Arc::new(choice.map_or_else(
            || {
                FrozenRequirementGuard::Choice(super::FrozenChoice {
                    scope: base.scope_identity.clone(),
                    decision: 17,
                })
            },
            FrozenRequirementGuard::Predicate,
        ));
        let positive = Arc::clone(&choice);
        let negative = Arc::new(FrozenRequirementGuard::Select {
            test: choice,
            yes: Arc::new(FrozenRequirementGuard::False),
            no: Arc::new(FrozenRequirementGuard::True),
        });
        Ok([positive, negative]
            .into_iter()
            .map(|branch| {
                let mut occurrence = base.clone();
                occurrence.guard = Arc::new(FrozenRequirementGuard::Select {
                    test: Arc::new(FrozenRequirementGuard::Predicate(Predicate::truthy_path(
                        "p",
                    ))),
                    yes: branch,
                    no: Arc::new(FrozenRequirementGuard::False),
                });
                occurrence
            })
            .collect())
    }

    /// This occurrence under the conjunction of `choices` fresh unresolved choices of its own
    /// scope: a guard with no values atom to split on.
    pub(crate) fn test_under_choices(&self, choices: u64) -> Self {
        use std::sync::Arc;
        let mut guard = Arc::clone(&self.guard);
        for decision in 0..choices {
            guard = Arc::new(FrozenRequirementGuard::Select {
                test: Arc::new(FrozenRequirementGuard::Choice(super::FrozenChoice {
                    scope: self.scope_identity.clone(),
                    decision: 1_000 + decision,
                })),
                yes: guard,
                no: Arc::new(FrozenRequirementGuard::False),
            });
        }
        let mut result = self.clone();
        result.guard = guard;
        result
    }

    pub(crate) fn test_under(&self, predicate: Predicate) -> Self {
        let mut result = self.clone();
        result.guard = std::sync::Arc::new(FrozenRequirementGuard::Select {
            test: std::sync::Arc::new(FrozenRequirementGuard::Predicate(predicate)),
            yes: std::sync::Arc::clone(&self.guard),
            no: std::sync::Arc::new(FrozenRequirementGuard::False),
        });
        result
    }

    pub(crate) fn test_literal_true(&self) -> Self {
        let mut result = self.clone();
        result.guard = std::sync::Arc::new(FrozenRequirementGuard::True);
        result
    }
}

/// A cached predicate pays the same work as a cold predicate in the next item's allowance.
#[test]
fn requirement_freezing_charges_the_same_work_warm_or_cold() -> eyre::Result<()> {
    use crate::value::{Coverage, State, Value};
    let mut state = State::new(Value::unknown());
    let mut guards = Vec::new();
    for prefix in ["a", "b"] {
        let predicate = Predicate::all(
            (0..40)
                .map(|index| Predicate::truthy_path(format!("{prefix}{index:02}")))
                .collect(),
        );
        let test =
            state
                .heap
                .arena
                .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        state = state.fork(test, true);
        state.record_requirement(
            &Value::input(ValuesPath::parse("host")),
            &Predicate::True,
            crate::value::query::RequirementKind::MapArgument { allows_nil: false },
        );
        guards.push(
            state
                .consumptions
                .iter()
                .last()
                .ok_or_eyre("guarded consumption")?
                .clone(),
        );
    }
    let first = guards.first().ok_or_eyre("first guard")?;
    let second = guards.last().ok_or_eyre("second guard")?;
    let mut warm = AHashMap::new();
    let warmup = first.freeze(&mut 70, &mut warm);
    assert!(!matches!(
        warmup.guard.as_ref(),
        FrozenRequirementGuard::False | FrozenRequirementGuard::Unknown(_)
    ));
    let cold = second.freeze(&mut 70, &mut AHashMap::new());
    let warm = second.freeze(&mut 70, &mut warm);
    sim_assert_eq!(have: cold.guard.as_ref(), want: &FrozenRequirementGuard::Unknown(second.scope_identity.clone()));
    sim_assert_eq!(have: warm.guard, want: cold.guard);
    Ok(())
}

/// Shared remapping caches cannot lend an item the work paid by an earlier item.
#[test]
fn requirement_mapping_charges_the_same_work_warm_or_cold() {
    use std::sync::Arc;
    let predicate = |prefix| {
        Arc::new(FrozenRequirementGuard::Predicate(Predicate::all(
            (0..40)
                .map(|index| Predicate::truthy_path(format!("{prefix}{index:02}")))
                .collect(),
        )))
    };
    let first = predicate("a");
    let second = Arc::new(FrozenRequirementGuard::Select {
        test: Arc::clone(&first),
        yes: predicate("b"),
        no: Arc::new(FrozenRequirementGuard::False),
    });
    let mut warm = AHashMap::new();
    assert!(
        first
            .map_paths(&mut |path| path, &mut 70, &mut warm)
            .is_some()
    );
    let cold = second.map_paths(&mut |path| path, &mut 70, &mut AHashMap::new());
    let warm = second.map_paths(&mut |path| path, &mut 70, &mut warm);
    assert!(cold.is_none());
    assert!(warm.is_none());
}

/// Predicate measurement must not switch between syntax and DAG cost with the remaining fuel.
#[test]
fn shared_predicate_freezing_uses_a_fixed_measurement_ceiling() -> eyre::Result<()> {
    use crate::value::{Coverage, State, Value};
    let mut predicate = Predicate::truthy_path("flag");
    for _ in 0..8 {
        predicate = Predicate::And(vec![predicate.clone(), predicate]);
    }
    assert!(predicate.syntax_size() > 300);
    assert!(predicate.node_work(300).is_some_and(|work| work < 20));
    let state = State::new(Value::unknown());
    let test = state
        .heap
        .arena
        .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let mut state = state.fork(test, true);
    state.record_requirement(
        &Value::input(ValuesPath::parse("host")),
        &Predicate::True,
        crate::value::query::RequirementKind::MapArgument { allows_nil: false },
    );
    let consumption = state
        .consumptions
        .iter()
        .last()
        .ok_or_eyre("shared predicate consumption")?;
    let mut memo = AHashMap::new();
    let warmup = consumption.freeze(&mut 1024, &mut memo);
    assert!(!matches!(
        warmup.guard.as_ref(),
        FrozenRequirementGuard::False | FrozenRequirementGuard::Unknown(_)
    ));
    let warm = consumption.freeze(&mut 300, &mut memo);
    let cold = consumption.freeze(&mut 300, &mut AHashMap::new());
    sim_assert_eq!(have: cold.guard.as_ref(), want: &FrozenRequirementGuard::Unknown(consumption.scope_identity.clone()));
    sim_assert_eq!(have: warm.guard, want: cold.guard);
    Ok(())
}

/// Mapping charges the same shared predicate even when syntax size exceeds the local allowance.
#[test]
fn shared_predicate_mapping_uses_a_fixed_measurement_ceiling() {
    let mut predicate = Predicate::truthy_path("flag");
    for _ in 0..8 {
        predicate = Predicate::And(vec![predicate.clone(), predicate]);
    }
    let guard = FrozenRequirementGuard::Predicate(predicate);
    let mut memo = AHashMap::new();
    assert!(
        guard
            .map_paths(&mut |path| path, &mut 1024, &mut memo)
            .is_some()
    );
    let warm = guard.map_paths(&mut |path| path, &mut 300, &mut memo);
    let cold = guard.map_paths(&mut |path| path, &mut 300, &mut AHashMap::new());
    assert!(cold.is_none());
    assert!(warm.is_none());
}

/// Ordering roots retains their shared prefix once while preserving every condition.
#[test]
fn ordering_keys_share_storage_for_common_guard_prefixes() -> eyre::Result<()> {
    use super::{RequirementGuardInterner, RequirementIdentities};
    use std::sync::Arc;

    let source = observations("{{ $_ := upper .Values.value }}")
        .into_iter()
        .next()
        .ok_or_eyre("source consumption")?;
    let false_guard = Arc::new(FrozenRequirementGuard::False);
    let mut prefix = Arc::new(FrozenRequirementGuard::True);
    let prefix_depth = 32;
    let roots = 16;
    for index in 0..prefix_depth {
        prefix = Arc::new(FrozenRequirementGuard::Select {
            test: Arc::new(FrozenRequirementGuard::Predicate(Predicate::truthy_path(
                format!("prefix{index:02}"),
            ))),
            yes: prefix,
            no: Arc::clone(&false_guard),
        });
    }
    let occurrences = (0..roots)
        .map(|index| {
            let mut occurrence = source.clone();
            occurrence.guard = Arc::new(FrozenRequirementGuard::Select {
                test: Arc::new(FrozenRequirementGuard::Predicate(Predicate::truthy_path(
                    format!("row{index:02}"),
                ))),
                yes: Arc::clone(&prefix),
                no: Arc::clone(&false_guard),
            });
            occurrence
        })
        .collect::<Vec<_>>();
    let mut identities = RequirementIdentities::default();
    let mut interner = RequirementGuardInterner::default();
    let keys = occurrences
        .iter()
        .map(|item| {
            item.order_key(&mut identities, &mut interner)
                .ok_or_eyre("ordering key")
        })
        .collect::<eyre::Result<Vec<_>>>()?;
    for (occurrence, key) in occurrences.iter().zip(&keys) {
        let canonical = occurrence
            .canonicalized(key, &interner)
            .ok_or_eyre("canonical guard")?;
        sim_assert_eq!(have: canonical.test_condition(), want: occurrence.test_condition());
    }
    let retained = keys.len() + interner.entries.len();
    assert!(
        retained <= 4 * (prefix_depth + roots),
        "{retained} retained nodes for {prefix_depth} shared prefix nodes and {roots} roots"
    );
    Ok(())
}

/// Impossible opaque reads contribute no use evidence; unprojectable reads still do.
#[test]
fn exact_false_opaque_guards_are_distinct_from_unknown_guards() {
    use super::FrozenChoice;
    use std::sync::Arc;

    let source = indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config: |
            value={{ .Values.value }}
    "};
    let value = ValuesPath::parse("value");
    let uses = |consumptions| {
        let facts = crate::observed_facts::ObservedFacts {
            consumptions,
            ..crate::observed_facts::ObservedFacts::default()
        };
        crate::contract_signal_builder::derive_schema_signals_from_contract_parts(
            &[],
            &facts,
            &BTreeSet::new(),
            &helm_schema_core::PredicateMemo::new(),
        )
        .evidence_for(&value)
        .and_then(|evidence| evidence.opaque_consumption_guards.as_ref())
        .is_some_and(|guards| !guards.is_empty())
    };
    let mut unknown = observations(source);
    for occurrence in &mut unknown {
        occurrence.guard = Arc::new(FrozenRequirementGuard::Choice(FrozenChoice {
            scope: occurrence.scope_identity.clone(),
            decision: 991,
        }));
    }
    assert!(uses(unknown));
    let mut impossible = observations(source);
    for occurrence in &mut impossible {
        occurrence.activation.extend([
            Guard::Truthy {
                path: ValuesPath::parse("gate"),
            },
            Guard::Not {
                path: ValuesPath::parse("gate"),
            },
        ]);
    }
    assert!(!uses(impossible));
    let mut bounded = observations(source);
    for occurrence in &mut bounded {
        occurrence.guard = Arc::new(FrozenRequirementGuard::Unknown(
            occurrence.scope_identity.clone(),
        ));
    }
    assert!(uses(bounded.clone()));
    for occurrence in &mut bounded {
        occurrence.activation.extend([
            Guard::Truthy {
                path: ValuesPath::parse("gate"),
            },
            Guard::Not {
                path: ValuesPath::parse("gate"),
            },
        ]);
    }
    assert!(!uses(bounded));
}

/// A rejected ordering walk leaves no partial guard nodes in the cone interner.
#[test]
fn rejected_ordering_inventory_does_not_extend_the_interner() -> eyre::Result<()> {
    use super::{FrozenChoice, RequirementGuardInterner, RequirementIdentities};
    use std::sync::Arc;

    let mut occurrence = observations("{{ $_ := upper .Values.value }}")
        .into_iter()
        .next()
        .ok_or_eyre("source consumption")?;
    let mut level = (0..32_768)
        .map(|index| {
            Arc::new(FrozenRequirementGuard::Predicate(Predicate::truthy_path(
                format!("leaf{index}"),
            )))
        })
        .collect::<Vec<_>>();
    let mut decision = 0;
    while level.len() > 1 {
        let mut parents = Vec::new();
        let (pairs, remainder) = level.as_chunks::<2>();
        eyre::ensure!(remainder.is_empty(), "binary guard children");
        for [yes, no] in pairs {
            parents.push(Arc::new(FrozenRequirementGuard::Select {
                test: Arc::new(FrozenRequirementGuard::Choice(FrozenChoice {
                    scope: occurrence.scope_identity.clone(),
                    decision,
                })),
                yes: Arc::clone(yes),
                no: Arc::clone(no),
            }));
            decision += 1;
        }
        level = parents;
    }
    occurrence.guard = level.pop().ok_or_eyre("guard root")?;
    let mut interner = RequirementGuardInterner::default();
    assert!(
        occurrence
            .order_key(&mut RequirementIdentities::default(), &mut interner)
            .is_none()
    );
    sim_assert_eq!(have: interner.entries.len(), want: 0);
    Ok(())
}

impl FrozenConsumption {
    pub(crate) fn test_impossible_choice(&self) -> Self {
        use std::sync::Arc;
        let mut result = self.clone();
        let choice = Arc::new(FrozenRequirementGuard::Choice(super::FrozenChoice {
            scope: self.scope_identity.clone(),
            decision: 992,
        }));
        result.guard = Arc::new(FrozenRequirementGuard::Select {
            test: Arc::clone(&choice),
            yes: Arc::new(FrozenRequirementGuard::Select {
                test: choice,
                yes: Arc::new(FrozenRequirementGuard::False),
                no: Arc::new(FrozenRequirementGuard::True),
            }),
            no: Arc::new(FrozenRequirementGuard::False),
        });
        result
    }
}

/// A narrowed Boolean branch must discard dead syntax before charging the next inventory.
#[test]
fn cofactors_reduce_dead_branches_before_inventory() -> eyre::Result<()> {
    use std::sync::Arc;
    let mut source = observations("{{ $_ := upper .Values.bad }}")
        .into_iter()
        .next()
        .ok_or_eyre("cofactor source")?;
    let gate = Guard::Truthy {
        path: ValuesPath::parse("gate"),
    };
    source.guard = Arc::new(FrozenRequirementGuard::Predicate(Predicate::And(vec![
        Predicate::from(gate.clone()),
        Predicate::Or(
            (0..256)
                .map(|index| Predicate::truthy_path(format!("row{index}")))
                .collect(),
        ),
    ])));
    let dead = source.cofactor(&gate, false).ok_or_eyre("dead cofactor")?;
    assert!(matches!(dead.guard.as_ref(), FrozenRequirementGuard::False));

    // The unreachable arm is deliberately larger than this item's cofactor allowance.
    source.guard = Arc::new(FrozenRequirementGuard::Select {
        test: Arc::new(FrozenRequirementGuard::Predicate(Predicate::from(
            gate.clone(),
        ))),
        yes: Arc::new(FrozenRequirementGuard::True),
        no: Arc::new(FrozenRequirementGuard::Predicate(Predicate::And(
            (0..65_536)
                .map(|index| Predicate::truthy_path(format!("dead{index}")))
                .collect(),
        ))),
    });
    let live = source
        .cofactor(&gate, true)
        .ok_or_eyre("live cofactor spent dead-arm work")?;
    assert!(matches!(live.guard.as_ref(), FrozenRequirementGuard::True));
    Ok(())
}

/// Cofactors substitute the BDD's complementary literals and preserve member-import constants.
#[test]
fn cofactors_respect_literal_polarity_and_member_import_semantics() -> eyre::Result<()> {
    use std::sync::Arc;
    let mut source = observations("{{ $_ := upper .Values.bad }}")
        .into_iter()
        .next()
        .ok_or_eyre("cofactor source")?;
    let path = ValuesPath::parse("gate");
    for (positive, negative) in [
        (
            Guard::Truthy { path: path.clone() },
            Guard::Not { path: path.clone() },
        ),
        (
            Guard::Eq {
                path: path.clone(),
                value: helm_schema_core::GuardValue::Int(7),
            },
            Guard::NotEq {
                path: path.clone(),
                value: helm_schema_core::GuardValue::Int(7),
            },
        ),
        (
            Guard::TypeIs {
                path: path.clone(),
                schema_type: "string".to_owned(),
            },
            Guard::NotTypeIs {
                path: path.clone(),
                schema_type: "string".to_owned(),
            },
        ),
    ] {
        source.activation.clear();
        source.guard = Arc::new(FrozenRequirementGuard::Predicate(Predicate::Guard(
            negative.clone(),
        )));
        let false_arm = source
            .cofactor(&positive, true)
            .ok_or_eyre("opposite cofactor")?;
        assert!(matches!(
            false_arm.guard.as_ref(),
            FrozenRequirementGuard::False
        ));
        let true_arm = source
            .cofactor(&positive, false)
            .ok_or_eyre("opposite cofactor")?;
        assert!(matches!(
            true_arm.guard.as_ref(),
            FrozenRequirementGuard::True
        ));
        source.guard = Arc::new(FrozenRequirementGuard::True);
        source.activation = vec![negative];
        let active = source
            .cofactor(&positive, false)
            .ok_or_eyre("activation cofactor")?;
        sim_assert_eq!(have: active.activation, want: Vec::<Guard>::new());
        assert!(matches!(
            active.guard.as_ref(),
            FrozenRequirementGuard::True
        ));
        let inactive = source
            .cofactor(&positive, true)
            .ok_or_eyre("inactive cofactor")?;
        assert!(matches!(
            inactive.guard.as_ref(),
            FrozenRequirementGuard::False
        ));
    }
    source.activation.clear();
    source.path = ValuesPath::parse("items.*.bad");
    let collection = ValuesPath::parse("items");
    let truthy = Guard::Truthy {
        path: collection.clone(),
    };
    source.guard = Arc::new(FrozenRequirementGuard::Predicate(Predicate::Guard(
        Guard::Not {
            path: collection.clone(),
        },
    )));
    let member = source
        .cofactor(&truthy, false)
        .ok_or_eyre("member cofactor")?;
    assert!(matches!(
        member.guard.as_ref(),
        FrozenRequirementGuard::False
    ));
    source.guard = Arc::new(FrozenRequirementGuard::True);
    source.activation = vec![Guard::Not { path: collection }];
    let active = source
        .cofactor(&truthy, false)
        .ok_or_eyre("member activation")?;
    sim_assert_eq!(have: active.activation, want: Vec::<Guard>::new());
    assert!(matches!(
        active.guard.as_ref(),
        FrozenRequirementGuard::True
    ));
    Ok(())
}

/// (6a1 rework item 1) A `tpl` validity assumption joined under an unresolved choice keeps the
/// later consumption: Helm aborts at `upper` on a non-string `after` whether or not the program
/// ran, so the demand holds wherever the program writes nothing.
#[test]
fn an_assumption_joined_under_a_choice_keeps_later_consumptions() -> eyre::Result<()> {
    let consumptions = observations(indoc::indoc! {r#"
        {{- if eq (randAlpha 1) "a" }}{{ $_ := tpl .Values.program $ }}{{ end }}
        {{- $_ := upper .Values.after }}
    "#});
    let after = ValuesPath::parse("after");
    let consumption = consumptions
        .iter()
        .find(|consumption| {
            consumption.path == after
                && matches!(consumption.domain, ConsumptionDomain::Known { .. })
        })
        .ok_or_else(|| eyre::eyre!("no consumption of after: {consumptions:#?}"))?;
    assert!(!consumption.is_raw_impossible(), "{consumption:#?}");
    Ok(())
}
