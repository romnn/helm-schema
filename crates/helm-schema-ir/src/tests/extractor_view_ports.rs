use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{FailValueRequirement, GuardDnf, Predicate, ValuesPath};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::{ContractUse, Guard};

#[derive(Debug)]
struct RangeSubject {
    path: ValuesPath,
    condition: Predicate,
    allow_integer: bool,
}

#[derive(Debug)]
struct InputRead {
    path: ValuesPath,
    condition: GuardDnf,
}

#[derive(Debug)]
struct RenderedOrigin {
    source_expr: ValuesPath,
    condition: GuardDnf,
}

impl RenderedOrigin {
    fn single_guard_conjunction(&self) -> Vec<Guard> {
        let conjunction = self.condition.single_guard_conjunction();
        assert!(
            conjunction.is_some(),
            "the observed guard must be exact: {self:#?}"
        );
        conjunction.unwrap_or_default()
    }
}

#[derive(Debug)]
struct Evaluation {
    exported: Vec<ContractUse>,
    rendered: Vec<ContractUse>,
    ranges: Vec<RangeSubject>,
    reads: Vec<InputRead>,
    origins: Vec<RenderedOrigin>,
}

fn generate(source: &str, helpers: &str) -> Evaluation {
    let mut defines = DefineIndex::new();
    if !helpers.is_empty() {
        defines.add_file_source("helpers.tpl", helpers);
    }
    let db = IrAnalysisDb::new(&defines);
    let mut result = Evaluation {
        exported: Vec::new(),
        rendered: Vec::new(),
        ranges: Vec::new(),
        reads: Vec::new(),
        origins: Vec::new(),
    };
    super::support::with_execution(source, None, &db, |execution| {
        result.exported = super::support::project_execution(source, None, &db, execution)
            .contract
            .finalize()
            .uses()
            .to_vec();
        result.rendered = result
            .exported
            .iter()
            .filter(|row| !row.path.0.is_empty())
            .cloned()
            .collect();
        result.reads = execution
            .reads()
            .iter()
            .filter(|read| read.kind == crate::ValueKind::WidenedDependency)
            .map(|read| InputRead {
                path: read.values_path.clone(),
                condition: read.condition.clone(),
            })
            .collect();
        result.ranges = execution
            .observed_facts()
            .consumptions
            .iter()
            .filter_map(|consumption| {
                let crate::value::ConsumptionDomain::Known {
                    preimage: FailValueRequirement::Iterable { allow_integer },
                    requires_presence: _,
                } = &consumption.domain
                else {
                    return None;
                };
                Some(RangeSubject {
                    path: consumption.path.clone(),
                    condition: consumption.test_condition(),
                    allow_integer: *allow_integer,
                })
            })
            .collect();
        for site in execution.sites().filter(|site| site.rendered()) {
            for source_expr in site.value().inputs() {
                result.origins.push(RenderedOrigin {
                    source_expr,
                    condition: GuardDnf::from_conjunction([site.condition().clone()]),
                });
            }
        }
    });
    result
}

fn truthy(path: &str) -> Guard {
    Guard::Truthy {
        path: ValuesPath::parse(path),
    }
}

fn contributions<'a>(evaluation: &'a Evaluation, path: &str) -> Vec<&'a ContractUse> {
    let path = ValuesPath::parse(path);
    evaluation
        .exported
        .iter()
        .filter(|row| {
            row.source_expr == path
                && !matches!(
                    row.kind,
                    crate::ValueKind::WidenedDependency | crate::ValueKind::WidenedRegion
                )
        })
        .collect()
}

fn range_guard(path: &str) -> Guard {
    Guard::Range {
        path: ValuesPath::parse(path),
    }
}

#[test]
#[ignore = "R3b missing mechanism: the executed range has no placed mapping fragment at data; preserve the header-read and fragment count."]
fn destructuring_range_header_emits_value_use_for_range_expression() -> eyre::Result<()> {
    // The unconditional header read and the guarded rendered fragment are separate roles.
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- range $k, $v := .Values.map }}
          {{ $k }}: "{{ $v }}"
        {{- end }}
    "#};

    let evaluation = generate(template, "");
    let ir = &evaluation.rendered;

    let map_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|u| u.source_expr == helm_schema_core::ValuesPath::parse("map"))
        .collect();
    let headers: Vec<_> = evaluation
        .reads
        .iter()
        .filter(|subject| subject.path == ValuesPath::parse("map"))
        .collect();
    sim_assert_eq!(
        have: headers.len() + map_uses.len(),
        want: 2,
        "expected a header input read plus a fragment use for `map`; \
         got: {ir:#?}",
    );

    sim_assert_eq!(have: headers.len(), want: 1, "one physical header read: {ir:#?}");
    let header = headers
        .first()
        .copied()
        .ok_or_eyre("header input read present")?;
    sim_assert_eq!(have: &header.condition, want: &GuardDnf::unconditional(), "the header has no enclosing activation guard");
    let consumers: Vec<_> = evaluation
        .ranges
        .iter()
        .filter(|subject| subject.path == ValuesPath::parse("map"))
        .collect();
    assert!(
        !consumers.is_empty()
            && consumers
                .iter()
                .all(|consumer| !consumer.allow_integer
                    && !consumer.condition.contains_approximation()),
        "the range must preserve its exact collection-only consumption: {evaluation:#?}"
    );

    let fragment_use = map_uses
        .iter()
        .copied()
        .find(|use_| use_.path.0 == ["data".to_string()] && use_.kind == crate::ValueKind::Fragment)
        .ok_or_eyre("range fragment use present")?;
    assert!(
        fragment_use
            .single_guard_conjunction()
            .contains(&range_guard("map")),
        "the fragment use should inherit Range(map); got guards: {:?}",
        fragment_use.single_guard_conjunction(),
    );
    Ok(())
}

#[test]
#[ignore = "R3b missing mechanism: default-selected range output loses its placed mapping fragment and source provenance."]
fn destructuring_range_header_with_helper_call_inside_range_expression() -> eyre::Result<()> {
    // Default selection does not erase the header read or the selected collection's provenance.
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- range $k, $v := default (dict) .Values.fallbackMap }}
          {{ $k }}: "{{ $v }}"
        {{- end }}
    "#};

    let evaluation = generate(template, "");
    let ir = &evaluation.rendered;

    let fallback_map_uses: Vec<&ContractUse> = ir
        .iter()
        .filter(|u| u.source_expr == helm_schema_core::ValuesPath::parse("fallbackMap"))
        .collect();
    let headers: Vec<_> = evaluation
        .reads
        .iter()
        .filter(|subject| subject.path == ValuesPath::parse("fallbackMap"))
        .collect();
    sim_assert_eq!(
        have: headers.len() + fallback_map_uses.len(),
        want: 2,
        "expected a header input read plus a fragment use for `fallbackMap`; got: {ir:#?}",
    );

    sim_assert_eq!(have: headers.len(), want: 1, "one physical header read: {ir:#?}");
    let header = headers
        .first()
        .copied()
        .ok_or_eyre("header input read present")?;
    sim_assert_eq!(have: &header.condition, want: &GuardDnf::unconditional(), "the header has no enclosing activation guard");
    let consumers: Vec<_> = evaluation
        .ranges
        .iter()
        .filter(|subject| subject.path == ValuesPath::parse("fallbackMap"))
        .collect();
    assert!(
        !consumers.is_empty()
            && consumers
                .iter()
                .all(|consumer| !consumer.allow_integer
                    && !consumer.condition.contains_approximation()),
        "the range must preserve its exact collection-only consumption: {evaluation:#?}"
    );

    let fragment_use = fallback_map_uses
        .iter()
        .copied()
        .find(|use_| use_.path.0 == ["data".to_string()] && use_.kind == crate::ValueKind::Fragment)
        .ok_or_eyre("range fragment use present")?;
    assert!(
        fragment_use
            .single_guard_conjunction()
            .contains(&range_guard("fallbackMap")),
        "the fragment use should inherit Range(fallbackMap); got guards: {:?}",
        fragment_use.single_guard_conjunction(),
    );

    // No phantom paths from misparsing `default`, `(dict)`, etc.
    let phantoms: Vec<String> = evaluation
        .exported
        .iter()
        .map(|u| u.source_expr.encode())
        .chain(
            evaluation
                .ranges
                .iter()
                .map(|subject| subject.path.encode()),
        )
        .chain(
            evaluation
                .origins
                .iter()
                .map(|origin| origin.source_expr.encode()),
        )
        .filter(|s| matches!(s.as_str(), "dict" | "default"))
        .collect();
    assert!(
        phantoms.is_empty(),
        "no `default`/`dict` builtin should surface as a `.Values.*` path; \
         got: {phantoms:?}",
    );
    Ok(())
}

#[test]
fn range_body_uses_inherit_truthy_guard_on_destructured_source() -> eyre::Result<()> {
    // Inside the destructured range body, any `.Values.X` reference
    // must carry a Range guard on the destructured source (`themap`).
    // This proves the guard pushed by the range header propagates into
    // body uses — the exact contract `SymbolicIrContext` requires.
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          fallback: "{{ .Values.fallback }}"
        {{- range $k, $v := .Values.themap }}
          {{ $k }}: "{{ $v }}-{{ $.Values.suffix }}"
        {{- end }}
    "#};

    let evaluation = generate(template, "");
    let ir = &evaluation.rendered;

    // `suffix` is referenced INSIDE the range body — must carry the
    // Range(themap) guard. The IR shape for this fixture is precise
    // enough to pin exactly one use; a regression that emitted a
    // second wrongly-guarded duplicate would fail this assertion.
    let suffix_uses: Vec<&RenderedOrigin> = evaluation
        .origins
        .iter()
        .filter(|u| u.source_expr == helm_schema_core::ValuesPath::parse("suffix"))
        .collect();
    sim_assert_eq!(
        have: suffix_uses.len(),
        want: 1,
        "expected exactly one `suffix` use inside range body; got: {ir:#?}",
    );
    let suffix = suffix_uses
        .first()
        .copied()
        .ok_or_eyre("suffix occurrence present")?;
    assert!(
        suffix
            .single_guard_conjunction()
            .contains(&range_guard("themap")),
        "expected `suffix` use guarded by Range(themap); got: {:?}",
        suffix.single_guard_conjunction(),
    );
    let exported_suffix = contributions(&evaluation, "suffix");
    sim_assert_eq!(have: exported_suffix.len(), want: 1, "an exported duplicate remains a regression: {exported_suffix:#?}");
    assert!(
        evaluation
            .exported
            .iter()
            .filter(|row| row.source_expr == ValuesPath::parse("suffix"))
            .all(|row| {
                row.single_guard_conjunction()
                    .contains(&range_guard("themap"))
            })
    );

    // `fallback` is OUTSIDE the range — must NOT carry that guard.
    let fallback_uses: Vec<&RenderedOrigin> = evaluation
        .origins
        .iter()
        .filter(|u| u.source_expr == helm_schema_core::ValuesPath::parse("fallback"))
        .collect();
    assert!(
        !fallback_uses.is_empty(),
        "expected `fallback` use outside range; got: {ir:#?}",
    );
    assert!(
        fallback_uses.iter().all(|u| {
            let guards = u.single_guard_conjunction();
            !guards.contains(&truthy("themap")) && !guards.contains(&range_guard("themap"))
        }),
        "`fallback` use outside the range body must NOT inherit the \
         range's Truthy guard; got: {fallback_uses:#?}",
    );
    let exported_fallback = contributions(&evaluation, "fallback");
    assert!(
        !exported_fallback.is_empty(),
        "the outside-range render must survive export: {evaluation:#?}"
    );
    assert!(
        evaluation
            .exported
            .iter()
            .filter(|row| row.source_expr == ValuesPath::parse("fallback"))
            .all(|row| {
                let guards = row.single_guard_conjunction();
                !guards.contains(&truthy("themap")) && !guards.contains(&range_guard("themap"))
            })
    );
    Ok(())
}

/// A range over a local whose value is a selection of two exact lists
/// iterates BOTH lists exactly, so each alternative's members resolve.
///
/// The subject is one unconditional binding leaf holding an ordered choice,
/// which is the shape a `splitList` over a branch-selected key produces. An
/// alternative that is not itself a list must not abandon the whole exact
/// iteration: the joined value still supplies one.
#[test]
#[ignore = "R3b missing mechanism: selected exact-list iteration loses a/b/c/d/e identity and joined output placement."]
fn range_over_selected_list_iterates_each_exact_alternative() {
    let template = indoc! {r#"
        {{- $parts := splitList "." (ternary "a.b" "c.d.e" .Values.pick) -}}
        apiVersion: v1
        kind: ConfigMap
        data:
          joined: {{ range $parts }}{{ index $.Values . }}{{ end }}
    "#};

    let evaluation = generate(template, "");
    let ir = &evaluation.rendered;
    let joined_sources: BTreeSet<String> = ir
        .iter()
        .filter(|use_| use_.path.0 == ["data".to_string(), "joined".to_string()])
        .map(|use_| use_.source_expr.encode())
        .collect();

    let executed_sources: BTreeSet<_> = evaluation
        .origins
        .iter()
        .map(|origin| origin.source_expr.encode())
        .collect();
    sim_assert_eq!(have: &executed_sources, want: &joined_sources, "frozen value provenance and exported output placement must agree: {evaluation:#?}");

    sim_assert_eq!(
        have: joined_sources,
        want: BTreeSet::from([
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ]),
        "each exactly iterable alternative must supply its members: {ir:#?}"
    );
}

#[test]
#[ignore = "R3b missing mechanism: collection consumption survives but the serialized per-item provider placement is absent."]
fn scalar_item_range_keeps_parent_collection_path() -> eyre::Result<()> {
    let template = indoc! {r"
        apiVersion: v1
        kind: PersistentVolumeClaim
        metadata:
          name: test
        spec:
          accessModes:
          {{- range .Values.accessModes }}
            - {{ . | quote }}
          {{- end }}
    "};

    let evaluation = generate(template, "");
    let ir = &evaluation.rendered;

    let parents: Vec<_> = evaluation
        .ranges
        .iter()
        .filter(|subject| subject.path == ValuesPath::parse("accessModes"))
        .collect();
    sim_assert_eq!(have: parents.len(), want: 1, "one collection-level range consumer: {evaluation:#?}");
    let parent = parents
        .first()
        .copied()
        .ok_or_eyre("collection-level header input read present")?;
    sim_assert_eq!(have: &parent.condition, want: &Predicate::True, "the collection header is unconditional");

    let item_use = ir
        .iter()
        .find(|use_| {
            use_.source_expr == helm_schema_core::ValuesPath::parse("accessModes.*")
                && use_.path.0 == ["spec".to_string(), "accessModes[*]".to_string()]
                // Quoted items are total stringifications, so the serialized sink sees text.
                && use_.kind == crate::ValueKind::Serialized
        })
        .ok_or_eyre("scalar-item range should still emit per-item uses")?;
    assert!(
        item_use
            .single_guard_conjunction()
            .contains(&range_guard("accessModes")),
        "the item use should inherit Range(accessModes); got: {:?}",
        item_use.single_guard_conjunction(),
    );
    Ok(())
}

#[test]
#[ignore = "R3b missing mechanism: nested scalar range loses hosts.*.paths.* identity and its leaf provider placement."]
fn scalar_range_wrapped_into_object_item_stays_on_leaf_path() -> eyre::Result<()> {
    let template = indoc! {r"
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
        spec:
          rules:
          {{- range .Values.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                {{- range .paths }}
                  - path: {{ . | quote }}
                    pathType: Prefix
                    backend:
                      service:
                        name: app
                        port:
                          number: 80
                {{- end }}
          {{- end }}
    "};

    let evaluation = generate(template, "");
    let ir = &evaluation.rendered;

    assert!(
        ir.iter().all(|use_| {
            !(use_.source_expr == helm_schema_core::ValuesPath::parse("hosts.*.paths")
                && use_.path.0
                    == [
                        "spec".to_string(),
                        "rules[*]".to_string(),
                        "http".to_string(),
                        "paths".to_string(),
                    ])
        }),
        "the scalar input list should not be projected onto the output object array: {ir:#?}",
    );

    assert!(
        evaluation
            .origins
            .iter()
            .any(|origin| origin.source_expr == ValuesPath::parse("hosts.*.paths.*")),
        "the quoted leaf must retain its exact nested input identity: {evaluation:#?}"
    );

    let leaf_use = ir
        .iter()
        .find(|use_| {
            use_.source_expr == helm_schema_core::ValuesPath::parse("hosts.*.paths.*")
                && use_.kind == crate::ValueKind::Serialized
        })
        .ok_or_eyre("scalar path item should still surface as a value use")?;
    assert!(
        leaf_use
            .single_guard_conjunction()
            .contains(&range_guard("hosts"))
            && leaf_use
                .single_guard_conjunction()
                .contains(&range_guard("hosts.*.paths")),
        "the wrapped scalar item use should retain both enclosing range guards; got: {:?}",
        leaf_use.single_guard_conjunction(),
    );
    Ok(())
}
