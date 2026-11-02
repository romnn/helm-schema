use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::ValuesPath;
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;

impl crate::value::DocumentExecution {
    fn serialized_payload<'a>(
        &self,
        value: &'a crate::value::FrozenValue,
    ) -> Vec<crate::value::use_policy::RelationshipUse<'a>> {
        self.relationships(value, &[], 1024)
            .known
            .into_iter()
            .filter_map(crate::value::relationship::Relationship::destination)
            .filter(|payload| {
                payload.capture.is_none()
                    && payload.output_path.is_empty()
                    && matches!(
                        payload.kind,
                        crate::ValueKind::YamlSerialized
                            | crate::ValueKind::TemplatedYamlSerialized
                    )
            })
            .collect()
    }
}

fn payload_origins(source: &str, helpers: &str) -> BTreeSet<ValuesPath> {
    let mut defines = DefineIndex::new();
    defines.add_file_source("_helpers.tpl", helpers);
    let db = IrAnalysisDb::new(&defines);
    let execution = execute_document(source, None, &db);
    execution
        .sites()
        .filter(|site| site.rendered())
        .flat_map(|site| {
            execution
                .serialized_payload(site.value())
                .into_iter()
                .filter(|payload| payload.omitted_members.is_empty())
                .map(|payload| payload.source.into_owned())
        })
        .collect()
}

/// A later alias write cannot change the source snapshot retained by serialization.
#[test]
fn serialization_before_an_alias_write_keeps_the_original_payload() {
    let source = indoc! {r#"
        {{- $alias := .Values.payload -}}
        {{- $saved := toYaml $alias -}}
        {{- $_ := set $alias "changed" "later" -}}
        value: {{ $saved | nindent 2 }}
    "#};
    sim_assert_eq!(
        have: payload_origins(source, ""),
        want: BTreeSet::from([ValuesPath::parse("payload")])
    );
}

/// Reading an alias does not freeze the map: serialization after a write sees modified members.
#[test]
fn serialization_after_an_alias_write_withdraws_the_whole_input_claim() {
    let source = indoc! {r#"
        {{- $alias := .Values.payload -}}
        {{- $_ := set $alias "changed" "earlier" -}}
        value: {{ toYaml $alias | nindent 2 }}
    "#};
    sim_assert_eq!(have: payload_origins(source, ""), want: BTreeSet::new());
}

/// Executed helper captures retain the same whole-payload relationship as direct serializers.
#[test]
fn helper_captures_and_json_preserve_the_borrowed_origin() {
    let helpers = indoc! {r#"
        {{- define "serialize" -}}{{ toYaml . }}{{- end -}}
    "#};
    for source in [
        "value: {{ toYaml .Values.payload | nindent 2 }}",
        "value: {{ toJson .Values.payload }}",
        r#"value: {{ include "serialize" .Values.payload | nindent 2 }}"#,
    ] {
        sim_assert_eq!(
            have: payload_origins(source, helpers),
            want: BTreeSet::from([ValuesPath::parse("payload")]),
            "{source}"
        );
    }
}

/// Literal whitespace around a captured serializer does not erase its payload relationship.
#[test]
fn multipart_capture_preserves_serialized_origin() {
    let helpers = indoc! {r#"
        {{ define "serialize" }}
        {{ toYaml . }}
        {{ end }}
    "#};
    sim_assert_eq!(
        have: payload_origins(r#"value: {{ include "serialize" .Values.payload | nindent 2 }}"#, helpers),
        want: BTreeSet::from([ValuesPath::parse("payload")])
    );
}

/// Decoding a serializer keeps the operand snapshot through a later serialization.
#[test]
fn decoded_serialization_preserves_frozen_source() {
    let source = indoc! {r#"
        {{- $saved := fromYaml (toYaml .Values.payload) -}}
        {{- $_ := set .Values.payload "changed" "later" -}}
        value: {{ toYaml $saved | nindent 2 }}
    "#};
    sim_assert_eq!(
        have: payload_origins(source, ""),
        want: BTreeSet::from([ValuesPath::parse("payload")])
    );
}

/// Unknown decoded kinds share one operation preimage even without a values-level kind test.
#[test]
fn json_decoder_keeps_its_common_source_preimage() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document("{{ fromJson (toJson .Values.payload) }}", None, &db);
    let site = execution
        .sites()
        .find(|site| site.rendered())
        .ok_or_eyre("decoded output missing")?;
    let view = execution.relationships(site.value(), &[], 1024);
    sim_assert_eq!(have: view.known.len(), want: 1);
    sim_assert_eq!(have: view.known.first().ok_or_eyre("decoded source missing")?.source.as_ref(), want: &ValuesPath::parse("payload"));
    Ok(())
}

/// Whole-map and member queries agree on surviving fields after an overwrite.
#[test]
fn relationship_query_projects_surviving_constructed_members() -> eyre::Result<()> {
    use crate::value::relationship::RelationshipQuery;
    use crate::value::{Field, Remainder, State, Value};

    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let object = state.map_target(&map).ok_or_eyre("map backing missing")?;
    state.heap.write(
        &object,
        "keep",
        Field::Present(Value::input(ValuesPath::parse("original"))),
    );
    state.heap.write(
        &object,
        "overwrite",
        Field::Present(Value::input(ValuesPath::parse("discarded"))),
    );
    state
        .heap
        .write(&object, "overwrite", Field::Present(Value::string("fixed")));
    let frozen = state.heap.freeze(&map);
    let inputs = state.heap.arena.input_objects.borrow();
    let whole = RelationshipQuery::new(&inputs, 1024).value(&frozen, &[]);
    sim_assert_eq!(have: whole.known.len(), want: 1);
    let kept = whole.known.first().ok_or_eyre("surviving member missing")?;
    sim_assert_eq!(have: kept.output_path.clone(), want: vec!["keep".to_owned()]);
    sim_assert_eq!(have: kept.source.as_ref(), want: &ValuesPath::parse("original"));
    let member = RelationshipQuery::new(&inputs, 1024).value(&frozen, &["keep".to_owned()]);
    sim_assert_eq!(have: member.known.first().ok_or_eyre("requested member missing")?.source.as_ref(), want: kept.source.as_ref());
    Ok(())
}

/// Picking named members cannot claim that the discarded remainder still survives.
#[test]
fn relationship_query_pick_keeps_only_selected_members() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(r#"{{ toYaml (pick .Values.payload "kept") }}"#, None, &db);
    let site = execution
        .sites()
        .find(|site| site.rendered())
        .ok_or_eyre("picked output missing")?;
    let view = execution.relationships(site.value(), &[], 1024);
    sim_assert_eq!(have: view.known.len(), want: 1);
    let arm = view.known.first().ok_or_eyre("selected member missing")?;
    sim_assert_eq!(have: arm.source.as_ref(), want: &ValuesPath::parse("payload.kept"));
    sim_assert_eq!(have: &arm.output_path, want: &vec!["kept".to_owned()]);
    Ok(())
}

/// A shallow omission preserves nested aliases, so later nested writes withdraw that region.
#[test]
fn omitted_map_keeps_later_alias_exclusions() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- $saved := omit .Values.payload "removed" -}}
        {{- $_ := set .Values.payload.nested "changed" "fixed" -}}
        {{ toYaml $saved }}
    "#};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, None, &db);
    let site = execution
        .sites()
        .find(|site| site.rendered())
        .ok_or_eyre("omitted map output missing")?;
    let view = execution.relationships(site.value(), &[], 1024);
    let arm = view
        .known
        .first()
        .ok_or_eyre("remaining input region missing")?;
    sim_assert_eq!(have: arm.source.as_ref(), want: &ValuesPath::parse("payload"));
    sim_assert_eq!(have: &arm.omitted_members, want: &BTreeSet::from(["nested".to_owned(), "removed".to_owned()]));
    Ok(())
}

/// An unknown write to the decoded object cannot retain its old whole-source certificate.
#[test]
fn widened_decoded_map_drops_its_whole_source_claim() {
    for (decoder, serializer) in [("fromYaml", "toYaml"), ("mustFromJson", "toJson")] {
        let source = formatdoc! {r#"
            {{{{- $saved := {decoder} ({serializer} .Values.payload) -}}}}
            {{{{- $_ := set $saved (randAlphaNum 1) "fixed" -}}}}
            {{{{ toYaml $saved }}}}
        "#};
        sim_assert_eq!(have: payload_origins(&source, ""), want: BTreeSet::new());
    }
}

/// Exhaustion leaves unknown coverage instead of declaring an incomplete query exhaustive.
#[test]
fn relationship_query_budget_exhaustion_widens() {
    use crate::value::relationship::RelationshipQuery;
    use crate::value::{State, Value};
    use helm_schema_core::Predicate;

    let state = State::new(Value::nil());
    let frozen = state
        .heap
        .freeze(&Value::input(ValuesPath::parse("payload")));
    let inputs = state.heap.arena.input_objects.borrow();
    for limit in 0..=1 {
        let mut query = RelationshipQuery::new(&inputs, limit);
        let view = query.value(&frozen, &[]);
        sim_assert_eq!(have: view.unknown, want: Predicate::True);
        sim_assert_eq!(have: view.known.len(), want: 0);
        assert!(query.work() <= limit);
    }
}

/// A repeated decision cannot introduce a source from the contradictory branch.
#[test]
fn relationship_query_keeps_guard_correlation() {
    use crate::value::relationship::RelationshipQuery;
    use crate::value::{Coverage, State, Value, ValueNode};
    use helm_schema_core::Predicate;
    use std::rc::Rc;

    let state = State::new(Value::nil());
    let condition = Predicate::truthy_path("enabled");
    let decision =
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let nested = Value {
        node: Rc::new(ValueNode::Select {
            decision: Rc::clone(&decision),
            yes: Value::input(ValuesPath::parse("live")),
            no: Value::input(ValuesPath::parse("impossible")),
        }),
    };
    let frozen = state.heap.freeze(&Value {
        node: Rc::new(ValueNode::Select {
            decision,
            yes: nested,
            no: Value::string("fixed"),
        }),
    });
    let inputs = state.heap.arena.input_objects.borrow();
    let view = RelationshipQuery::new(&inputs, 1024).value(&frozen, &[]);
    sim_assert_eq!(have: view.known.len(), want: 1);
    if let Some(arm) = view.known.first() {
        sim_assert_eq!(have: arm.source.as_ref(), want: &ValuesPath::parse("live"));
        sim_assert_eq!(have: &arm.condition, want: &condition);
    }
}

/// Composed captures retain every source occurrence without asserting a whole-payload identity.
#[test]
fn relationship_query_traverses_every_capture_part() -> eyre::Result<()> {
    let helpers = indoc! {r#"
        {{- define "composed" -}}
        first: {{ .first }}
        second: {{ .second }}
        {{- end -}}
    "#};
    let source = r#"{{ include "composed" (dict "first" .Values.left "second" .Values.right) }}"#;
    let mut defines = DefineIndex::new();
    defines.add_file_source("_helpers.tpl", helpers);
    let db = IrAnalysisDb::new(&defines);
    let execution = execute_document(source, None, &db);
    let site = execution
        .sites()
        .find(|site| site.rendered())
        .ok_or_eyre("capture output missing")?;
    let view = execution.relationships(site.value(), &[], 1024);
    sim_assert_eq!(have: view.known.iter().map(|arm| arm.source.as_ref().clone()).collect::<BTreeSet<_>>(), want: BTreeSet::from([ValuesPath::parse("left"), ValuesPath::parse("right")]));
    assert!(view.known.iter().all(|arm| arm.capture.is_some()));
    sim_assert_eq!(have: view.unknown, want: helm_schema_core::Predicate::True);
    Ok(())
}

/// Exact serializer bytes still carry constructed members' operation preimages through captures.
#[test]
fn literal_capture_keeps_constructed_member_preimages() -> eyre::Result<()> {
    let helpers = indoc! {r#"
        {{- define "constructed" -}}
        {{- toYaml (dict "type" (typeOf (toString .))) -}}
        {{- end -}}
    "#};
    let mut defines = DefineIndex::new();
    defines.add_file_source("_helpers.tpl", helpers);
    let db = IrAnalysisDb::new(&defines);
    let execution = execute_document(r#"{{ include "constructed" .Values.input }}"#, None, &db);
    let site = execution
        .sites()
        .find(|site| site.rendered())
        .ok_or_eyre("constructed capture missing")?;
    let view = execution.relationships(site.value(), &[], 1024);
    sim_assert_eq!(have: view.known.len(), want: 1);
    let arm = view
        .known
        .first()
        .ok_or_eyre("constructed member preimage missing")?;
    sim_assert_eq!(have: arm.source.as_ref(), want: &ValuesPath::parse("input"));
    sim_assert_eq!(have: &arm.output_path, want: &vec!["type".to_owned()]);
    Ok(())
}

/// Result types do not replace the frozen input needed for a later consumer's preimage.
#[test]
fn relationship_query_retains_operation_preimages() -> eyre::Result<()> {
    use crate::value::relationship::{Preimage, ValueOperation};

    for source in [
        "{{ toString .Values.input }}",
        "{{ int .Values.input }}",
        "{{ typeOf .Values.input }}",
        "{{ b64enc .Values.input }}",
        "{{ tpl (toString .Values.input) . }}",
    ] {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(source, None, &db);
        let site = execution
            .sites()
            .find(|site| site.rendered())
            .ok_or_eyre("operation output missing")?;
        let view = execution.relationships(site.value(), &[], 1024);
        let arm = view
            .known
            .first()
            .ok_or_eyre("operation preimage missing")?;
        sim_assert_eq!(have: arm.source.as_ref(), want: &ValuesPath::parse("input"));
        assert!(
            arm.preimage.iter().any(|step| matches!(
                step,
                Preimage::Text(_)
                    | Preimage::Operation(crate::value::relationship::Derivation {
                        source: _,
                        operation: ValueOperation::Format(
                            crate::value::relationship::TextFormat::Value
                                | crate::value::relationship::TextFormat::String
                        ) | ValueOperation::Numeric(_)
                            | ValueOperation::Descriptor { kind_only: _ },
                    })
            )),
            "{source}"
        );
    }
    Ok(())
}

/// Guarded constants still need their source preimage even with no leaf data origins.
#[test]
fn control_only_operands_keep_their_frozen_preimage() -> eyre::Result<()> {
    use crate::value::relationship::ValueOperation;
    use crate::value::{Coverage, ScalarType, State, Value};
    use helm_schema_core::{GuardValue, Predicate};

    let state = State::new(Value::nil());
    let condition = Predicate::truthy_path("enabled");
    let decision =
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let source = Value::select(
        decision,
        Value::literal(GuardValue::Bool(true)),
        Value::literal(GuardValue::Bool(false)),
    );
    let frozen = state.heap.freeze(&source);
    sim_assert_eq!(have: frozen.inputs(), want: BTreeSet::new());
    let result = Value::scalar(ScalarType::Int, None)
        .derived(frozen, ValueOperation::Numeric(ScalarType::Int));
    let preimage = result
        .operation_preimage()
        .ok_or_eyre("guard-only source preimage was dropped")?;
    sim_assert_eq!(have: &preimage.source.value, want: &source);
    Ok(())
}

/// Reusing one frozen payload preserves both executed destinations after an intervening write.
#[test]
fn one_frozen_payload_keeps_both_output_occurrences() {
    let source = indoc! {r#"
        {{- $saved := toYaml .Values.payload -}}
        {{- $_ := set .Values.payload "changed" "later" -}}
        first: {{ $saved | nindent 2 }}
        second: {{ $saved | nindent 2 }}
    "#};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, None, &db);
    let mut occurrences = BTreeMap::<_, BTreeSet<_>>::new();
    for site in execution.sites().filter(|site| site.rendered()) {
        for payload in execution.serialized_payload(site.value()) {
            occurrences
                .entry(site.span().start)
                .or_default()
                .insert(payload.source.into_owned());
        }
    }
    sim_assert_eq!(have: occurrences.len(), want: 2);
    sim_assert_eq!(
        have: occurrences.into_values().flatten().collect::<BTreeSet<_>>(),
        want: BTreeSet::from([ValuesPath::parse("payload")])
    );
}

/// Template-supplied keys remain present when a later serialized map can overwrite their values.
#[test]
fn member_splices_keep_template_supplied_required_keys() -> eyre::Result<()> {
    let source = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          volumes:
            - name: fixed
              {{- toYaml .Values.payload | nindent 6 }}
    "};
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    let payload = contract
        .uses()
        .iter()
        .find(|row| {
            row.source_expr == ValuesPath::parse("payload")
                && row.kind == crate::ValueKind::YamlSerialized
        })
        .ok_or_eyre("serialized volume member claim missing")?;
    sim_assert_eq!(
        have: payload.template_supplied_member_keys,
        want: BTreeSet::from(["name".to_owned()])
    );
    Ok(())
}

/// Independent guarded members keep their own live conditions beyond the context-product bound.
#[test]
fn independent_guarded_serializers_keep_every_destination() -> eyre::Result<()> {
    use std::fmt::Write as _;

    use helm_schema_core::{Guard, Predicate};

    let mut source = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
    "}
    .to_owned();
    for index in 0..12 {
        writeln!(
            source,
            "{{{{- if .Values.enabled{index} }}}}\n  field{index}: {{{{ toYaml .Values.payload{index} | nindent 4 }}}}\n{{{{- end }}}}"
        )?;
    }
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(&source)
        .finalize();
    for index in 0..12 {
        let path = ValuesPath::parse(&format!("payload{index}"));
        let rows = contract
            .uses()
            .iter()
            .filter(|row| row.source_expr == path && !row.path.0.is_empty())
            .collect::<Vec<_>>();
        sim_assert_eq!(have: rows.len(), want: 1, "payload{index}");
        let row = rows
            .first()
            .ok_or_eyre("guarded serializer placement missing")?;
        sim_assert_eq!(have: &row.path.0, want: &vec!["data".to_owned(), format!("field{index}")]);
        let enabled = Predicate::from(Guard::Truthy {
            path: ValuesPath::parse(&format!("enabled{index}")),
        });
        assert!(
            row.condition
                .disjuncts()
                .iter()
                .all(|parts| parts.contains(&enabled)),
            "payload{index}: {:?}",
            row.condition
        );
    }
    Ok(())
}

/// Scalar YAML and JSON flow nodes bind to their value slot, and YAML never grants flow safety to collections.
#[test]
fn serialized_atomic_positions_keep_their_source_kind_coverage() -> eyre::Result<()> {
    use helm_schema_core::{Guard, Predicate};

    for (expression, json) in [
        ("toYaml .Values.payload | indent 2", false),
        ("toJson .Values.payload", true),
    ] {
        let source = format!(
            "apiVersion: v1\nkind: Pod\nspec: {{ nodeSelector: {{{{ {expression} }}}} }}\n"
        );
        let defines = DefineIndex::new();
        let contract = crate::SymbolicIrContext::new(&defines)
            .generate_contract_ir(&source)
            .finalize();
        let row = contract
            .uses()
            .iter()
            .find(|row| {
                row.source_expr == ValuesPath::parse("payload")
                    && row.path.0 == ["spec", "nodeSelector"]
            })
            .ok_or_eyre("serialized flow value missing")?;
        sim_assert_eq!(have: row.kind, want: crate::ValueKind::YamlSerialized);
        sim_assert_eq!(have: row.strip_marker, want: !json);
        let condition = Predicate::Or(
            row.condition
                .disjuncts()
                .iter()
                .map(|parts| Predicate::all(parts.iter().cloned().collect()))
                .collect(),
        );
        let object = Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("payload"),
            schema_type: "object".to_owned(),
        });
        sim_assert_eq!(have: Predicate::all(vec![condition, object]).normalize_boolean() == Predicate::False, want: !json);
    }
    Ok(())
}

/// A sequence splice contributes each original input member alongside fixed template items.
#[test]
fn serialized_sequence_members_keep_each_item_origin() {
    let source = indoc! {r"
        apiVersion: v1
        kind: Pod
        spec:
          volumes:
            - name: fixed
              emptyDir: {}
            {{- toYaml .Values.extraVolumes | nindent 4 }}
    "};
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    let rows = contract
        .uses()
        .iter()
        .filter(|row| {
            !row.path.0.is_empty() && row.source_expr == ValuesPath::parse("extraVolumes.*")
        })
        .map(|row| (row.path.clone(), row.kind))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: rows, want: vec![(crate::YamlPath(vec!["spec".to_owned(), "volumes[*]".to_owned()]), crate::ValueKind::YamlSerialized)]);
}

/// JSON's template-opener test uses the frozen strings and keys, preserving an exact helper guard.
#[test]
fn json_contains_template_opener_keeps_exact_source_coverage() -> eyre::Result<()> {
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- if not (contains "{{" (toJson .Values.payload)) }}
        data:
          value: {{ toYaml .Values.payload | nindent 4 }}
        {{- end }}
    "#};
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    let row = contract
        .uses()
        .iter()
        .find(|row| row.source_expr == ValuesPath::parse("payload") && !row.path.0.is_empty())
        .ok_or_eyre("guarded serialization missing")?;
    assert!(!row.condition.is_unconditional());
    assert!(
        row.condition
            .disjuncts()
            .iter()
            .flatten()
            .all(|predicate| !predicate.contains_approximation()),
        "{:?}",
        row.condition
    );
    Ok(())
}

/// The same bounded flow alphabet governs keys, nested strings, and emitted guard schemas.
#[test]
fn yaml_flow_spelling_subset_excludes_delimiter_injection() {
    use helm_schema_core::ValuePattern;

    for (value, want) in [
        (
            serde_json::json!({"name": ["normal value", 7, false]}),
            true,
        ),
        (serde_json::json!("a,b"), false),
        (serde_json::json!({"a": "x,", "c": "d"}), false),
        (serde_json::json!({"a,b": "x"}), false),
        (serde_json::json!("a\n"), false),
    ] {
        sim_assert_eq!(have: ValuePattern::YamlFlowStable.matches(&value), want: Some(want));
    }
}

/// A write replaces only its own input member; the untouched remainder still supplies the map.
#[test]
fn modified_payload_keeps_only_surviving_input_members() {
    for mutation in [
        r#"{{- $_ := set $alias "changed" "fixed" -}}"#,
        r#"{{- $_ := set $alias "changed" (dict "nested" "fixed") -}}"#,
    ] {
        let source = format!(
            r#"{{{{- $alias := .Values.payload -}}}}{{{{- if kindIs "map" $alias -}}}}{mutation}value: {{{{ toYaml $alias | nindent 2 }}}}{{{{- end -}}}}"#
        );
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(&source, None, &db);
        let mut claims = BTreeSet::new();
        for site in execution.sites().filter(|site| site.rendered()) {
            for payload in execution.serialized_payload(site.value()) {
                claims.insert((payload.source.into_owned(), payload.omitted_members));
            }
        }
        sim_assert_eq!(
            have: claims,
            want: BTreeSet::from([(
                ValuesPath::parse("payload"),
                BTreeSet::from(["changed".to_owned()])
            )]),
            "{mutation}"
        );
    }
}

/// A later literal YAML member overwrites the serialized member at the same destination.
#[test]
fn later_yaml_member_withdraws_original_member_typing() -> eyre::Result<()> {
    let source = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
          annotations:
            fixed: before
            {{- toYaml .Values.payload | nindent 4 }}
            changed: after
    "};
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    let claims = contract
        .uses()
        .iter()
        .filter(|row| row.source_expr == ValuesPath::parse("payload") && !row.path.0.is_empty())
        .collect::<Vec<_>>();
    sim_assert_eq!(have: claims.len(), want: 1);
    let claim = claims
        .first()
        .ok_or_eyre("partial annotation claim missing")?;
    sim_assert_eq!(have: &claim.path.0, want: &vec!["metadata".to_owned(), "annotations".to_owned()]);
    sim_assert_eq!(
        have: claim.omitted_members.keys().cloned().collect::<BTreeSet<_>>(),
        want: BTreeSet::from(["changed".to_owned()])
    );
    Ok(())
}

/// Cache hits agree with fresh queries and consume the same finite allowance as emitted claims.
#[test]
fn borrowed_query_cache_preserves_guard_correlation() {
    use crate::value::relationship::RelationshipQuery;
    use crate::value::text::TextTerm;
    use crate::value::{Coverage, State, Value};
    use helm_schema_core::Predicate;

    let state = State::new(Value::nil());
    let condition = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let serialize = |path| {
        Value::frozen_text(
            TextTerm::Yaml {
                value: state.heap.freeze(&Value::input(ValuesPath::parse(path))),
                pretty: false,
            },
            BTreeSet::new(),
        )
    };
    let frozen = state.heap.freeze(&Value::select(
        decision,
        serialize("left"),
        serialize("right"),
    ));
    let inputs = state.heap.arena.input_objects.borrow();
    let mut query = RelationshipQuery::new(&inputs, 1024);
    let first = query.value(&frozen, &[]);
    sim_assert_eq!(have: first.unknown.clone(), want: Predicate::False);
    sim_assert_eq!(have: first.known.len(), want: 2);
    for _ in 0..128 {
        let uncached = RelationshipQuery::new(&inputs, 1024).value(&frozen, &[]);
        sim_assert_eq!(have: query.value(&frozen, &[]), want: uncached);
    }
    assert!(query.work() <= 1024);
}

/// Small allowances cannot turn a truncated alternative set into complete provenance.
#[test]
fn exhausted_payload_queries_retain_unknown_coverage() {
    use crate::value::relationship::RelationshipQuery;
    use crate::value::text::TextTerm;
    use crate::value::{Coverage, State, Value};
    use helm_schema_core::Predicate;

    let state = State::new(Value::nil());
    for width in [1, 2, 4, 8, 16, 64] {
        let mut value = Value::unknown();
        for index in 0..width {
            let condition = Predicate::truthy_path(format!("choose{index}"));
            let decision =
                state
                    .heap
                    .arena
                    .decision(condition.clone(), condition.negated(), Coverage::Exact);
            let serialized = Value::frozen_text(
                TextTerm::Yaml {
                    value: state
                        .heap
                        .freeze(&Value::input(ValuesPath::parse(&format!("source{index}")))),
                    pretty: false,
                },
                BTreeSet::new(),
            );
            value = Value::select(decision, serialized, value);
        }
        let frozen = state.heap.freeze(&value);
        let inputs = state.heap.arena.input_objects.borrow();
        for limit in [0, 1, 8, 32] {
            let mut query = RelationshipQuery::new(&inputs, limit);
            let view = query.value(&frozen, &[]);
            assert!(query.work() <= limit);
            // The final opaque alternative remains possible at every width, even before overflow.
            assert_ne!(view.unknown, Predicate::False);
            if width == 1 && limit == 32 {
                sim_assert_eq!(have: view.known.len(), want: 1);
            }
            let sources = (0..width)
                .map(|index| ValuesPath::parse(&format!("source{index}")))
                .collect::<BTreeSet<_>>();
            for claim in view.known {
                assert!(sources.contains(claim.source.as_ref()));
            }
        }
    }
}

/// Unrelated later heap versions do not increase the work of querying an already frozen source.
#[test]
fn payload_query_work_is_independent_of_later_heap_history() -> eyre::Result<()> {
    use crate::value::relationship::RelationshipQuery;
    use crate::value::text::TextTerm;
    use crate::value::{Field, Remainder, State, Value};

    let mut state = State::new(Value::nil());
    let serialized = Value::frozen_text(
        TextTerm::Yaml {
            value: state
                .heap
                .freeze(&Value::input(ValuesPath::parse("payload"))),
            pretty: false,
        },
        BTreeSet::new(),
    );
    let frozen = state.heap.freeze(&serialized);
    let target = state.heap.map(Remainder::Closed);
    let object = state
        .map_target(&target)
        .ok_or_eyre("test map has no backing")?;
    let mut expected = None;
    for history in [0, 1, 8, 64] {
        for index in 0..history {
            state.heap.write(
                &object,
                "changed",
                Field::Present(Value::string(&index.to_string())),
            );
        }
        let inputs = state.heap.arena.input_objects.borrow();
        let mut query = RelationshipQuery::new(&inputs, 1024);
        let view = query.value(&frozen, &[]);
        let have = (view, query.work());
        if let Some(want) = &expected {
            sim_assert_eq!(have: &have, want: want);
        } else {
            expected = Some(have);
        }
    }
    Ok(())
}

/// Shared helper branches grow query work with graph depth, rather than their expanded path count.
#[test]
fn shared_capture_depth_has_bounded_query_work() {
    use crate::value::relationship::RelationshipQuery;
    use crate::value::text::TextTerm;
    use crate::value::{Coverage, OutputNode, State, TextBuffer, TextPart, Value};
    use helm_schema_core::Predicate;
    use std::rc::Rc;

    let state = State::new(Value::nil());
    let source = Value::frozen_text(
        TextTerm::Yaml {
            value: state
                .heap
                .freeze(&Value::input(ValuesPath::parse("payload"))),
            pretty: false,
        },
        BTreeSet::new(),
    );
    let mut frozen = state.heap.freeze(&source);
    for depth in 1..=64 {
        let branch = Rc::new(TextBuffer {
            parts: vec![OutputNode {
                occurrences: Vec::new(),
                part: TextPart::Frozen(frozen),
            }],
            ..TextBuffer::default()
        });
        let condition = Predicate::truthy_path(format!("branch{depth}"));
        let decision =
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact);
        let graph = Rc::new(TextBuffer {
            parts: vec![OutputNode {
                occurrences: Vec::new(),
                part: TextPart::Select {
                    decision,
                    yes: Rc::clone(&branch),
                    no: branch,
                },
            }],
            ..TextBuffer::default()
        });
        let value = Value::frozen_text(
            TextTerm::Capture {
                graph,
                strip_marker: false,
            },
            BTreeSet::new(),
        );
        frozen = state.heap.freeze(&value);
        let inputs = state.heap.arena.input_objects.borrow();
        let mut query = RelationshipQuery::new(&inputs, 1024);
        let view = query.value(&frozen, &[]);
        assert!(query.work() <= (48 * depth + 16).min(1024));
        if depth <= 16 {
            sim_assert_eq!(have: view.unknown, want: Predicate::False);
            sim_assert_eq!(have: view.known.len(), want: 1);
        }
    }
}

/// A changed nested alias is omitted even when the queried parent has its own input backing.
#[test]
fn modified_descendant_members_are_not_claimed_unchanged() -> eyre::Result<()> {
    use crate::value::relationship::RelationshipQuery;
    use crate::value::text::TextTerm;
    use crate::value::{
        AliasSet, Datum, Field, Leaf, MapType, Origin, State, TypeCoverage, UnknownValue, Value,
        ValueNode,
    };
    use std::rc::Rc;

    let mut state = State::new(Value::nil());
    let root = Value::input(ValuesPath::default());
    state
        .map_target(&root)
        .ok_or_eyre("root input backing missing")?;
    let mut child_path = ValuesPath::default();
    child_path.push("nested");
    let child = Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Unknown(UnknownValue {
                types: TypeCoverage::Map(MapType::Interface),
                aliases: AliasSet::RenderGraph,
            }),
            origin: Origin {
                derivation: None,
                identity: Some(child_path),
                influences: BTreeSet::new(),
            },
        })),
    };
    let object = state
        .map_target(&child)
        .ok_or_eyre("nested child backing missing")?;
    state
        .heap
        .write(&object, "changed", Field::Present(Value::string("fixed")));
    let serialized = Value::frozen_text(
        TextTerm::Yaml {
            value: state.heap.freeze(&root),
            pretty: false,
        },
        BTreeSet::new(),
    );
    let frozen = state.heap.freeze(&serialized);
    let inputs = state.heap.arena.input_objects.borrow();
    let claims = RelationshipQuery::new(&inputs, 1024)
        .value(&frozen, &[])
        .known;
    sim_assert_eq!(have: claims.len(), want: 1);
    sim_assert_eq!(have: &claims.first().ok_or_eyre("root partial claim missing")?.omitted_members, want: &BTreeSet::from(["nested".to_owned()]));
    Ok(())
}

/// Reusing a frozen helper value retains each destination's independent occurrence guard.
#[test]
fn reused_payload_keeps_independent_occurrence_guards() -> eyre::Result<()> {
    use helm_schema_core::{Guard, Predicate};

    let source = indoc! {r"
        {{- $saved := toYaml .Values.payload -}}
        apiVersion: v1
        kind: ConfigMap
        data:
          {{- if .Values.leftEnabled }}
          left: {{ $saved | nindent 4 }}
          {{- end }}
          {{- if .Values.rightEnabled }}
          right: {{ $saved | nindent 4 }}
          {{- end }}
    "};
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    let rows = contract
        .uses()
        .iter()
        .filter(|row| row.source_expr == ValuesPath::parse("payload") && !row.path.0.is_empty())
        .collect::<Vec<_>>();
    sim_assert_eq!(have: rows.len(), want: 2);
    for (field, flag) in [("left", "leftEnabled"), ("right", "rightEnabled")] {
        let row = rows
            .iter()
            .find(|row| row.path.0.iter().map(String::as_str).eq(["data", field]))
            .ok_or_eyre("reused payload destination missing")?;
        let guard = Predicate::from(Guard::Truthy {
            path: ValuesPath::parse(flag),
        });
        let other_flag = if flag == "leftEnabled" {
            "rightEnabled"
        } else {
            "leftEnabled"
        };
        let other = Predicate::from(Guard::Truthy {
            path: ValuesPath::parse(other_flag),
        });
        assert!(!row.condition.is_never());
        assert!(
            row.condition
                .disjuncts()
                .iter()
                .all(|parts| parts.contains(&guard)
                    && !parts.contains(&other)
                    && !parts.contains(&other.negated()))
        );
    }
    Ok(())
}

/// The executed renderer's action-free arm retains a provider destination within the query cap.
#[test]
fn executed_renderer_keeps_the_action_free_serialized_payload() -> eyre::Result<()> {
    let helpers = indoc! {r#"
        {{- define "renamed.render" -}}
        {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) }}
        {{- if contains "{{" (toJson .value) }}
          {{- if .scope }}
            {{- tpl (cat "{{- with $.RelativeScope -}}" $value "{{- end }}") (merge (dict "RelativeScope" .scope) .context) }}
          {{- else }}
            {{- tpl $value .context }}
          {{- end }}
        {{- else }}
          {{- $value }}
        {{- end }}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          {{- if .Values.replica.affinity }}
          affinity: {{- include "renamed.render" (dict "value" .Values.replica.affinity "context" $) | nindent 4 }}
          {{- end }}
    "#};
    let mut defines = DefineIndex::new();
    defines.add_file_source("_helpers.tpl", helpers);
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    let row = contract
        .uses()
        .iter()
        .find(|row| {
            row.source_expr == ValuesPath::parse("replica.affinity")
                && row.kind == crate::ValueKind::YamlSerialized
                && row
                    .path
                    .0
                    .iter()
                    .map(String::as_str)
                    .eq(["spec", "affinity"])
        })
        .ok_or_eyre("executed renderer lost its action-free provider destination")?;
    sim_assert_eq!(have: row.kind, want: crate::ValueKind::YamlSerialized);
    sim_assert_eq!(have: row.resource.clone(), want: Some(crate::ResourceRef::concrete("v1".to_owned(), "Pod".to_owned())));
    let path = ValuesPath::parse("replica.affinity");
    let action_free = helm_schema_core::Predicate::Or(vec![
        helm_schema_core::Predicate::from(helm_schema_core::Guard::Absent { path: path.clone() }),
        helm_schema_core::Predicate::from(helm_schema_core::Guard::MatchesValue {
            path,
            pattern: helm_schema_core::ValuePattern::TemplateActionFree,
        }),
    ]);
    let condition = helm_schema_core::Predicate::Or(
        row.condition
            .disjuncts()
            .iter()
            .map(|parts| helm_schema_core::Predicate::all(parts.iter().cloned().collect()))
            .collect(),
    );
    assert!(helm_schema_core::PredicateMemo::new().exactly_implies(&condition, &action_free));
    let evidence = contract
        .schema_signals()
        .evidence_for(&ValuesPath::parse("replica.affinity"))
        .ok_or_eyre("renderer schema evidence missing")?;
    assert!(
        evidence
            .provider_schema_uses
            .iter()
            .chain(
                evidence
                    .conditional_overlays
                    .iter()
                    .flat_map(|overlay| &overlay.evidence.provider_schema_uses)
            )
            .any(|usage| usage.kind == crate::ValueKind::YamlSerialized)
    );
    Ok(())
}

/// An unknown later member map can replace every original member of the earlier contribution.
#[test]
fn competing_member_maps_do_not_type_overwritten_original_members() {
    let source = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
          annotations:
            fixed: fixed
            {{- toYaml .Values.first | nindent 4 }}
            {{- toYaml .Values.second | nindent 4 }}
    "};
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    let claims = contract
        .uses()
        .iter()
        .filter(|row| {
            row.source_expr == ValuesPath::parse("first")
                && row.kind == crate::ValueKind::YamlSerialized
                && !row.path.0.is_empty()
        })
        .count();
    sim_assert_eq!(have: claims, want: 0);
}

struct SequencePrefixCase {
    source: &'static str,
    item: &'static str,
    indentation: usize,
    resource: crate::ResourceRef,
}

/// Unknown literal sequence additions preserve the earlier sink, including indentless sequences.
#[test]
fn independent_unknown_sequence_choices_preserve_the_earlier_resource_sink() {
    let cases = [
        SequencePrefixCase {
            source: indoc! {r"
                apiVersion: v1
                kind: Pod
                metadata:
                  name: {{ .Values.name }}
                spec:
                  containers:
                    - name: fixed
                      image: fixed
            "},
            item: indoc! {"
                - name: choice
                  image: fixed
            "},
            indentation: 4,
            resource: crate::ResourceRef::concrete("v1".to_owned(), "Pod".to_owned()),
        },
        SequencePrefixCase {
            source: indoc! {r#"
                apiVersion: rbac.authorization.k8s.io/v1
                kind: ClusterRole
                metadata:
                  name: {{ .Values.name }}
                rules:
                - apiGroups: [""]
                  resources: ["pods"]
                  verbs: ["get"]
            "#},
            item: indoc! {r#"
                - apiGroups: [""]
                  resources: ["pods"]
                  verbs: ["watch"]
            "#},
            indentation: 0,
            resource: crate::ResourceRef::concrete(
                "rbac.authorization.k8s.io/v1".to_owned(),
                "ClusterRole".to_owned(),
            ),
        },
    ];
    for case in cases {
        sim_assert_eq!(have: earlier_sink_count(case), want: 1);
    }
}

fn earlier_sink_count(case: SequencePrefixCase) -> usize {
    let SequencePrefixCase {
        source,
        item,
        indentation,
        resource,
    } = case;
    let mut source = source.to_owned();
    let padding = " ".repeat(indentation);
    for index in 0..12 {
        source.push_str(&formatdoc! {r#"
                {{{{ if eq (toString .Values.choice{index}) "true" }}}}
            "#});
        for line in item.lines() {
            source.push_str(&padding);
            source.push_str(line);
            source.push('\n');
        }
        source.push_str("{{ end }}");
        source.push('\n');
    }
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(&source)
        .finalize();
    contract
        .uses()
        .iter()
        .filter(|row| {
            row.source_expr == ValuesPath::parse("name")
                && row.kind == crate::ValueKind::Scalar
                && row
                    .path
                    .0
                    .iter()
                    .map(String::as_str)
                    .eq(["metadata", "name"])
                && row.resource.as_ref() == Some(&resource)
        })
        .count()
}
