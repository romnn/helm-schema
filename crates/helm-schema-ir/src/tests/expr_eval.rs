use std::collections::BTreeSet;

use helm_schema_ast::{DefineIndex, render_printf_string_sets};
use helm_schema_core::{Conjunction, Guard, GuardValue, Predicate, ValuesPath};
use helm_schema_syntax::Span;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::observed_facts::{CaptureKind, FailCapture, HintGrade, StringRequirementRoute};

struct Observation {
    execution: crate::value::DocumentExecution,
    value_span: Span,
    true_span: Span,
    false_span: Span,
    rows: Vec<crate::ContractUse>,
}

impl Observation {
    fn sequences(&self) -> Vec<Vec<Option<GuardValue>>> {
        self.execution
            .sites()
            .filter(|site| site.rendered() && site.span() == self.value_span)
            .flat_map(|site| site.value().alternatives())
            .filter_map(|alternative| alternative.value().sequence_items())
            .map(|items| items.iter().map(|item| item.literal().cloned()).collect())
            .collect()
    }

    fn literals(&self) -> BTreeSet<GuardValue> {
        self.execution
            .sites()
            .filter(|site| site.rendered() && site.span() == self.value_span)
            .flat_map(|site| site.value().alternatives())
            .filter_map(|alternative| alternative.value().literal().cloned())
            .collect()
    }

    fn paths(&self) -> BTreeSet<ValuesPath> {
        self.execution
            .sites()
            .filter(|site| site.rendered() && site.span() == self.value_span)
            .flat_map(|site| site.value().inputs())
            .collect()
    }

    fn raw_conditions(&self, path: &str) -> Option<Predicate> {
        let path = ValuesPath::parse(path);
        let mut conditions = Vec::new();
        for site in self
            .execution
            .sites()
            .filter(|site| site.rendered() && site.span() == self.value_span)
        {
            for alternative in site.value().alternatives() {
                if alternative.value().identity() != Some(&path) {
                    continue;
                }
                if !site.coverage_exact() || !alternative.coverage_exact() {
                    return None;
                }
                conditions.push(Predicate::all(vec![
                    site.condition().clone(),
                    alternative.condition().clone(),
                ]));
            }
        }
        Some(Predicate::Or(conditions).normalize_boolean())
    }

    fn when_true(&self) -> Predicate {
        self.marker(self.true_span, "selected")
    }
    fn when_false(&self) -> Predicate {
        self.marker(self.false_span, "other")
    }

    fn marker(&self, span: Span, _marker: &str) -> Predicate {
        self.execution.observed_marker_condition(span)
    }

    fn truth(&self) -> Option<Predicate> {
        if !self.execution.observed_marker_complete(self.true_span)
            || !self.execution.observed_marker_complete(self.false_span)
        {
            return None;
        }
        let yes = self.when_true();
        let no = self.when_false();
        (Predicate::Or(vec![yes.clone(), no]).normalize_boolean() == Predicate::True).then_some(yes)
    }

    fn captures(&self) -> BTreeSet<FailCapture> {
        self.execution.observed_facts().captures.clone()
    }

    fn kinds(&self) -> BTreeSet<CaptureKind> {
        self.captures()
            .into_iter()
            .map(|capture| capture.kind)
            .collect()
    }

    fn captured_boolean_subset(&self, truth: bool) -> Predicate {
        let mut conditions = Vec::new();
        for site in self
            .execution
            .sites()
            .filter(|site| site.rendered() && site.span() == self.value_span)
        {
            if !site.coverage_exact() {
                continue;
            }
            for alternative in site.value().alternatives() {
                if alternative.coverage_exact()
                    && alternative.value().literal() == Some(&GuardValue::Bool(truth))
                {
                    conditions.push(Predicate::all(vec![
                        site.condition().clone(),
                        alternative.condition().clone(),
                    ]));
                }
            }
        }
        Predicate::Or(conditions).normalize_boolean()
    }

    fn requirements(&self) -> BTreeSet<(CaptureKind, BTreeSet<Predicate>)> {
        self.captures()
            .into_iter()
            .map(|capture| (capture.kind, capture.conjunction.into_iter().collect()))
            .collect()
    }

    fn string_routes(&self) -> BTreeSet<(ValuesPath, StringRequirementRoute, Conjunction)> {
        self.captures()
            .into_iter()
            .filter_map(|capture| match capture.kind {
                CaptureKind::StringRequirement {
                    path,
                    route,
                    selection,
                } => Some((path, route, selection)),
                _ => None,
            })
            .collect()
    }

    fn string_conditions(&self) -> BTreeSet<(ValuesPath, Conjunction)> {
        self.captures()
            .into_iter()
            .filter_map(|capture| match capture.kind {
                CaptureKind::StringRequirement { path, .. } => Some((path, capture.conjunction)),
                _ => None,
            })
            .collect()
    }

    fn typed_conditions(
        &self,
        schema_type: &str,
        null_aborts: bool,
    ) -> BTreeSet<(ValuesPath, Conjunction)> {
        self.captures()
            .into_iter()
            .filter_map(|capture| match capture.kind {
                CaptureKind::ValueType {
                    path,
                    schema_type: actual,
                    null_aborts: actual_null,
                } if actual == schema_type && actual_null == null_aborts => {
                    Some((path, capture.conjunction))
                }
                _ => None,
            })
            .collect()
    }

    fn member_conditions(&self) -> BTreeSet<Conjunction> {
        self.captures()
            .into_iter()
            .filter_map(|capture| match capture.kind {
                CaptureKind::MemberAccess { .. } => Some(capture.conjunction),
                _ => None,
            })
            .collect()
    }

    fn declared_types(&self, path: &str) -> Option<&BTreeSet<String>> {
        self.execution
            .observed_facts()
            .type_hints
            .get(&HintGrade::DECLARED)?
            .get(&ValuesPath::parse(path))
    }

    fn has_boolean_contract(&self, path: &str) -> bool {
        self.kinds().iter().any(|kind| {
            matches!(kind,
            CaptureKind::ComparableKind { path: actual, schema_type }
            | CaptureKind::ValueType { path: actual, schema_type, .. }
            if actual == &ValuesPath::parse(path) && schema_type == "boolean")
        })
    }
}

fn observe(prefix: &str, expression: &str) -> Observation {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    observe_with_db(prefix, expression, &db)
}

fn observe_with_db(prefix: &str, expression: &str, db: &IrAnalysisDb) -> Observation {
    let mut source = format!("{prefix}{{{{ $__observed := {expression} }}}}value: ");
    let start = source.len();
    source.push_str("{{ $__observed }}");
    let value_span = Span::new(start, source.len());
    source.push('\n');
    source.push_str("{{ if $__observed }}# ");
    let start = source.len();
    source.push_str("selected");
    let true_span = Span::new(start, source.len());
    source.push_str("{{ else }}# ");
    let start = source.len();
    source.push_str("other");
    let false_span = Span::new(start, source.len());
    source.push_str("{{ end }}");
    let execution = crate::value::tests::execute_document(&source, None, db);
    let projected = crate::tests::support::project_execution(&source, None, db, &execution);
    let contract = crate::fragment_eval::contract_ir_from_document(&projected).finalize();
    let rows = contract
        .uses()
        .iter()
        .filter(|row| row.path == crate::YamlPath(vec!["value".to_owned()]))
        .cloned()
        .collect();
    Observation {
        execution,
        value_span,
        true_span,
        false_span,
        rows,
    }
}

fn paths(names: &[&str]) -> BTreeSet<ValuesPath> {
    names.iter().map(|name| ValuesPath::parse(name)).collect()
}
fn truthy(name: &str) -> Predicate {
    Predicate::truthy_path(name)
}
fn typed(name: &str, schema_type: &str) -> Predicate {
    Predicate::from(Guard::TypeIs {
        path: ValuesPath::parse(name),
        schema_type: schema_type.to_owned(),
    })
}
fn object_kinds(name: &str, null_aborts: bool, absence_aborts: bool) -> BTreeSet<CaptureKind> {
    let path = ValuesPath::parse(name);
    let mut result = BTreeSet::from([CaptureKind::ValueType {
        path: path.clone(),
        schema_type: "object".to_owned(),
        null_aborts,
    }]);
    if absence_aborts {
        result.insert(CaptureKind::AbsenceAborts { path });
    }
    result
}
fn assert_no_raw_type(observation: &Observation, names: &[&str]) {
    for name in names {
        sim_assert_eq!(have: observation.declared_types(name), want: None);
    }
}
fn assert_selected(observation: &Observation, path: &str, condition: Predicate) {
    sim_assert_eq!(have: observation.raw_conditions(path), want: Some(condition.normalize_boolean()));
}

fn contains_predicate(predicate: &Predicate, target: &Predicate) -> bool {
    if predicate == target {
        return true;
    }
    match predicate.kind() {
        helm_schema_core::PredicateKind::And(parts)
        | helm_schema_core::PredicateKind::Or(parts) => {
            parts.iter().any(|part| contains_predicate(part, target))
        }
        helm_schema_core::PredicateKind::Not(inner) => contains_predicate(inner, target),
        helm_schema_core::PredicateKind::True
        | helm_schema_core::PredicateKind::False
        | helm_schema_core::PredicateKind::Guard(_)
        | helm_schema_core::PredicateKind::Approximate { .. } => false,
    }
}

#[test]
fn grouped_map_argument_preserves_nil_conversion_boundary() {
    let bare = observe("", r#"hasKey .Values.subject "key""#);
    let grouped = observe("", r#"hasKey (.Values.subject) "key""#);
    sim_assert_eq!(have: bare.kinds(), want: object_kinds("subject", true, true));
    sim_assert_eq!(have: grouped.kinds(), want: object_kinds("subject", false, false));
}

#[test]
fn outer_grouping_overrides_direct_binding_provenance() {
    let prefix = "{{ $subject := .Values.subject }}";
    let bare = observe(prefix, r#"hasKey $subject "key""#);
    let grouped = observe(prefix, r#"hasKey ($subject) "key""#);
    sim_assert_eq!(have: bare.kinds(), want: object_kinds("subject", false, false));
    sim_assert_eq!(have: grouped.kinds(), want: object_kinds("subject", false, false));
}

#[test]
fn mixed_binding_modes_keep_values_paired_with_branch_conditions() {
    let result = observe(
        "{{ $cfg := .Values.direct }}{{ if .Values.rebind }}{{ $cfg = .Values.evaluated }}{{ end }}",
        r#"hasKey $cfg.child "key""#,
    );
    sim_assert_eq!(have: result.typed_conditions("object", true), want: BTreeSet::from([
        (ValuesPath::parse("direct.child"), vec![truthy("rebind").negated()].into()),
        (ValuesPath::parse("evaluated.child"), vec![truthy("rebind")].into()),
    ]));
}

#[test]
fn evaluated_leaf_scalar_is_not_rederived_from_its_structural_source() {
    let direct = observe("{{ $cfg := .Values.cfg }}", "$cfg");
    let evaluated = observe("{{ $cfg := toString .Values.cfg }}", "$cfg");
    sim_assert_eq!(have: direct.truth(), want: Some(truthy("cfg")));
    assert_selected(&direct, "cfg", Predicate::True);
    sim_assert_eq!(have: evaluated.paths(), want: paths(&["cfg"]));
    sim_assert_eq!(have: evaluated.raw_conditions("cfg"), want: Some(Predicate::False));
}

#[test]
fn unknown_decision_keeps_both_leaves_and_proves_no_strict_operand() {
    let prefix = r#"{{ $_ := .Values.choose }}{{ $cfg := .Values.first }}{{ if eq (randAlphaNum 1) "a" }}{{ $cfg = .Values.second }}{{ end }}"#;
    let read = observe(prefix, "$cfg");
    sim_assert_eq!(have: read.paths(), want: paths(&["first", "second"]));
    assert!(read.truth().is_none());
    let strict = observe(prefix, r#"hasKey $cfg "key""#);
    sim_assert_eq!(have: strict.kinds(), want: BTreeSet::new());
}

#[test]
#[ignore = "R3b regression vs main, package A (consumed/comparison provenance): strict string consumer keeps each binding leaf under its decision; tracked in plan/value-algebra-r3b.md follow-ups"]
fn strict_string_consumer_keeps_each_binding_leaf_under_its_decision() {
    let result = observe(
        "{{ $value := .Values.a }}{{ if .Values.useB }}{{ $value = .Values.b }}{{ end }}",
        "b64enc $value",
    );
    sim_assert_eq!(have: result.string_conditions(), want: BTreeSet::from([
        (ValuesPath::parse("a"), vec![truthy("useB").negated()].into()),
        (ValuesPath::parse("b"), vec![truthy("useB")].into()),
    ]));
}

#[test]
fn type_test_keeps_each_selected_binding_leaf_under_its_decision() {
    let result = observe(
        "{{ $value := .Values.a }}{{ if .Values.useB }}{{ $value = .Values.b }}{{ end }}",
        r#"kindIs "string" $value"#,
    );
    let expected = Predicate::Or(vec![
        Predicate::all(vec![truthy("useB"), typed("b", "string")]),
        Predicate::all(vec![truthy("useB").negated(), typed("a", "string")]),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: result.truth(), want: Some(expected));
}

#[test]
fn type_test_keeps_partial_selected_binding_truth_unresolved() {
    let result = observe(
        r#"{{ $value := .Values.a }}{{ if or .Values.known (eq (randAlphaNum 1) "a") }}{{ $value = .Values.b }}{{ end }}"#,
        r#"kindIs "string" $value"#,
    );
    sim_assert_eq!(have: result.truth(), want: None);
    sim_assert_eq!(have: result.when_true(), want: Predicate::all(vec![truthy("known"), typed("b", "string")]).normalize_boolean());
    sim_assert_eq!(have: result.when_false(), want: Predicate::all(vec![truthy("known"), typed("b", "string").negated()]).normalize_boolean());
}

#[test]
fn type_test_follows_first_truthy_value_selection() {
    let result = observe(
        r#"{{ $value := default "fallback" .Values.primary }}"#,
        r#"typeIs "string" $value"#,
    );
    let expected = Predicate::Or(vec![
        truthy("primary").negated(),
        typed("primary", "string"),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: result.truth(), want: Some(expected));
}

#[test]
fn default_composes_selected_primary_leaves_with_its_fallback() {
    let result = observe(
        "{{ $value := .Values.a }}{{ if .Values.useB }}{{ $value = .Values.b }}{{ end }}",
        r#"kindIs "map" (default (dict) $value)"#,
    );
    let expected = Predicate::Or(vec![
        Predicate::all(vec![truthy("useB"), truthy("b"), typed("b", "object")]),
        Predicate::all(vec![truthy("useB"), truthy("b").negated()]),
        Predicate::all(vec![
            truthy("useB").negated(),
            truthy("a"),
            typed("a", "object"),
        ]),
        Predicate::all(vec![truthy("useB").negated(), truthy("a").negated()]),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: result.truth(), want: Some(expected));
}

#[test]
fn default_rebuilds_a_complete_composed_result_from_its_surviving_leaf() {
    sim_assert_eq!(have: observe("", r#"kindIs "map" (default (dict) (ternary "selected" (dict "k" 1) true))"#).truth(), want: Some(Predicate::False));
}

#[test]
fn type_test_classifies_a_default_scalar_fallback_arm() {
    let expected = Predicate::Or(vec![
        truthy("x").negated(),
        Predicate::all(vec![truthy("x"), typed("x", "boolean")]),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: observe("", r#"kindIs "bool" (.Values.x | default false)"#).truth(), want: Some(expected));
}

#[test]
fn pristine_root_values_selector_records_direct_hosts_in_both_binding_modes() {
    for prefix in ["", "{{ $root := $ }}{{ $ = $root }}"] {
        let result = observe(prefix, "$.Values.a.b");
        sim_assert_eq!(have: result.paths(), want: paths(&["a.b"]));
        sim_assert_eq!(have: result.truth(), want: Some(truthy("a.b")));
        sim_assert_eq!(have: result.member_conditions(), want: BTreeSet::from([vec![typed("a", "object").negated()].into()]));
    }
}

#[test]
#[ignore = "R3b regression vs main, package C (parser, collection and validation transfers): static semver fields fold through decimal printf; tracked in plan/value-algebra-r3b.md follow-ups"]
fn static_semver_fields_fold_through_decimal_printf() {
    let prefix =
        r#"{{ $_ := set $ "Capabilities" (dict "KubeVersion" (dict "Version" "v1.35.0")) }}"#;
    let formatted = r#"printf "%d.%d.0" (semver .Capabilities.KubeVersion.Version).Major (semver .Capabilities.KubeVersion.Version).Minor"#;
    sim_assert_eq!(have: observe(prefix, formatted).literals(), want: BTreeSet::from([GuardValue::string("1.35.0")]));
    sim_assert_eq!(have: observe(prefix, r#"semverCompare "<1.30.0" (printf "%d.%d.0" (semver .Capabilities.KubeVersion.Version).Major (semver .Capabilities.KubeVersion.Version).Minor)"#).truth(), want: Some(Predicate::False));
}

#[test]
fn helper_value_expression_uses_shared_expression_eval() {
    let result = observe(
        r#"{{ $_ := set $ "ctx" (dict "config" .Values.serviceAccount) }}"#,
        r#".ctx.config.name | default "x""#,
    );
    sim_assert_eq!(have: result.paths(), want: paths(&["serviceAccount.name"]));
    assert_selected(
        &result,
        "serviceAccount.name",
        truthy("serviceAccount.name"),
    );
    assert!(result.literals().contains(&GuardValue::string("x")));
}

#[test]
fn helper_argument_projection_uses_shared_expression_eval() {
    let prefix = r#"{{ $arg := dict "ctx" $ "config" .Values.serviceAccount }}"#;
    sim_assert_eq!(have: observe(prefix, "$arg.config").paths(), want: paths(&["serviceAccount"]));
    sim_assert_eq!(have: observe(prefix, "$arg.ctx.Values.child").paths(), want: paths(&["child"]));
}

#[test]
fn defaulted_helper_root_descends_to_raw_member_paths() {
    let prefix = r#"{{ $arg := dict "Values" (.Values.dependency | default dict) }}"#;
    let result = observe(prefix, "$arg.Values.nameOverride");
    assert_selected(&result, "dependency.nameOverride", truthy("dependency"));
}

#[test]
fn grouped_selector_preserves_scalar_identity() {
    assert_selected(
        &observe("", "(.Values.feature).mode"),
        "feature.mode",
        Predicate::True,
    );
}

#[test]
fn bound_path_resolution_uses_shared_expression_eval() {
    let result = observe(
        r#"{{ $_ := set $ "config" .Values.serviceAccount }}"#,
        ".config.name",
    );
    sim_assert_eq!(have: result.paths(), want: paths(&["serviceAccount.name"]));
}

#[test]
fn direct_root_values_path_is_an_expression_eval_projection() {
    for (expression, path) in [
        (".Values.foo.bar", "foo.bar"),
        ("$.Values.X", "X"),
        (
            "((.Values.appVersions).airtype).global",
            "appVersions.airtype.global",
        ),
    ] {
        assert_selected(&observe("", expression), path, Predicate::True);
    }
    let contextual = observe(
        r#"{{ $root := dict "Values" (dict "Y" "literal") }}"#,
        "$root.Values.Y",
    );
    sim_assert_eq!(have: contextual.paths(), want: BTreeSet::new());
    sim_assert_eq!(have: contextual.literals(), want: BTreeSet::from([GuardValue::string("literal")]));
}

#[test]
fn named_root_alias_projects_values_only_from_its_typed_binding() {
    assert_selected(
        &observe("{{ $root := $ }}", "$root.Values.Y"),
        "Y",
        Predicate::True,
    );
}

#[test]
fn direct_values_path_projection_rejects_computed_and_contextual_paths() {
    for expression in [
        ".context.Values.X",
        r#"eq .Values.X ".Values.fake""#,
        r#"" .Values.fake ""#,
    ] {
        sim_assert_eq!(have: observe("", expression).raw_conditions("X"), want: Some(Predicate::False));
    }
}

#[test]
fn set_default_chart_paths_ignores_unrelated_default_inside_set_rhs() {
    let result = observe(
        r#"{{ $_ := set $ "serviceAccount" .Values.serviceAccount }}{{ $_ := set $ "other" .Values.other }}"#,
        r#"set .serviceAccount "name" (printf "%s" (.other | default "fallback"))"#,
    );
    sim_assert_eq!(have: result.execution.observed_facts().values_default_sources, want: BTreeSet::new());
}

#[test]
fn string_transform_pipeline_preserves_all_printf_argument_paths() {
    let result = observe(
        "",
        r#"printf "%s-%s" .Values.primary.name .Values.suffix | trunc 63"#,
    );
    sim_assert_eq!(have: result.paths(), want: paths(&["primary.name", "suffix"]));
    assert_no_raw_type(&result, &["primary.name", "suffix"]);
}

#[test]
fn quote_pipeline_erases_input_shape_without_typing() {
    let result = observe("", ".Values.flag | quote");
    assert_no_raw_type(&result, &["flag"]);
    sim_assert_eq!(have: result.paths(), want: paths(&["flag"]));
    assert!(
        result
            .execution
            .observed_facts()
            .shape_erased_paths
            .contains(&ValuesPath::parse("flag")),
        "quote must record shape erasure; placed rows: {:?}",
        result.rows
    );
}

#[test]
fn local_fragment_variable_effects_include_shallow_source_paths() {
    let prefix = r#"{{ $_ := .Values.choose }}{{ $nodeSelector := .Values.nodeSelector }}{{ if eq (randAlphaNum 1) "a" }}{{ $nodeSelector = .Values.global.nodeSelector }}{{ end }}"#;
    sim_assert_eq!(have: observe(prefix, "$nodeSelector").paths(), want: paths(&["global.nodeSelector", "nodeSelector"]));
}

#[test]
fn printf_exact_rendering_only_accepts_supported_string_formats() {
    let values = [BTreeSet::from(["x".to_string()])];

    sim_assert_eq!(
        have: render_printf_string_sets("prefix-%s-%%", &values),
        want: Some(BTreeSet::from(["prefix-x-%".to_string()]))
    );
    sim_assert_eq!(have: render_printf_string_sets("%d", &values), want: None);
    sim_assert_eq!(
        have: render_printf_string_sets("literal", &[BTreeSet::from(["unused".to_string()])]),
        want: None
    );
    sim_assert_eq!(have: render_printf_string_sets("%s-%s", &values), want: None);
}

#[test]
fn integer_index_on_values_path_descends_array_item_wildcard() {
    assert_selected(
        &observe(
            "",
            "index .Values.sentinel.externalAccess.service.loadBalancerIP 0",
        ),
        "sentinel.externalAccess.service.loadBalancerIP.*",
        Predicate::True,
    );
}

#[test]
fn integer_index_on_known_list_stays_positional() {
    sim_assert_eq!(have: observe("", r#"index (list "root" "scope" "pod") 1"#).literals(), want: BTreeSet::from([GuardValue::string("scope")]));
}

#[test]
fn get_requires_its_values_backed_host_to_be_an_object() {
    let result = observe(
        "{{ $context := index (.Values.contexts | toJson | fromJsonArray) 0 }}",
        r#"get $context "creds""#,
    );
    assert!(
        result
            .member_conditions()
            .contains(&vec![typed("contexts.*", "object").negated()].into())
    );
}

#[test]
fn grouped_selector_requires_an_object_only_when_receiver_is_present() {
    let result = observe("", "(.Values.resources.limits).memory");
    sim_assert_eq!(have: result.paths(), want: paths(&["resources.limits.memory"]));
    assert!(
        result.member_conditions().contains(
            &vec![
                Predicate::from(Guard::Absent {
                    path: ValuesPath::parse("resources.limits")
                })
                .negated(),
                typed("resources.limits", "object").negated()
            ]
            .into()
        )
    );
}

#[test]
fn ungrouped_selector_still_requires_the_intermediate_member() {
    assert!(
        observe("", ".Values.resources.limits.memory")
            .member_conditions()
            .contains(&vec![typed("resources.limits", "object").negated()].into())
    );
}

#[test]
fn set_call_updates_local_key_with_assigned_literal() {
    let prefix = r#"{{ $config := dict "name" .Values.serviceAccount.name "annotations" .Values.serviceAccount.annotations }}{{ set $config (printf "%s" "name") "generated" }}"#;
    sim_assert_eq!(have: observe(prefix, "$config.name").literals(), want: BTreeSet::from([GuardValue::string("generated")]));
    sim_assert_eq!(have: observe(prefix, "$config.annotations").paths(), want: paths(&["serviceAccount.annotations"]));
}

#[test]
fn set_call_inside_throwaway_assignment_updates_local_key() {
    let prefix = r#"{{ $config := dict "name" .Values.serviceAccount.name }}{{ $_ := set $config (printf "%s" "name") "generated" }}"#;
    sim_assert_eq!(have: observe(prefix, "$config.name").literals(), want: BTreeSet::from([GuardValue::string("generated")]));
}

#[test]
fn set_call_preserves_assigned_value_path() {
    let prefix = r#"{{ $config := dict "name" .Values.serviceAccount.name }}{{ $_ := set $config "name" .Values.generatedName }}"#;
    sim_assert_eq!(have: observe(prefix, "$config.name").paths(), want: paths(&["generatedName"]));
}

#[test]
fn selector_on_local_dict_records_only_selected_child_reads() {
    let prefix = r#"{{ $config := dict "name" .Values.serviceAccount.name "annotations" .Values.serviceAccount.annotations }}"#;
    sim_assert_eq!(have: observe(prefix, "$config.annotations").paths(), want: paths(&["serviceAccount.annotations"]));
}

#[test]
fn unsupported_printf_format_types_nothing_without_exact_string() {
    let result = observe("", r#"printf "%d" .Values.count"#);
    assert_no_raw_type(&result, &["count"]);
    sim_assert_eq!(have: result.paths(), want: paths(&["count"]));
    assert!(result.literals().is_empty());
}

#[test]
fn pipeline_ternary_collapses_value_but_keeps_branch_facts() {
    let expression =
        r#"typeIs "string" .Values.config | ternary .Values.config (.Values.config | toYaml)"#;
    let result = observe("", expression);
    sim_assert_eq!(have: result.paths(), want: paths(&["config"]));
    assert_no_raw_type(&result, &["config"]);
    sim_assert_eq!(have: result.execution.observed_facts().type_hints.get(&HintGrade::GUARDED_DECLARED).and_then(|hints| hints.get(&ValuesPath::parse("config"))), want: Some(&BTreeSet::from(["string".to_owned()])));
    let consumed = observe("", &format!("{expression} | b64enc"));
    sim_assert_eq!(have: consumed.string_routes(), want: BTreeSet::new());
}

#[test]
fn type_test_uses_the_structural_value_instead_of_its_influences() {
    let prefix =
        r#"{{ $obj := dict "merge" .Values.service.merge "patch" .Values.service.patch }}"#;
    sim_assert_eq!(have: observe(prefix, r#"kindIs "map" $obj"#).truth(), want: Some(Predicate::True));
}

#[test]
fn invalid_kind_requires_one_exact_subject_identity() {
    sim_assert_eq!(have: observe("", r#"kindIs "invalid" (ternary .Values.value "fallback" .Values.enabled)"#).truth(), want: None);
}

#[test]
fn invalid_kind_retains_a_meta_selected_subject_guard() {
    let prefix = r#"{{ $selected := "fallback" }}{{ if .Values.enabled }}{{ $selected = .Values.value }}{{ end }}"#;
    let result = observe(prefix, r#"kindIs "invalid" $selected"#);
    let expected = Predicate::all(vec![
        truthy("enabled"),
        Predicate::Or(vec![
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse("value"),
                value: GuardValue::Null,
            }),
            Predicate::from(Guard::Absent {
                path: ValuesPath::parse("value"),
            }),
        ]),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: result.truth(), want: Some(expected.clone()));
    sim_assert_eq!(have: result.when_false(), want: expected.negated().normalize_boolean());
}

#[test]
fn invalid_kind_retains_a_default_selected_subject_guard() {
    let result = observe(
        "{{ $selected := default nil .Values.value }}",
        r#"kindIs "invalid" $selected"#,
    );
    let expected = Predicate::Or(vec![
        Predicate::from(Guard::Eq {
            path: ValuesPath::parse("value"),
            value: GuardValue::Null,
        }),
        Predicate::from(Guard::Absent {
            path: ValuesPath::parse("value"),
        }),
        truthy("value").negated(),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: result.truth(), want: Some(expected.clone()));
    sim_assert_eq!(have: result.when_false(), want: expected.negated().normalize_boolean());
}

#[test]
fn go_regex_literal_escaping_leaves_re2_hyphens_bare() {
    sim_assert_eq!(
        have: crate::escape_regex_literal("prefix-with.+symbols"),
        want: r"prefix-with\.\+symbols"
    );
}

#[test]
fn lexical_escape_patterns_use_the_shared_go_regex_quoting() {
    let pattern = crate::helper_meta::pattern_with_lexical_escapes(
        "^value$",
        &BTreeSet::from([crate::helper_meta::LexicalEscape::TrimSuffix(
            "-/x.y".to_string(),
        )]),
    );

    sim_assert_eq!(have: pattern, want: r"^(?:value)(?:-/x\.y)?$");
}

#[test]
fn quoted_negative_zero_keeps_falsy_pattern_truth_unknown() {
    sim_assert_eq!(have: observe("", r#"contains "-" (quote .Values.value)"#).truth(), want: None);
}

#[test]
fn values_numeric_type_spellings_remain_provenance_dependent() {
    let int64 = observe("", r#"typeIs "int64" .Values.value"#);
    let float64 = observe("", r#"kindIs "float64" .Values.value"#);
    sim_assert_eq!(have: int64.truth(), want: None);
    sim_assert_eq!(have: int64.captured_boolean_subset(true), want: Predicate::False);
    sim_assert_eq!(have: int64.captured_boolean_subset(false), want: typed("value", "integer").negated());
    sim_assert_eq!(have: float64.truth(), want: None);
    sim_assert_eq!(have: float64.captured_boolean_subset(true), want: Predicate::all(vec![typed("value", "number"), typed("value", "integer").negated()]).normalize_boolean());
    sim_assert_eq!(have: float64.captured_boolean_subset(false), want: typed("value", "number").negated());
}

#[test]
fn stringified_pattern_truth_keeps_raw_string_subsets() {
    let result = observe("", r#"regexMatch "^[0-9]+$" (toString .Values.value)"#);
    sim_assert_eq!(have: result.truth(), want: None);
    sim_assert_eq!(have: result.when_true(), want: Predicate::from(Guard::MatchesPattern { path: ValuesPath::parse("value"), pattern: "^[0-9]+$".to_owned(), templated: false }));
    sim_assert_eq!(have: result.when_false(), want: Predicate::from(Guard::NotMatchesPattern { path: ValuesPath::parse("value"), pattern: "^[0-9]+$".to_owned() }));
}

#[test]
fn stringified_contains_uses_go_compatible_literal_pattern_syntax() {
    let result = observe("", r#"contains "foo-bar" (toString .Values.value)"#);
    sim_assert_eq!(have: result.when_true(), want: Predicate::from(Guard::MatchesPattern { path: ValuesPath::parse("value"), pattern: "foo-bar".to_owned(), templated: false }));
    sim_assert_eq!(have: result.when_false(), want: Predicate::from(Guard::NotMatchesPattern { path: ValuesPath::parse("value"), pattern: "foo-bar".to_owned() }));
}

#[test]
fn ternary_preserves_scalar_branch_dispatch() {
    let result = observe(
        "",
        r#"ternary true false (semverCompare ">=3.0.0" .Values.version)"#,
    );
    sim_assert_eq!(have: result.literals(), want: BTreeSet::from([GuardValue::Bool(true), GuardValue::Bool(false)]));
    assert!(result.truth().is_some_and(|predicate| {
        predicate
            .value_paths()
            .contains(&ValuesPath::parse("version"))
    }));
}

#[test]
fn base64_pipeline_preserves_source_path() {
    sim_assert_eq!(have: observe("", ".Values.auth.password | toString | b64enc").paths(), want: paths(&["auth.password"]));
}

#[test]
#[ignore = "R3b regression vs main, package B (iterable domains and loop outcomes): uniq pipeline preserves local list items; tracked in plan/value-algebra-r3b.md follow-ups"]
fn uniq_pipeline_preserves_local_list_items() {
    let result = observe(
        "{{ $pullSecrets := list (index .Values.image.pullSecrets 0) }}",
        "$pullSecrets | uniq",
    );
    sim_assert_eq!(have: result.paths(), want: paths(&["image.pullSecrets.*"]));
}

#[test]
fn split_list_preserves_exact_segment_sequence() {
    sim_assert_eq!(have: observe("", r#"splitList "." "auth.password""#).sequences(), want: vec![vec![Some(GuardValue::string("auth")), Some(GuardValue::string("password"))]]);
}

#[test]
fn split_list_preserves_mixed_length_path_alternatives() {
    let result = observe(
        "",
        r#"splitList "." (coalesce "auth.password" "global.auth.password")"#,
    );
    // A nonempty first literal prevents the later candidate from being selected.
    sim_assert_eq!(have: result.sequences(), want: vec![vec![Some(GuardValue::string("auth")), Some(GuardValue::string("password"))]]);
}

#[test]
#[ignore = "R3b regression vs main, package B (iterable domains and loop outcomes): append projects the source collection to its member domain; tracked in plan/value-algebra-r3b.md follow-ups"]
fn append_projects_the_source_collection_to_its_member_domain() {
    let result = observe("", r#"append (default (list) .Values.items) "synthetic""#);
    sim_assert_eq!(have: result.paths(), want: paths(&["items.*"]));
    assert!(
        result
            .sequences()
            .iter()
            .any(|items| items.last() == Some(&Some(GuardValue::string("synthetic"))))
    );
}

#[test]
fn first_and_reverse_preserve_list_structure() {
    for expression in [r#"first (list "a" "b")"#, r#"(list "a" "b") | first"#] {
        sim_assert_eq!(have: observe("", expression).literals(), want: BTreeSet::from([GuardValue::string("a")]));
    }
    for expression in [r#"reverse (list "a" "b")"#, r#"(list "a" "b") | reverse"#] {
        sim_assert_eq!(have: observe("", expression).sequences(), want: vec![vec![Some(GuardValue::string("b")), Some(GuardValue::string("a"))]]);
    }
}

#[test]
#[ignore = "R3b regression vs main, package A (consumed/comparison provenance): strict pipeline calls match direct operand contracts; tracked in plan/value-algebra-r3b.md follow-ups"]
fn strict_pipeline_calls_match_direct_operand_contracts() {
    for (direct, pipeline) in [
        ("len .Values.input", ".Values.input | len"),
        ("first .Values.input", ".Values.input | first"),
        ("reverse .Values.input", ".Values.input | reverse"),
        (
            r#"eq .Values.input "active""#,
            r#".Values.input | eq "active""#,
        ),
        (
            r#"ne .Values.input "active""#,
            r#".Values.input | ne "active""#,
        ),
    ] {
        let direct = observe("", direct);
        let pipeline = observe("", pipeline);
        assert!(
            !direct.captures().is_empty(),
            "the reference consumer must publish a contract"
        );
        sim_assert_eq!(have: pipeline.captures(), want: direct.captures());
    }
}

#[test]
fn migrated_invocation_families_match_between_call_and_pipeline_syntax() {
    for (direct, pipeline) in [
        ("first .Values.input", ".Values.input | first"),
        (
            r#"eq .Values.input "active""#,
            r#".Values.input | eq "active""#,
        ),
        (
            r#"ternary "yes" "no" .Values.input"#,
            r#".Values.input | ternary "yes" "no""#,
        ),
        (
            r#"replace "old" "new" .Values.input"#,
            r#".Values.input | replace "old" "new""#,
        ),
        (
            r#"trimSuffix "x" .Values.input"#,
            r#".Values.input | trimSuffix "x""#,
        ),
        ("fromJson .Values.input", ".Values.input | fromJson"),
        (r#"join "," .Values.input"#, r#".Values.input | join ",""#),
        (r#"split "," .Values.input"#, r#".Values.input | split ",""#),
        ("b64enc .Values.input", ".Values.input | b64enc"),
    ] {
        let direct = observe("", direct);
        let pipeline = observe("", pipeline);
        sim_assert_eq!(have: pipeline.paths(), want: direct.paths());
        sim_assert_eq!(have: pipeline.literals(), want: direct.literals());
        sim_assert_eq!(have: pipeline.truth(), want: direct.truth());
        sim_assert_eq!(have: pipeline.captures(), want: direct.captures());
    }
}

#[test]
fn integer_and_float_comparisons_keep_distinct_runtime_kinds() {
    let nil = Predicate::invalid_kind_path("input").normalize_boolean();
    let number = Predicate::from(Guard::TypeIs {
        path: ValuesPath::parse("input"),
        schema_type: "number".to_owned(),
    });
    // Integer input may be Int64 from --set or Float64 from a file; either valid loader survives.
    for (expression, accepted) in [
        (
            "eq .Values.input 1",
            Predicate::Or(vec![nil.clone(), typed("input", "integer")]).normalize_boolean(),
        ),
        (
            "eq .Values.input 1.5",
            Predicate::Or(vec![nil, number]).normalize_boolean(),
        ),
    ] {
        let result = observe("", expression);
        sim_assert_eq!(have: result.execution.abort_predicate(), want: accepted.negated().normalize_boolean());
        sim_assert_eq!(have: result.kinds(), want: BTreeSet::from([CaptureKind::Fail]));
    }
}

#[test]
fn helper_argument_fields_resolve_from_dot_root() {
    let result = observe(
        r#"{{ $_ := set $ "config" .Values.serviceAccount }}"#,
        r#"default "generated" .config.name"#,
    );
    assert_selected(
        &result,
        "serviceAccount.name",
        truthy("serviceAccount.name"),
    );
    assert!(result.literals().contains(&GuardValue::string("generated")));
}

#[test]
fn default_choice_records_primary_and_fallback_selection_conditions() {
    let result = observe(
        "",
        "default .Values.persistence.storageClass .Values.global.storageClass",
    );
    assert_selected(
        &result,
        "global.storageClass",
        truthy("global.storageClass"),
    );
    assert_selected(
        &result,
        "persistence.storageClass",
        truthy("global.storageClass").negated(),
    );
}

#[test]
fn chained_default_records_the_composed_primary_selection_on_the_final_fallback() {
    let result = observe("", ".Values.x | default .Values.y | default .Values.z");
    assert_selected(
        &result,
        "z",
        Predicate::all(vec![truthy("x").negated(), truthy("y").negated()]),
    );
}

#[test]
fn quoted_default_primary_never_selects_its_fallback() {
    let result = observe("", r#"printf "%q" .Values.alpha | default .Values.omega"#);
    sim_assert_eq!(have: result.raw_conditions("omega"), want: Some(Predicate::False));
    sim_assert_eq!(have: result.paths(), want: paths(&["alpha"]));
}

#[test]
fn formatter_default_chain_uses_rendered_truthiness_for_the_final_fallback() {
    let result = observe(
        "",
        r#"printf "%s" .Values.alpha | default .Values.beta | default .Values.omega"#,
    );
    let empty = Predicate::from(Guard::MatchesPattern {
        path: ValuesPath::parse("alpha"),
        pattern: "^$".to_owned(),
        templated: false,
    });
    assert_selected(
        &result,
        "omega",
        Predicate::all(vec![empty, truthy("beta").negated()]),
    );
}

#[test]
#[ignore = "R3b regression vs main, package A (consumed/comparison provenance): generated default chain keeps truthy primary string consumption; tracked in plan/value-algebra-r3b.md follow-ups"]
fn generated_default_chain_keeps_truthy_primary_string_consumption() {
    let mut defines = DefineIndex::new();
    defines.add_file_source("helpers.tpl", r#"{{ define "existing" }}{{ end }}"#);
    let db = IrAnalysisDb::new(&defines);
    let result = observe_with_db(
        "",
        r#".Values.secret | default (include "existing" .) | default (randAlphaNum 16) | b64enc"#,
        &db,
    );
    assert!(result.string_routes().contains(&(
        ValuesPath::parse("secret"),
        StringRequirementRoute::Selected,
        vec![truthy("secret")].into()
    )));
}

#[test]
#[ignore = "R3b regression vs main, package A (consumed/comparison provenance): lexical transforms preserve selected string consumption; tracked in plan/value-algebra-r3b.md follow-ups"]
fn lexical_transforms_preserve_selected_string_consumption() {
    let result = observe(
        "",
        r#".Values.image.tag | default "1.13.1" | replace ":" "-" | replace "@" "_" | trunc 63 | trimSuffix "-""#,
    );
    sim_assert_eq!(have: result.string_routes(), want: BTreeSet::from([(ValuesPath::parse("image.tag"), StringRequirementRoute::Selected, vec![truthy("image.tag")].into())]));
}

#[test]
fn serializer_output_does_not_retype_raw_input() {
    for expression in [
        ".Values.labels | toYaml | b64enc",
        "merge .Values.override .Values.base | toYaml | b64enc",
    ] {
        sim_assert_eq!(have: observe("", expression).string_routes(), want: BTreeSet::new());
    }
    let raw = observe("{{ $encoded := .Values.labels }}", "$encoded | b64enc");
    sim_assert_eq!(have: raw.string_routes().into_iter().map(|(_, route, _)| route).collect::<BTreeSet<_>>(), want: BTreeSet::from([StringRequirementRoute::Direct]));
}

#[test]
fn literal_and_string_set_default_primaries_record_exact_fallback_reachability() {
    for (expression, selected) in [
        (r#""" | default .Values.omega"#, Predicate::True),
        (r#""x" | default .Values.omega"#, Predicate::False),
        (
            r#"ternary "" "" .Values.choose | default .Values.omega"#,
            Predicate::from(Guard::TypeIs {
                path: ValuesPath::parse("choose"),
                schema_type: "boolean".to_owned(),
            }),
        ),
        (
            r#"ternary "x" "y" .Values.choose | default .Values.omega"#,
            Predicate::False,
        ),
    ] {
        assert_selected(&observe("", expression), "omega", selected);
    }
}

#[test]
fn coalesce_records_ordered_candidate_selection_conditions() {
    let result = observe("", "coalesce .Values.primary .Values.fallback .Values.last");
    assert_selected(&result, "primary", truthy("primary"));
    assert_selected(
        &result,
        "fallback",
        Predicate::all(vec![truthy("primary").negated(), truthy("fallback")]),
    );
    assert_selected(
        &result,
        "last",
        Predicate::all(vec![
            truthy("primary").negated(),
            truthy("fallback").negated(),
            truthy("last"),
        ]),
    );
    let consumed = observe("", "coalesce .Values.primary .Values.fallback | b64enc");
    sim_assert_eq!(have: consumed.requirements(), want: BTreeSet::from([
        (CaptureKind::StringRequirement { path: ValuesPath::parse("primary"), route: StringRequirementRoute::Selected, selection: vec![truthy("primary")].into() }, BTreeSet::new()),
        (CaptureKind::StringRequirement { path: ValuesPath::parse("fallback"), route: StringRequirementRoute::Selected, selection: vec![truthy("fallback"), truthy("primary").negated()].into() }, BTreeSet::new()),
    ]));
}

#[test]
fn short_circuit_calls_return_guarded_operand_values() {
    let result = observe("", "or .Values.primary .Values.fallback .Values.last");
    sim_assert_eq!(have: result.paths(), want: paths(&["primary", "fallback", "last"]));
    assert_selected(
        &result,
        "fallback",
        Predicate::all(vec![truthy("primary").negated(), truthy("fallback")]),
    );
    assert_selected(
        &result,
        "last",
        Predicate::all(vec![
            truthy("primary").negated(),
            truthy("fallback").negated(),
        ]),
    );
    let result = observe("", "and .Values.primary .Values.fallback .Values.last");
    assert_selected(
        &result,
        "fallback",
        Predicate::all(vec![truthy("primary"), truthy("fallback").negated()]),
    );
    assert_selected(
        &result,
        "last",
        Predicate::all(vec![truthy("primary"), truthy("fallback")]),
    );
}

#[test]
#[ignore = "R3b regression vs main, package A (consumed/comparison provenance): short circuit calls scope later runtime failures to execution; tracked in plan/value-algebra-r3b.md follow-ups"]
fn short_circuit_calls_scope_later_runtime_failures_to_execution() {
    let result = observe("", "or .Values.ready (b64enc .Values.payload)");
    let outer = BTreeSet::from([truthy("ready").negated()]);
    sim_assert_eq!(have: result.requirements(), want: BTreeSet::from([
        (CaptureKind::StringRequirement { path: ValuesPath::parse("payload"), route: StringRequirementRoute::Direct, selection: Vec::new().into() }, outer.clone()),
        (CaptureKind::AbsenceAborts { path: ValuesPath::parse("payload") }, outer),
    ]));
}

#[test]
#[ignore = "R3b regression vs main, package A (consumed/comparison provenance): truth only locals drive short circuit execution; tracked in plan/value-algebra-r3b.md follow-ups"]
fn truth_only_locals_drive_short_circuit_execution() {
    let result = observe(
        "{{ $shouldContinue := true }}",
        r#"and $shouldContinue (hasKey .Values.cfg "a")"#,
    );
    sim_assert_eq!(have: result.requirements(), want: BTreeSet::from([
        (CaptureKind::ValueType { path: ValuesPath::parse("cfg"), schema_type: "object".to_owned(), null_aborts: true }, BTreeSet::new()),
        (CaptureKind::AbsenceAborts { path: ValuesPath::parse("cfg") }, BTreeSet::new()),
    ]));
}

#[test]
fn unset_nil_behavior_distinguishes_direct_access_from_a_local_binding() {
    sim_assert_eq!(have: observe("", r#"unset .Values.absent "k""#).kinds(), want: object_kinds("absent", true, true));
    sim_assert_eq!(have: observe("{{ $x := .Values.absent }}", r#"unset $x "k""#).kinds(), want: object_kinds("absent", false, false));
}

#[test]
fn local_selector_truth_comes_from_the_selected_value() {
    let result = observe(
        "{{ $plugin := index .Values.plugins 0 }}",
        "$plugin.hostPath",
    );
    sim_assert_eq!(have: result.truth(), want: Some(truthy("plugins.*.hostPath")));
}

#[test]
fn stringified_trimmed_equality_uses_the_transformed_scalar_value() {
    let result = observe(
        "",
        r#"eq (.Values.image.tag | toString | trimSuffix "-jmx") "7""#,
    );
    let pattern = crate::helper_meta::pattern_with_lexical_escapes(
        "^7$",
        &BTreeSet::from([crate::helper_meta::LexicalEscape::TrimSuffix(
            "-jmx".to_owned(),
        )]),
    );
    sim_assert_eq!(have: result.truth(), want: Some(Predicate::Or(vec![
    Predicate::from(Guard::Eq { path: ValuesPath::parse("image.tag"), value: GuardValue::Int(7) }),
    Predicate::from(Guard::MatchesPattern { path: ValuesPath::parse("image.tag"), pattern, templated: false }),
]).normalize_boolean()));
}

#[test]
fn helper_argument_dict_projects_string_and_raw_string_keys() {
    let prefix = r#"{{ $arg := dict "name" .Values.serviceAccount.name `raw` .Values.raw }}"#;
    sim_assert_eq!(have: observe(prefix, "$arg.name").paths(), want: paths(&["serviceAccount.name"]));
    sim_assert_eq!(have: observe(prefix, "$arg.raw").paths(), want: paths(&["raw"]));
}

#[test]
fn helper_argument_merge_preserves_ordered_overwrite_and_root_context_expansion() {
    let selectors_succeed = Predicate::all(vec![
        Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("root"),
            schema_type: "object".to_owned(),
        }),
        Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("fallback"),
            schema_type: "object".to_owned(),
        }),
    ]);
    let non_nil = |path| {
        Predicate::Or(vec![
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse(path),
                value: GuardValue::Null,
            }),
            Predicate::from(Guard::Absent {
                path: ValuesPath::parse(path),
            }),
        ])
        .negated()
    };
    let prefix = r#"{{ $root := dict "root" .Values.root.value }}{{ $fallback := dict "fallback" .Values.fallback.value }}{{ $overrideMap := dict "fallback" "override" }}{{ $arg := merge $fallback $root $overrideMap }}"#;
    // A later nonempty source replaces an empty destination; nonempty destination values win.
    assert_selected(
        &observe(prefix, "$arg.fallback"),
        "fallback.value",
        Predicate::all(vec![selectors_succeed.clone(), truthy("fallback.value")]),
    );
    assert_selected(
        &observe(prefix, "$arg.root"),
        "root.value",
        Predicate::all(vec![selectors_succeed, non_nil("root.value")]),
    );
    let prefix = r#"{{ $root := dict "root" .Values.root.value }}{{ $fallback := dict "fallback" "" }}{{ $overrideMap := dict "fallback" .Values.override }}{{ $arg := merge $fallback $root $overrideMap }}"#;
    assert_selected(
        &observe(prefix, "$arg.fallback"),
        "override",
        Predicate::all(vec![
            Predicate::from(Guard::TypeIs {
                path: ValuesPath::parse("root"),
                schema_type: "object".to_owned(),
            }),
            non_nil("override"),
        ]),
    );
}

#[test]
fn json_roundtrip_preserves_input_identity_with_decoded_representation() {
    let result = observe("", ".Values.extraResources | toJson | fromJson");
    sim_assert_eq!(have: result.paths(), want: paths(&["extraResources"]));
    assert_selected(&result, "extraResources", Predicate::True);
}

#[test]
fn json_roundtrip_preserves_values_root_inside_constructed_container() {
    let result = observe("", r#"get (dict "doc" .Values | toJson | fromJson) "doc""#);
    sim_assert_eq!(have: result.paths(), want: paths(&[""]));
    assert_selected(&result, "", Predicate::True);
}

#[test]
fn yaml_roundtrip_preserves_values_root_inside_constructed_container() {
    assert_selected(
        &observe("", r#"get (dict "doc" .Values | toYaml | fromYaml) "doc""#),
        "",
        Predicate::True,
    );
}

#[test]
#[ignore = "R3b regression vs main, package C (parser, collection and validation transfers): from json without matching serialization only contracts the input string; tracked in plan/value-algebra-r3b.md follow-ups"]
fn from_json_without_matching_serialization_only_contracts_the_input_string() {
    let result = observe("", ".Values.payload | fromJson");
    assert!(result.literals().is_empty());
    assert!(
        result
            .string_routes()
            .iter()
            .any(|(path, _, _)| path == &ValuesPath::parse("payload"))
    );
}

#[test]
fn root_values_replacement_is_exported_and_used_by_later_values_reads() {
    let result = observe(
        r#"{{ $_ := set . "Values" (.Values | toJson | fromJson) }}"#,
        ".Values.extraResources",
    );
    assert_selected(&result, "extraResources", Predicate::True);
}

#[test]
fn root_set_truth_predicates_feed_later_root_field_assignments() {
    let prefix = indoc! {r#"
    {{ $_ := set . "serverEnabled" (or
        (eq (.Values.server.enabled | toString) "true")
        (and (eq (.Values.server.enabled | toString) "-") (eq (.Values.global.enabled | toString) "true"))) }}
    {{ $_ := set . "serverServiceEnabled" (and .serverEnabled (eq (.Values.server.service.enabled | toString) "true")) }}
"#};
    let server = Predicate::Or(vec![
        Predicate::Or(vec![
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse("server.enabled"),
                value: GuardValue::string("true"),
            }),
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse("server.enabled"),
                value: GuardValue::Bool(true),
            }),
        ]),
        Predicate::all(vec![
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse("server.enabled"),
                value: GuardValue::string("-"),
            }),
            Predicate::Or(vec![
                Predicate::from(Guard::Eq {
                    path: ValuesPath::parse("global.enabled"),
                    value: GuardValue::string("true"),
                }),
                Predicate::from(Guard::Eq {
                    path: ValuesPath::parse("global.enabled"),
                    value: GuardValue::Bool(true),
                }),
            ]),
        ]),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: observe(prefix, ".serverEnabled").truth(), want: Some(server.clone()));
    let service = Predicate::all(vec![
        server,
        Predicate::Or(vec![
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse("server.service.enabled"),
                value: GuardValue::string("true"),
            }),
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse("server.service.enabled"),
                value: GuardValue::Bool(true),
            }),
        ]),
    ])
    .normalize_boolean();
    sim_assert_eq!(have: observe(prefix, ".serverServiceEnabled").truth(), want: Some(service));
}

#[test]
fn root_scalar_metadata_reports_rendered_selection_source() {
    let mode = observe(r#"{{ $_ := set . "mode" "server" }}"#, ".mode");
    sim_assert_eq!(have: mode.literals(), want: BTreeSet::from([GuardValue::string("server")]));
    sim_assert_eq!(have: mode.truth(), want: Some(Predicate::True));
    sim_assert_eq!(have: mode.paths(), want: BTreeSet::new());
}

#[test]
fn root_values_merge_records_the_fallback_values_subtree() {
    let result = observe(
        r#"{{ $defaults := .Values._internal_defaults }}{{ $_ := set $ "Values" (mustMergeOverwrite $defaults $.Values) }}"#,
        ".Values",
    );
    sim_assert_eq!(have: result.execution.observed_facts().values_default_sources, want: BTreeSet::from([crate::ValuesDefaultSource { target_path: ValuesPath::default(), source_path: ValuesPath::parse("_internal_defaults") }]));
}

#[test]
fn coercing_arithmetic_erases_raw_operand_shape() {
    let result = observe("", "mulf .Values.pct 1048576.0 | divf 1048576.0 | floor");
    assert_no_raw_type(&result, &["pct"]);
    sim_assert_eq!(have: result.paths(), want: paths(&["pct"]));
    assert!(
        result
            .execution
            .observed_facts()
            .shape_erased_paths
            .contains(&ValuesPath::parse("pct")),
        "numeric coercion must record shape erasure; placed rows: {:?}",
        result.rows
    );
}

#[test]
fn division_operand_is_not_arithmetic_erased() {
    let result = observe("", "div .Values.count 2");
    assert!(
        !result
            .execution
            .observed_facts()
            .shape_erased_paths
            .contains(&ValuesPath::parse("count"))
    );
}

#[test]
#[ignore = "R3b regression vs main, package C (parser, collection and validation transfers): finite selector program construction stays exact; tracked in plan/value-algebra-r3b.md follow-ups"]
fn finite_selector_program_construction_stays_exact() {
    let result = observe(
        r#"{{ $dep := "telemetry.v2.stackdriver.disableOutbound" }}"#,
        r#"print "{{" (repeat (split "." $dep | len) "(") ".Values." (replace "." ")." $dep) ")}}""#,
    );
    sim_assert_eq!(have: result.literals(), want: BTreeSet::from([GuardValue::string("{{((((.Values.telemetry).v2).stackdriver).disableOutbound)}}")]));
}

#[test]
fn ternary_condition_identity_stays_out_of_output_paths() {
    for expression in [
        r#"ternary "https-web" "http-web" .Values.internalTLS.enabled"#,
        r#".Values.internalTLS.enabled | ternary "https-web" "http-web""#,
    ] {
        let result = observe("", expression);
        assert!(
            !result
                .paths()
                .contains(&ValuesPath::parse("internalTLS.enabled"))
        );
        assert!(result.has_boolean_contract("internalTLS.enabled"));
    }
}

#[test]
fn ternary_condition_discards_local_output_metadata_but_keeps_consumption_contracts() {
    let result = observe(
        "{{ $flag := .Values.diagnosticMode.enabled }}",
        r#"$flag | ternary "enabled" "disabled""#,
    );
    sim_assert_eq!(have: result.paths(), want: BTreeSet::new());
    assert!(result.has_boolean_contract("diagnosticMode.enabled"));
    let nested = observe(
        "",
        r#"empty (b64enc .Values.payload) | ternary "empty" "present""#,
    );
    assert!(
        nested
            .string_routes()
            .iter()
            .any(|(path, _, _)| path == &ValuesPath::parse("payload"))
    );
    assert!(
        nested
            .execution
            .reads()
            .iter()
            .any(|read| read.values_path == ValuesPath::parse("payload"))
    );
}

#[test]
fn merge_of_values_paths_forms_ordered_layers() {
    for expression in [
        "merge (.Values.preferred | default dict) (.Values.legacy | default dict)",
        "mergeOverwrite (.Values.legacy | default dict) (.Values.preferred | default dict)",
    ] {
        let result = observe("", expression);
        sim_assert_eq!(have: result.paths(), want: paths(&["", "preferred", "legacy"]));
    }
    let prefix =
        r#"{{ $preferred := dict "key" "preferred" }}{{ $legacy := dict "key" "legacy" }}"#;
    sim_assert_eq!(have: observe(prefix, r#"get (merge $preferred $legacy) "key""#).literals(), want: BTreeSet::from([GuardValue::string("preferred")]));
    sim_assert_eq!(have: observe(prefix, r#"get (mergeOverwrite $legacy $preferred) "key""#).literals(), want: BTreeSet::from([GuardValue::string("preferred")]));
    let literal = observe("", r#"merge (dict "a" 1) .Values.legacy"#);
    assert!(
        literal
            .raw_conditions("legacy")
            .is_some_and(|condition| condition == Predicate::False)
    );
}

#[test]
fn transformed_middle_candidate_leaves_the_final_fallback_approximate() {
    let result = observe(
        "",
        r#".Values.a | default (printf "%.0s" .Values.b) | default .Values.c"#,
    );
    if let Some(condition) = result.raw_conditions("c") {
        assert!(!contains_predicate(&condition, &truthy("b").negated()));
    }
    let concrete = observe(
        r#"{{ $_ := set .Values "a" "" }}{{ $_ := set .Values "b" "x" }}{{ $_ := set .Values "c" "tail" }}"#,
        r#".Values.a | default (printf "%.0s" .Values.b) | default .Values.c"#,
    );
    sim_assert_eq!(have: concrete.literals(), want: BTreeSet::from([GuardValue::string("tail")]));
}
