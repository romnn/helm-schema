use super::*;
use crate::{
    SchemaProfile,
    emission_plan::LoweredEmissionPlan,
    emission_policy::{EmissionClassKind, EmissionPolicy},
    generate_values_schema_with_report,
};
use color_eyre::eyre;
use std::sync::atomic::{AtomicUsize, Ordering};
use test_util::prelude::sim_assert_eq;

fn requirement_implication_schema(
    guard_rows: Vec<Vec<helm_schema_core::ConditionalGuard>>,
    typed_base: bool,
) -> Value {
    use helm_schema_core::{
        ContractPathSchemaEvidence, ContractRequirementImplication, ContractRequirementTarget,
        FailValueRequirement, ValuesPath,
    };
    let evidence = ContractPathSchemaEvidence {
        is_referenced_value_path: true,
        type_hints: if typed_base {
            BTreeSet::from(["string".to_owned()])
        } else {
            BTreeSet::new()
        },
        requirement_implications: guard_rows
            .into_iter()
            .map(|guards| ContractRequirementImplication {
                outer_guards: guards.into(),
                target: ContractRequirementTarget::Value,
                requirements: vec![FailValueRequirement::SchemaType("string".to_owned())],
            })
            .collect(),
        ..ContractPathSchemaEvidence::default()
    };
    let signals = ContractSchemaSignals::new(
        BTreeMap::from([(ValuesPath::parse("target"), evidence)]),
        Vec::new(),
    );
    generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider).with_authoring_policy(
            crate::AuthoringPolicy {
                root: crate::RootPolicy::Open,
                ..Default::default()
            },
        ),
    )
}

#[test]
fn long_requirement_disjunction_is_shallow_on_a_two_mib_stack() -> eyre::Result<()> {
    use helm_schema_core::{ConditionalGuard, RequirementConditionBuilder, ValuesPath};
    use serde_json::json;

    const ALTERNATIVES: i64 = 2_049;
    const MAX_CONDITION_DEPTH: usize = 16;

    let mut graph = RequirementConditionBuilder::default();
    let mut root = RequirementConditionBuilder::FALSE;
    for value in 0..ALTERNATIVES {
        let leaf = graph.guard(ConditionalGuard::Eq {
            path: ValuesPath::parse("choice"),
            value: GuardValue::Int(value),
        });
        root = graph.or([root, leaf]);
    }
    let schema = requirement_implication_schema(
        vec![vec![ConditionalGuard::Requirement(
            graph.freeze().root(root, true),
        )]],
        false,
    );
    let mut depth = 0;
    let mut pending = vec![(&schema, 0)];
    while let Some((node, level)) = pending.pop() {
        depth = depth.max(level);
        if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
            let target = schema
                .pointer(reference.trim_start_matches('#'))
                .ok_or_else(|| eyre::eyre!("missing target {reference}"))?;
            pending.push((target, level + 1));
        }
        match node {
            Value::Object(fields) => pending.extend(
                fields
                    .iter()
                    .filter(|(key, _)| *key != "definitions")
                    .map(|(_, child)| (child, level + 1)),
            ),
            Value::Array(items) => pending.extend(items.iter().map(|item| (item, level + 1))),
            _ => {}
        }
    }
    assert!(
        depth <= MAX_CONDITION_DEPTH,
        "condition depth {depth} exceeds {MAX_CONDITION_DEPTH}"
    );
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || -> eyre::Result<()> {
            let validator = jsonschema::validator_for(&schema)?;
            for (values, accepts) in [
                (json!({"choice": 0, "target": false}), false),
                (json!({"choice": ALTERNATIVES - 1, "target": false}), false),
                (json!({"choice": ALTERNATIVES, "target": false}), true),
                (json!({"choice": 0, "target": "valid"}), true),
                (json!({"target": false}), true),
            ] {
                sim_assert_eq!(have: validator.is_valid(&values), want: accepts);
            }
            Ok(())
        })?
        .join()
        .map_err(|_| eyre::eyre!("condition validator worker panicked"))?
}

#[test]
fn requirement_junctions_flatten_without_copying_shared_nodes() -> eyre::Result<()> {
    use color_eyre::eyre::OptionExt as _;
    use helm_schema_core::{ConditionalGuard, RequirementConditionBuilder, ValuesPath};
    use serde_json::json;

    let flag = |name| ConditionalGuard::Eq {
        path: ValuesPath::parse(name),
        value: GuardValue::Bool(true),
    };
    let mut graph = RequirementConditionBuilder::default();
    let [
        shared_first,
        shared_second,
        extra_first,
        extra_second,
        required_first,
        required_second,
        required_third,
    ] = ["a", "b", "c", "d", "e", "f", "g"].map(|name| graph.guard(flag(name)));
    let shared = graph.or([shared_first, shared_second]);
    let prefix = graph.or([shared, extra_first]);
    let left = graph.or([prefix, extra_second]);
    let pair = graph.and([required_first, required_second]);
    let conjunction = graph.and([pair, required_third]);
    let right = graph.or([shared, conjunction]);
    let root = graph.decision(flag("choose"), left, right);
    let schema = requirement_implication_schema(
        vec![vec![ConditionalGuard::Requirement(
            graph.freeze().root(root, true),
        )]],
        false,
    );
    let validator = jsonschema::validator_for(&schema)?;
    for flags in 0_u16..256 {
        let [
            shared_first,
            shared_second,
            extra_first,
            extra_second,
            required_first,
            required_second,
            required_third,
            choose,
        ] = std::array::from_fn(|index| flags & (1 << index) != 0);
        let active = if choose {
            shared_first || shared_second || extra_first || extra_second
        } else {
            shared_first || shared_second || (required_first && required_second && required_third)
        };
        let mut values = json!({
            "a": shared_first, "b": shared_second, "c": extra_first, "d": extra_second,
            "e": required_first, "f": required_second, "g": required_third, "choose": choose,
        });
        for (target, valid) in [(json!(false), false), (json!("valid"), true)] {
            values["target"] = target;
            sim_assert_eq!(have: validator.is_valid(&values), want: !active || valid);
        }
    }

    let definitions = schema["definitions"]
        .as_object()
        .ok_or_eyre("missing condition definitions")?;
    let decision = definitions
        .values()
        .find(|definition| definition.get("else").is_some())
        .ok_or_eyre("missing condition decision")?;
    let resolve = |reference: &Value| {
        let pointer = reference["$ref"].as_str()?.strip_prefix('#')?;
        schema.pointer(pointer)
    };
    let left = resolve(&decision["then"]).ok_or_eyre("missing left branch")?;
    let right = resolve(&decision["else"]).ok_or_eyre("missing right branch")?;
    let left = left["anyOf"]
        .as_array()
        .ok_or_eyre("left branch is not a disjunction")?;
    let right = right["anyOf"]
        .as_array()
        .ok_or_eyre("right branch is not a disjunction")?;
    sim_assert_eq!(have: left.len(), want: 3);
    sim_assert_eq!(have: right.len(), want: 2);
    let shared = right
        .iter()
        .find(|reference| resolve(reference).is_some_and(|body| body.get("anyOf").is_some()))
        .ok_or_eyre("right branch does not reference the shared disjunction")?;
    assert!(left.contains(shared));
    let conjunction = right
        .iter()
        .filter_map(resolve)
        .find_map(|body| body["allOf"].as_array())
        .ok_or_eyre("missing right branch conjunction")?;
    sim_assert_eq!(have: conjunction.len(), want: 3);
    Ok(())
}

#[test]
fn requirement_support_abstention_keeps_a_supported_sibling_arm() -> eyre::Result<()> {
    use helm_schema_core::{ConditionalGuard, RequirementConditionBuilder, ValuesPath};
    use serde_json::json;

    let supported = ConditionalGuard::Eq {
        path: ValuesPath::parse("enabled"),
        value: GuardValue::Bool(true),
    };
    let mut graph = RequirementConditionBuilder::default();
    let known = graph.guard(supported.clone());
    let unsupported = graph.guard(ConditionalGuard::Truthy {
        path: ValuesPath::parse("unresolved"),
    });
    let root = graph.union([known, unsupported]);
    let schema = requirement_implication_schema(
        vec![vec![ConditionalGuard::Requirement(
            graph.freeze().root(root, true),
        )]],
        false,
    );
    let expected = requirement_implication_schema(vec![vec![supported]], false);
    sim_assert_eq!(have: &schema, want: &expected);
    let validator = jsonschema::validator_for(&schema)?;
    sim_assert_eq!(have: validator.is_valid(&json!({"enabled":true,"target":false})), want: false);
    sim_assert_eq!(have: validator.is_valid(&json!({"unresolved":true,"target":false})), want: true);
    Ok(())
}

#[test]
fn requirement_guarded_owner_keeps_a_dormant_target_open() -> eyre::Result<()> {
    use helm_schema_core::{ConditionalGuard, RequirementConditionBuilder, ValuesPath};
    use serde_json::json;

    let self_truthy = ConditionalGuard::Truthy {
        path: ValuesPath::parse("target"),
    };
    let enabled = ConditionalGuard::Eq {
        path: ValuesPath::parse("enabled"),
        value: GuardValue::Bool(true),
    };
    let mut graph = RequirementConditionBuilder::default();
    let local = graph.guard(self_truthy.clone());
    let external = graph.guard(enabled.clone());
    let root = graph.union([local, external]);
    let schema = requirement_implication_schema(
        vec![vec![ConditionalGuard::Requirement(
            graph.freeze().root(root, true),
        )]],
        true,
    );
    let mut expected = requirement_implication_schema(vec![vec![self_truthy], vec![enabled]], true);
    expected["allOf"][0]["if"] = json!({"$ref":"#/definitions/helm-condition-0"});
    expected["definitions"]["helm-condition-0"] = json!({"anyOf":[
        {"type":"object","properties":{"target":{"$ref":"#/definitions/t"}},"required":["target"]},
        {"type":"object","properties":{"enabled":{"enum":[true]}},"required":["enabled"]},
    ]});
    sim_assert_eq!(have: &schema, want: &expected);
    let validator = jsonschema::validator_for(&schema)?;
    sim_assert_eq!(have: validator.is_valid(&json!({"enabled":false,"target":false})), want: true);
    sim_assert_eq!(have: validator.is_valid(&json!({"enabled":true,"target":false})), want: false);
    Ok(())
}

#[test]
fn requirement_split_keeps_guarded_runtime_defaults_with_equal_successors() -> eyre::Result<()> {
    use color_eyre::eyre::OptionExt as _;
    use helm_schema_core::{
        ConditionalGuard, ContractSchemaSignals, Guard, GuardedValuesDefaultSource, Predicate,
        RequirementAtom, RequirementBdd, RequirementConditionBuilder, ValuesDefaultSource,
        ValuesPath,
    };
    use serde_json::json;

    let enabled = ConditionalGuard::Truthy {
        path: ValuesPath::parse("enabled"),
    };
    let atoms = [RequirementAtom::Value(Guard::Truthy {
        path: ValuesPath::parse("enabled"),
    })];
    let predicate = Predicate::truthy_path("enabled");
    let mut work = RequirementBdd::WORK_BUDGET;
    let mut test = RequirementBdd::new(&atoms, &mut work).ok_or_eyre("split test inventory")?;
    let test_root = test
        .predicate(&predicate)
        .ok_or_eyre("split test predicate")?;
    let test = test.finish(test_root).ok_or_eyre("exact split test")?;
    let mut graph = RequirementConditionBuilder::default();
    let split = graph.split(
        test,
        RequirementConditionBuilder::TRUE,
        RequirementConditionBuilder::TRUE,
    );
    let signals = ContractSchemaSignals::new(
        BTreeMap::new(),
        vec![vec![
            ConditionalGuard::Requirement(graph.freeze().root(split, true)),
            ConditionalGuard::Absent {
                path: ValuesPath::parse("cfg"),
            },
        ]],
    )
    .with_guarded_values_default_sources([GuardedValuesDefaultSource {
        outer_guards: vec![enabled],
        source: ValuesDefaultSource {
            target_path: ValuesPath::default(),
            source_path: ValuesPath::parse("defaults"),
        },
    }]);
    let documents = crate::PreparedValuesDocuments::new(
        serde_yaml::to_value(json!({"enabled":true,"defaults":{"cfg":"ok"}}))?,
        serde_yaml::Value::Null,
    );
    let authoring = crate::AuthoringPolicy {
        root: crate::RootPolicy::Open,
        ..Default::default()
    };
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_documents(&documents)
            .with_authoring_policy(authoring),
    );
    let no_requirements = ContractSchemaSignals::default();
    let mut expected = generate_values_schema(
        ValuesSchemaInput::new(&no_requirements, &NoopProvider)
            .with_values_documents(&documents)
            .with_authoring_policy(authoring),
    );
    let enabled = json!({"type":"object","properties":{"enabled":{"$ref":"#/definitions/t"}},"required":["enabled"]});
    let test = json!({"$ref":"#/definitions/helm-condition-0"});
    let high = json!({"$ref":"#/definitions/helm-condition-1"});
    let low = json!({"$ref":"#/definitions/helm-condition-2"});
    expected["definitions"] = json!({
        "t": crate::condition_encoding::helm_truthy_definition_schema(),
        "helm-condition-0": {"if":enabled,"then":true,"else":false},
        "helm-condition-1": {"if":test,"then":true,"else":false},
        "helm-condition-2": {"if":test,"then":false,"else":true},
    });
    let cfg_supplied =
        json!({"type":"object","properties":{"cfg":{"not":{"enum":[null]}}},"required":["cfg"]});
    let cfg_absent = json!({"not":{"type":"object","properties":{"cfg":{}},"required":["cfg"]}});
    let cfg_null = json!({"type":"object","properties":{"cfg":{"enum":[null]}},"required":["cfg"]});
    expected["allOf"] = json!([
        {"if":{"allOf":[high,{"not":cfg_supplied},{"not":{
            "type":"object","properties":{"defaults":cfg_supplied},"required":["defaults"]
        }}]},"then":false},
        {"if":{"anyOf":[
            {"allOf":[cfg_null,high]},
            {"allOf":[{"anyOf":[cfg_absent,cfg_null]},low]},
        ]},"then":false},
    ]);
    sim_assert_eq!(have: &schema, want: &expected);
    let validator = jsonschema::validator_for(&schema)?;
    for (values, accepts) in [
        (json!({"enabled":true,"defaults":{"cfg":"ok"}}), true),
        (json!({"enabled":false,"defaults":{"cfg":"ok"}}), false),
        (json!({"enabled":true,"defaults":{}}), false),
        (
            json!({"enabled":false,"defaults":{},"cfg":"explicit"}),
            true,
        ),
    ] {
        sim_assert_eq!(have: validator.is_valid(&values), want: accepts);
    }
    Ok(())
}

#[test]
fn requirement_alternative_rejected_only_by_lint_keeps_its_own_disposition() -> eyre::Result<()> {
    use helm_schema_core::{
        ConditionalGuard, ContractSchemaSignals, RequirementConditionBuilder, ValuesPath,
    };
    use serde_json::json;

    let mut graph = RequirementConditionBuilder::default();
    let split = graph.split(
        ConditionalGuard::Truthy {
            path: ValuesPath::parse("flag"),
        },
        RequirementConditionBuilder::TRUE,
        RequirementConditionBuilder::TRUE,
    );
    let signals = ContractSchemaSignals::new(
        BTreeMap::new(),
        vec![vec![
            ConditionalGuard::Truthy {
                path: ValuesPath::parse("check"),
            },
            ConditionalGuard::Requirement(graph.freeze().root(split, true)),
        ]],
    );
    let coalesced = json!({"flag":false,"check":true});
    let raw = json!({"flag":true,"check":true});
    let documents = crate::PreparedValuesDocuments::new(
        serde_yaml::to_value(&coalesced)?,
        serde_yaml::Value::Null,
    )
    .with_uncoalesced_root(serde_yaml::to_value(&raw)?);
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider).with_values_documents(&documents),
    );
    let no_requirements = ContractSchemaSignals::default();
    let mut expected = generate_values_schema(
        ValuesSchemaInput::new(&no_requirements, &NoopProvider).with_values_documents(&documents),
    );
    expected["definitions"] = json!({
        "t": crate::condition_encoding::helm_truthy_definition_schema(),
        "helm-condition-2": {
            "if":{"type":"object","properties":{"flag":{"$ref":"#/definitions/t"}},"required":["flag"]},
            "then":false,
            "else":true,
        },
    });
    expected["allOf"] = json!([{"if":{"allOf":[
        {"type":"object","properties":{"check":{"$ref":"#/definitions/t"}},"required":["check"]},
        {"$ref":"#/definitions/helm-condition-2"},
    ]},"then":false}]);
    sim_assert_eq!(have: &schema, want: &expected);
    let validator = jsonschema::validator_for(&schema)?;
    sim_assert_eq!(have: validator.is_valid(&raw), want: true);
    sim_assert_eq!(have: validator.is_valid(&coalesced), want: false);
    sim_assert_eq!(have: validator.is_valid(&json!({"flag":false,"check":false})), want: true);
    Ok(())
}

/// Projection copies share immutable guard storage.
#[test]
fn projection_clones_share_immutable_guard_scope_storage() {
    use crate::emission_policy::GuardScopes;
    let scope = GuardScopes::new(
        (0..128)
            .map(|index| helm_schema_core::ConditionalGuard::Truthy {
                path: helm_schema_core::ValuesPath::parse(&format!("guard{index}")),
            })
            .collect::<Vec<_>>(),
        Vec::new(),
    );
    let copy = scope.clone();
    sim_assert_eq!(have: &copy, want: &scope);
    sim_assert_eq!(have: scope.outer.len(), want: 128);
    sim_assert_eq!(have: scope.outer.first().map(std::ptr::from_ref), want: copy.outer.first().map(std::ptr::from_ref));
}

#[test]
fn canonical_signal_scopes_share_their_rows_and_normalization_stays_local() {
    use crate::emission_policy::GuardScopes;
    use helm_schema_core::{ConditionalGuard, GuardConjunction, ValuesPath};
    let first = ConditionalGuard::Truthy {
        path: ValuesPath::parse("a"),
    };
    let second = ConditionalGuard::Truthy {
        path: ValuesPath::parse("b"),
    };
    let canonical = vec![first.clone(), second.clone()];
    let canonical_row = GuardConjunction::from(canonical.clone()).canonicalized();
    let shared = GuardScopes::new(canonical_row.clone(), Vec::new());
    sim_assert_eq!(have: shared.outer.first().map(std::ptr::from_ref), want: canonical_row.first().map(std::ptr::from_ref));
    let source = GuardConjunction::from(vec![second.clone(), first.clone(), second.clone()]);
    let normalized = GuardScopes::new(source.clone(), Vec::new());
    sim_assert_eq!(have: normalized.outer.to_vec(), want: canonical);
    sim_assert_eq!(have: source.to_vec(), want: vec![second.clone(), first, second]);
    sim_assert_eq!(have: normalized, want: shared);
}

#[test]
fn lean_profile_keeps_local_conditionals_and_omits_document_level_conditionals() {
    let source = indoc! {r#"
        {{- if .Values.enabled }}
        data:
          message: {{ .Values.message | quote }}
        {{- end }}
        {{- if .Values.forbidden }}
        {{- fail "forbidden" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
    "#};
    let values_yaml = indoc! {"
        enabled: false
        forbidden: false
        message: hello
    "};
    let signals = schema_signals_for(parse_ir(source));

    let default_schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_documents(&prepared_values_documents(Some(values_yaml))),
    );
    let full_schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_documents(&prepared_values_documents(Some(values_yaml)))
            .with_profile(SchemaProfile::Full),
    );
    let lean_schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_documents(&prepared_values_documents(Some(values_yaml)))
            .with_profile(SchemaProfile::Lean),
    );

    sim_assert_eq!(have: default_schema, want: full_schema);
    sim_assert_eq!(
        have: lean_schema,
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "global": {},
                "enabled": {},
                "forbidden": {},
                "message": {},
            },
            "type": "object",
        })
    );

    for instance in [
        serde_json::json!({}),
        serde_json::json!({
            "enabled": false,
            "forbidden": false,
            "message": "hello",
        }),
        serde_json::json!({
            "enabled": true,
            "forbidden": false,
            "message": "hello",
        }),
    ] {
        assert!(schema_accepts_instance(&full_schema, &instance));
        assert!(schema_accepts_instance(&lean_schema, &instance));
    }
    assert!(!schema_accepts_instance(
        &full_schema,
        &serde_json::json!({ "forbidden": true })
    ));
    assert!(schema_accepts_instance(
        &lean_schema,
        &serde_json::json!({ "forbidden": true })
    ));
}

#[test]
fn emission_report_conserves_facts_and_keeps_mandatory_facts() {
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          member: {{ .Values.host.member }}
        {{- if .Values.enabled }}
          guarded: {{ .Values.guarded }}
        {{- end }}
        {{- if .Values.forbidden }}
        {{- fail "forbidden" }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: false
        forbidden: false
        guarded: value
        host:
          member: value
    "};
    let signals = schema_signals_for(parse_ir(source));
    let full = generate_values_schema_with_report(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_documents(&prepared_values_documents(Some(values_yaml)))
            .with_profile(SchemaProfile::Full),
    )
    .emission_report;
    let lean = generate_values_schema_with_report(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_documents(&prepared_values_documents(Some(values_yaml)))
            .with_profile(SchemaProfile::Lean),
    )
    .emission_report;

    for report in [&full, &lean] {
        sim_assert_eq!(
            have: report.facts.lowered,
            want: report.facts.selected + report.facts.dropped
        );
    }
    let full_mandatory =
        full.counts_for_class(crate::emission_policy::EmissionClassKind::Mandatory);
    assert!(full_mandatory.lowered > 0);
    sim_assert_eq!(have: full_mandatory.dropped, want: 0);
    sim_assert_eq!(
        have: full_mandatory.selected,
        want: full.mandatory_outcomes.total()
    );
    let lean_mandatory =
        lean.counts_for_class(crate::emission_policy::EmissionClassKind::Mandatory);
    sim_assert_eq!(have: lean_mandatory.dropped, want: 0);
    sim_assert_eq!(
        have: lean_mandatory.selected,
        want: lean.mandatory_outcomes.total()
    );
    assert!(lean_mandatory.selected > 0);
    assert!(full.facts.selected > lean.facts.selected);
    for report in [&full, &lean] {
        sim_assert_eq!(
            have: report.insertion_abstentions,
            want: crate::InsertionAbstentionCounts::default()
        );
    }
    for class in [
        crate::emission_policy::EmissionClassKind::OrdinaryRoot,
        crate::emission_policy::EmissionClassKind::KindPartitionRoot,
        crate::emission_policy::EmissionClassKind::KindPartitionLocal,
        crate::emission_policy::EmissionClassKind::TerminalAlways,
        crate::emission_policy::EmissionClassKind::TerminalGuarded,
    ] {
        sim_assert_eq!(have: lean.counts_for_class(class).selected, want: 0);
    }
    let lean_local =
        lean.counts_for_class(crate::emission_policy::EmissionClassKind::OrdinaryLocal);
    sim_assert_eq!(have: lean_local.selected, want: lean_local.lowered);
}

#[test]
fn member_projection_reports_ambiguous_descendant_insertion() {
    use crate::overlay_lowering::member_descendant_projection;
    use crate::path_resolver::ResolvedPathSchema;

    let root = ResolvedPathSchema {
        value_path: helm_schema_core::ValuesPath::parse("items.*"),
        path_segments: vec!["items".to_string(), "*".to_string()],
        schema: serde_json::json!({}),
        independent_base_contract: None,
        structural_schema: serde_json::json!({
            "anyOf": [
                {
                    "properties": { "member": { "type": "integer" } },
                    "required": ["member"],
                    "type": "object",
                },
                {
                    "properties": { "member": { "type": "boolean" } },
                    "required": ["member"],
                    "type": "object",
                },
            ],
        }),
        values_yaml_schema: serde_json::json!({}),
        provider_schema_candidate: None,
        used_as_serialized: false,
        used_as_pathless_fragment: false,
        accepted_dependency_values_root_fragment: false,
    };
    let descendant = ResolvedPathSchema {
        value_path: helm_schema_core::ValuesPath::parse("items.*.member"),
        path_segments: vec!["items".to_string(), "*".to_string(), "member".to_string()],
        schema: serde_json::json!({ "type": "string" }),
        independent_base_contract: None,
        structural_schema: serde_json::json!({ "type": "string" }),
        values_yaml_schema: serde_json::json!({}),
        provider_schema_candidate: None,
        used_as_serialized: false,
        used_as_pathless_fragment: false,
        accepted_dependency_values_root_fragment: false,
    };
    let mut abstentions = 0;

    let projected = member_descendant_projection(
        &["items".to_string()],
        &[&root, &descendant],
        &mut abstentions,
    );

    sim_assert_eq!(have: abstentions, want: 1);
    sim_assert_eq!(have: projected, want: Some(root.structural_schema));
}

#[test]
fn kind_partition_policy_requires_at_least_one_anchor_lane() {
    use crate::emission_policy::{ConditionalAnchors, EmissionPolicy};

    assert!(EmissionPolicy::new(ConditionalAnchors::Local, true, true).is_ok());
    assert!(EmissionPolicy::new(ConditionalAnchors::Root, true, true).is_ok());
    assert!(EmissionPolicy::new(ConditionalAnchors::None, true, true).is_err());
    assert!(EmissionPolicy::new(ConditionalAnchors::None, true, false).is_ok());
}

#[test]
fn kind_partition_audit_retains_local_anchors() {
    let source = indoc! {r#"
        apiVersion: apps/v1
        kind: {{ .Values.workload.kind }}
        metadata:
          name: test
        spec:
          {{- if eq .Values.workload.kind "Deployment" }}
          strategy: {{- toYaml .Values.workload.strategy | nindent 4 }}
          {{- else if eq .Values.workload.kind "StatefulSet" }}
          updateStrategy: {{- toYaml .Values.workload.strategy | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        workload:
          kind: Deployment
          strategy:
            type: RollingUpdate
    "};
    let signals = schema_signals_for(parse_ir(source));
    let report = generate_values_schema_with_report(
        ValuesSchemaInput::new(&signals, &provider())
            .with_values_documents(&prepared_values_documents(Some(values_yaml))),
    )
    .emission_report;
    let local_partitions =
        report.counts_for_class(crate::emission_policy::EmissionClassKind::KindPartitionLocal);

    assert!(local_partitions.lowered > 0);
    sim_assert_eq!(have: local_partitions.selected, want: local_partitions.lowered);
}

#[test]
fn completion_passes_preserve_profile_monotonicity() -> eyre::Result<()> {
    let helpers = indoc! {r#"
        {{- define "test.tplValues" -}}
        {{- $doc := .doc -}}
        {{- if and (eq (kindOf $doc) "map") (eq (len $doc) 1) (hasKey $doc "$tplYaml") -}}
        {{- $tpl := get $doc "$tplYaml" -}}
        {{- toJson (dict "doc" (fromYaml (tpl $tpl .ctx))) -}}
        {{- else -}}
        {{- toJson (dict "doc" $doc) -}}
        {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        {{- $values := get (include "test.tplValues" (dict "doc" .Values "ctx" $) | fromJson) "doc" }}
        {{- $_ := set . "Values" $values }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- if eq .Values.mode "on" }}
        data:
          result: {{ add .Values.payload 1 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        mode: off
        payload: 1
    "};
    let signals = schema_signals_for(parse_ir_with_helpers(source, helpers));
    let documents = prepared_values_documents(Some(values_yaml));
    let input = ValuesSchemaInput::new(&signals, &NoopProvider).with_values_documents(&documents);
    let plan = LoweredEmissionPlan::build(&input);
    let full_policy = EmissionPolicy::for_profile(SchemaProfile::Full);
    let modes = [
        serde_json::json!(null),
        serde_json::json!("off"),
        serde_json::json!("on"),
    ];
    let payloads = [
        serde_json::json!(null),
        serde_json::json!(0),
        serde_json::json!("1"),
        serde_json::json!("wrong"),
        serde_json::json!({}),
        serde_json::json!({ "$tplYaml": "2" }),
    ];

    for ((pass, full), (lean_pass, lean)) in
        completion_schemas(&plan, full_policy)
            .into_iter()
            .zip(completion_schemas(
                &plan,
                EmissionPolicy::for_profile(SchemaProfile::Lean),
            ))
    {
        sim_assert_eq!(have: lean_pass, want: pass);
        let full = jsonschema::validator_for(&full)
            .map_err(|error| eyre::eyre!("compile full schema after {pass}: {error}"))?;
        let lean = jsonschema::validator_for(&lean)
            .map_err(|error| eyre::eyre!("compile lean schema after {pass}: {error}"))?;
        for mode in &modes {
            for payload in &payloads {
                let instance = serde_json::json!({ "mode": mode, "payload": payload });
                eyre::ensure!(
                    !full.is_valid(&instance) || lean.is_valid(&instance),
                    "{pass} narrowed the widened profile for {instance}"
                );
            }
        }
    }

    Ok(())
}

fn completion_schemas(
    plan: &LoweredEmissionPlan,
    policy: EmissionPolicy,
) -> Vec<(&'static str, serde_json::Value)> {
    let projected = plan.project(policy);
    let mut schemas = vec![("projected", finish_projected(projected.clone()))];
    let projected = plan.merge_missing_defaults(projected);
    schemas.push((
        "values-default-backfill",
        finish_projected(projected.clone()),
    ));
    let projected = plan.reserve_helm_global_namespaces(projected);
    schemas.push(("reserve-global", finish_projected(projected.clone())));
    let materialized = plan.preserve_declared_defaults(projected);
    schemas.push((
        "declared-defaults",
        finish_materialized(materialized.clone()),
    ));
    let materialized = LoweredEmissionPlan::extract_repeated_payloads(materialized);
    schemas.push((
        "repeated-provider-payloads",
        finish_materialized(materialized.clone()),
    ));
    let materialized = LoweredEmissionPlan::insert_shared_definitions(materialized);
    schemas.push((
        "shared-definitions",
        finish_materialized(materialized.clone()),
    ));
    let materialized = LoweredEmissionPlan::insert_condition_definitions(materialized);
    schemas.push((
        "condition-definitions",
        finish_materialized(materialized.clone()),
    ));
    let materialized = plan.apply_values_descriptions(materialized);
    schemas.push(("descriptions", finish_materialized(materialized)));
    schemas
}

fn finish_projected(projected: crate::emission_plan::ProjectedTree) -> serde_json::Value {
    crate::emission_plan::finish_generated(
        projected.document.into_value(),
        projected.emission_report,
        std::collections::BTreeMap::new(),
        crate::RootPolicy::Closed,
    )
    .schema
}

fn finish_materialized(materialized: crate::emission_plan::MaterializedTree) -> serde_json::Value {
    crate::emission_plan::finish_generated(
        materialized.schema,
        materialized.emission_report,
        std::collections::BTreeMap::new(),
        crate::RootPolicy::Closed,
    )
    .schema
}

#[test]
fn one_plan_projections_obey_floors_and_ignore_projection_order() {
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          member: {{ .Values.host.member }}
        {{- if .Values.enabled }}
          guarded: {{ .Values.guarded }}
        {{- end }}
        {{- if .Values.forbidden }}
        {{- fail "forbidden" }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: false
        forbidden: false
        guarded: value
        host:
          member: value
    "};
    let signals = schema_signals_for(parse_ir(source));
    let documents = prepared_values_documents(Some(values_yaml));
    let input = ValuesSchemaInput::new(&signals, &NoopProvider).with_values_documents(&documents);
    let plan = LoweredEmissionPlan::build(&input);
    let full_policy = EmissionPolicy::for_profile(SchemaProfile::Full);
    let lean_policy = EmissionPolicy::for_profile(SchemaProfile::Lean);

    let full_first = plan.complete(plan.project(full_policy));
    let lean_second = plan.complete(plan.project(lean_policy));
    let lean_first = plan.complete(plan.project(lean_policy));
    let full_second = plan.complete(plan.project(full_policy));

    sim_assert_eq!(have: full_first.schema, want: full_second.schema);
    sim_assert_eq!(
        have: full_first.emission_report,
        want: full_second.emission_report
    );
    sim_assert_eq!(have: lean_first.schema, want: lean_second.schema);
    sim_assert_eq!(
        have: lean_first.emission_report,
        want: lean_second.emission_report
    );

    for report in [&full_first.emission_report, &lean_first.emission_report] {
        sim_assert_eq!(
            have: report.facts.lowered,
            want: report.facts.selected + report.facts.dropped
        );
        sim_assert_eq!(
            have: report.counts_for_class(EmissionClassKind::Mandatory).dropped,
            want: 0
        );
    }
    for class in [
        EmissionClassKind::OrdinaryRoot,
        EmissionClassKind::KindPartitionRoot,
        EmissionClassKind::KindPartitionLocal,
        EmissionClassKind::TerminalAlways,
        EmissionClassKind::TerminalGuarded,
    ] {
        sim_assert_eq!(
            have: lean_first
                .emission_report
                .counts_for_class(class)
                .selected,
            want: 0
        );
    }
}

#[test]
fn projections_never_reenter_the_provider() {
    #[derive(Debug, Default)]
    struct CountingProvider {
        calls: AtomicUsize,
    }

    impl ResourceSchemaOracle for CountingProvider {
        fn schema_fragment_for_use(
            &self,
            _use_: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    let source = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          replicas: {{ .Values.replicas }}
    "};
    let values_yaml = "replicas: 1\n";
    let signals = schema_signals_for(parse_ir(source));
    let provider = CountingProvider::default();
    let documents = prepared_values_documents(Some(values_yaml));
    let input = ValuesSchemaInput::new(&signals, &provider).with_values_documents(&documents);
    let plan = LoweredEmissionPlan::build(&input);
    let calls_after_lowering = provider.calls.load(Ordering::Relaxed);
    assert!(calls_after_lowering > 0);

    let full_policy = EmissionPolicy::for_profile(SchemaProfile::Full);
    let lean_policy = EmissionPolicy::for_profile(SchemaProfile::Lean);
    let _ = plan.complete(plan.project(full_policy));
    let _ = plan.complete(plan.project(lean_policy));

    sim_assert_eq!(
        have: provider.calls.load(Ordering::Relaxed),
        want: calls_after_lowering
    );
}
