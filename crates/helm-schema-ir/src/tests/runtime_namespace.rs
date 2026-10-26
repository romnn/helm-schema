use std::collections::BTreeSet;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::Namespace;
use crate::analysis_db::{DefinitionInventory, IrAnalysisDb};
use crate::value::eval::Executor;
use crate::value::{DocumentExecution, ExitBuffer, Field, Flow, Remainder, State, Value};

fn execute(defines: &DefineIndex, source: &str) -> eyre::Result<DocumentExecution> {
    let db = IrAnalysisDb::new(defines);
    let mut state = State::new(Value::nil());
    let root = state.heap.map(Remainder::Closed);
    let target = state.map_target(&root).ok_or_eyre("missing new root map")?;
    state.heap.write(
        &target,
        "Values",
        Field::Present(Value::input(ValuesPath::default())),
    );
    state.frames.clear();
    state.enter_call(root);
    let main = db.parsed_source(
        source,
        Some("chart/templates/main.yaml"),
        "chart/templates/main.yaml",
    );
    let sources = db
        .template_sources()
        .iter()
        .cloned()
        .chain(std::iter::once(Rc::clone(&main)))
        .collect();
    let build = Namespace::from_sources(sources, DefinitionInventory::Complete, &state.heap.arena);
    let namespace = build.namespace.id();
    let mut exits = ExitBuffer::default();
    for exit in build.install(state) {
        match exit.flow {
            Flow::Normal(_) => {
                let namespace = Rc::clone(&exit.state.namespace);
                for lookup in namespace.resolve(&main.name, exit.state) {
                    let super::TemplateLookup {
                        mut state,
                        resolution,
                    } = lookup;
                    match resolution {
                        super::TemplateResolution::Found(program) => {
                            exits.extend(Executor { db: &db }.parsed_body(&program, state));
                        }
                        super::TemplateResolution::Missing => exits.push(crate::value::Exit {
                            state,
                            flow: Flow::Abort(crate::value::AbortReason::MissingTemplate),
                        }),
                        super::TemplateResolution::Unknown => {
                            crate::value::operations::invalidate(&mut state);
                            state.output.emit(state.heap.freeze(&Value::unknown()));
                            exits.extend(crate::value::eval::normal(state, Value::unknown()));
                        }
                    }
                }
            }
            flow => exits.push(crate::value::Exit {
                state: exit.state,
                flow,
            }),
        }
    }
    let exits = exits.finish();
    assert!(
        exits
            .iter()
            .all(|exit| exit.state.namespace.id() == namespace)
    );
    Ok(DocumentExecution::new(exits))
}

fn text(execution: &DocumentExecution) -> Vec<Option<String>> {
    execution
        .text()
        .map(|(_, text)| text.exact_text())
        .collect()
}

#[test]
fn file_definitions_follow_depth_then_reverse_lexical_parse_order() -> eyre::Result<()> {
    let mut defines = DefineIndex::new();
    defines.add_file_source("chart/templates/a.tpl", r#"{{ define "h" }}a{{ end }}"#);
    defines.add_file_source("chart/templates/z.tpl", r#"{{ define "h" }}z{{ end }}"#);
    defines.add_file_source(
        "chart/templates/deep/_h.tpl",
        r#"{{ define "h" }}deep{{ end }}"#,
    );
    sim_assert_eq!(have: text(&execute(&defines, r#"{{ include "h" . }}"#)?), want: vec![Some("a".to_string())]);
    Ok(())
}

#[test]
fn syntactically_empty_file_definitions_preserve_nonempty_bodies() -> eyre::Result<()> {
    for (replacement, expected) in [
        ("   ", "old"),
        ("{{/* empty */}}", "old"),
        ("{{- /* empty */ -}}", "old"),
        (
            concat!("\t", "\n", "\r", "\u{000b}", "\u{000c}", "\u{00a0}"),
            "old",
        ),
        (r#"{{ "" }}"#, ""),
        (r#"{{ print "" }}"#, ""),
    ] {
        let mut defines = DefineIndex::new();
        defines.add_file_source(
            "chart/templates/deep/_h.tpl",
            r#"{{ define "h" }}old{{ end }}"#,
        );
        defines.add_file_source(
            "chart/templates/_h.tpl",
            &format!("{{{{ define \"h\" }}}}{replacement}{{{{ end }}}}"),
        );
        sim_assert_eq!(have: text(&execute(&defines, r#"{{ include "h" . }}"#)?), want: vec![Some(expected.to_string())]);
    }
    Ok(())
}

#[test]
fn tpl_overlays_restore_the_callers_namespace_after_nested_clones() -> eyre::Result<()> {
    let source = r#"{{ define "h" }}old{{ end }}{{ tpl "{{ define \"h\" }}outer{{ end }}{{ tpl \"{{ define \\\"h\\\" }}inner{{ end }}{{ include \\\"h\\\" . }}\" . }}/{{ include \"h\" . }}" . }}/{{ include "h" . }}"#;
    sim_assert_eq!(have: text(&execute(&DefineIndex::new(), source)?), want: vec![Some("inner/outer/old".to_string())]);
    Ok(())
}

#[test]
fn syntactically_empty_tpl_definitions_preserve_prior_bodies() -> eyre::Result<()> {
    for (replacement, expected) in [
        ("   ", "old"),
        ("{{/* empty */}}", "old"),
        ("{{- /* empty */ -}}", "old"),
        (
            concat!("\t", "\n", "\r", "\u{000b}", "\u{000c}", "\u{00a0}"),
            "old",
        ),
        (r#"{{ "" }}"#, ""),
        (r#"{{ print "" }}"#, ""),
    ] {
        let mut defines = DefineIndex::new();
        defines.add_file_source("chart/templates/_h.tpl", r#"{{ define "h" }}old{{ end }}"#);
        let program =
            format!("{{{{ define \"h\" }}}}{replacement}{{{{ end }}}}{{{{ include \"h\" . }}}}");
        let quoted = serde_json::to_string(&program)?;
        let source = format!("{{{{ tpl {quoted} . }}}}/{{{{ include \"h\" . }}}}");
        sim_assert_eq!(have: text(&execute(&defines, &source)?), want: vec![Some(format!("{expected}/old"))]);
    }
    Ok(())
}

#[test]
fn duplicate_nonempty_definitions_abort_before_any_body_runs() -> eyre::Result<()> {
    let execution = execute(
        &DefineIndex::new(),
        r#"{{ define "h" }}first{{ end }}{{ define "h" }}second{{ end }}{{ "unreached" }}"#,
    )?;
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::True);
    sim_assert_eq!(have: execution.sites().count(), want: 0);
    sim_assert_eq!(have: execution.invocations().count(), want: 0);
    Ok(())
}

#[test]
fn complete_missing_names_abort_but_recovery_trees_remain_unknown() -> eyre::Result<()> {
    let missing = execute(&DefineIndex::new(), r#"{{ include "absent" . }}"#)?;
    sim_assert_eq!(have: missing.abort_predicate(), want: Predicate::True);
    let mut defines = DefineIndex::new();
    defines.add_file_source("chart/templates/_broken.tpl", "{{ if }}");
    let unknown = execute(&defines, r#"{{ include "absent" . }}"#)?;
    sim_assert_eq!(have: unknown.abort_predicate(), want: Predicate::False);
    sim_assert_eq!(have: text(&unknown), want: vec![None]);
    Ok(())
}

#[test]
fn static_file_payloads_never_contribute_template_definitions() -> eyre::Result<()> {
    let mut defines = DefineIndex::new();
    defines.add_files_get_source(
        "files/program.tpl",
        r#"{{ define "payload-only" }}not-registered{{ end }}"#,
    );
    let execution = execute(&defines, r#"{{ include "payload-only" . }}"#)?;
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::True);
    Ok(())
}

#[test]
fn selected_dynamic_names_use_the_shared_operand_dispatch() -> eyre::Result<()> {
    let source = r#"{{ define "yes" }}left{{ end }}{{ define "no" }}right{{ end }}{{ include (ternary "yes" "no" .Values.selected) . }}"#;
    let execution = execute(&DefineIndex::new(), source)?;
    let texts: BTreeSet<_> = execution
        .sites()
        .flat_map(|site| {
            site.value()
                .alternatives()
                .into_iter()
                .filter_map(|arm| arm.value().exact_text())
                .collect::<Vec<_>>()
        })
        .filter(|text| matches!(text.as_str(), "left" | "right"))
        .collect();
    sim_assert_eq!(have: texts, want: BTreeSet::from(["left".to_string(), "right".to_string()]));
    Ok(())
}

#[test]
fn unknown_tpl_does_not_poison_a_later_static_include() -> eyre::Result<()> {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "chart/templates/_h.tpl",
        r#"{{ define "h" }}known{{ end }}"#,
    );
    let execution = execute(
        &defines,
        r#"{{ $_ := tpl .Values.program . }}{{ include "h" . }}"#,
    )?;
    assert!(
        execution
            .sites()
            .any(|site| site.value().exact_text().as_deref() == Some("known"))
    );
    Ok(())
}

#[test]
fn source_local_duplicates_are_reported_at_their_actual_source() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let state = State::new(Value::nil());
    let source = db.parsed_source(
        r#"{{ define "h" }}first{{ end }}{{ define "h" }}second{{ end }}"#,
        Some("chart/templates/_h.tpl"),
        "chart/templates/_h.tpl",
    );
    let build = Namespace::from_sources(
        vec![source],
        DefinitionInventory::Complete,
        &state.heap.arena,
    );
    let failure = build
        .failures
        .first()
        .ok_or_eyre("missing duplicate definition error")?;
    sim_assert_eq!(have: failure.source_path.as_str(), want: "chart/templates/_h.tpl");
    assert!(
        matches!(&failure.reason, crate::analysis_db::DefinitionError::Duplicate { name } if name == "h")
    );
    let exits = build.install(state);
    sim_assert_eq!(have: exits.len(), want: 1);
    assert!(matches!(
        exits[0].flow,
        Flow::Abort(crate::value::AbortReason::TemplateParse)
    ));
    sim_assert_eq!(have: exits[0].state.path_predicate(), want: Predicate::True);
    Ok(())
}

#[test]
fn definition_names_use_decoded_strings_without_trimming_their_content() -> eyre::Result<()> {
    let source = r#"{{ define " spaced " }}space{{ end }}{{ define "\150" }}octal{{ end }}{{ include " spaced " . }}/{{ include "h" . }}"#;
    sim_assert_eq!(have: text(&execute(&DefineIndex::new(), source)?), want: vec![Some("space/octal".to_string())]);
    let duplicate = r#"{{ define "h" }}first{{ end }}{{ define "\150" }}second{{ end }}"#;
    sim_assert_eq!(have: execute(&DefineIndex::new(), duplicate)?.abort_predicate(), want: Predicate::True);
    Ok(())
}

#[test]
fn tpl_definitions_never_replace_the_callers_empty_helper() -> eyre::Result<()> {
    let mut defines = DefineIndex::new();
    defines.add_file_source("chart/templates/_h.tpl", r#"{{ define "h" }}{{ end }}"#);
    let source = r#"{{ $_ := tpl "{{ define \"h\" }}inner{{ end }}{{ include \"h\" . }}" . }}{{ required "empty caller helper" (include "h" .) }}"#;
    sim_assert_eq!(have: execute(&defines, source)?.abort_predicate(), want: Predicate::True);
    Ok(())
}

#[test]
fn block_invokes_the_resolved_namespace_program_once() -> eyre::Result<()> {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "chart/templates/a.tpl",
        r#"{{ define "h" }}replacement{{ end }}"#,
    );
    let source = r#"before/{{ block "h" . }}default{{ end }}/after"#;
    sim_assert_eq!(have: text(&execute(&defines, source)?), want: vec![Some("before/replacement/after".to_string())]);
    Ok(())
}

#[test]
fn empty_replacement_preserves_an_earlier_whitespace_body_across_file_parses() -> eyre::Result<()> {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "chart/templates/deep/_h.tpl",
        r#"{{ define "h" }} {{ end }}"#,
    );
    defines.add_file_source("chart/templates/_h.tpl", r#"{{ define "h" }}{{ end }}"#);
    let source = r#"{{ required "space survives" (include "h" .) | quote }}"#;
    sim_assert_eq!(have: text(&execute(&defines, source)?), want: vec![Some("\" \"".to_string())]);
    Ok(())
}

#[test]
fn one_parse_keeps_the_last_empty_definition_before_namespace_association() -> eyre::Result<()> {
    let source =
        r#"{{ define "h" }} {{ end }}{{ define "h" }}{{ end }}{{ include "h" . | quote }}"#;
    sim_assert_eq!(have: text(&execute(&DefineIndex::new(), source)?), want: vec![Some("\"\"".to_string())]);
    Ok(())
}

#[test]
fn file_execution_resolves_its_name_after_every_source_has_parsed() -> eyre::Result<()> {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "chart/templates/a.tpl",
        r#"{{ define "chart/templates/main.yaml" }}overridden{{ end }}"#,
    );
    sim_assert_eq!(have: text(&execute(&defines, "original")?), want: vec![Some("overridden".to_string())]);
    Ok(())
}

#[test]
fn a_definition_matching_its_source_name_supplies_the_effective_root_program() -> eyre::Result<()> {
    let source = r#"{{ define "chart/templates/main.yaml" }}associated{{ end }}"#;
    sim_assert_eq!(have: text(&execute(&DefineIndex::new(), source)?), want: vec![Some("associated".to_string())]);
    Ok(())
}

#[test]
fn missing_name_requires_an_explicitly_complete_source_roster() {
    for (inventory, expect_missing) in [
        (DefinitionInventory::Complete, true),
        (DefinitionInventory::Unknown, false),
    ] {
        let mut state = State::new(Value::nil());
        let build = Namespace::from_sources(Vec::new(), inventory, &state.heap.arena);
        state.namespace = Rc::clone(&build.namespace);
        let resolved = build.namespace.resolve("missing", state);
        sim_assert_eq!(have: resolved.len(), want: 1);
        sim_assert_eq!(
            have: resolved.iter().all(|lookup| matches!(lookup.resolution, super::TemplateResolution::Missing)),
            want: expect_missing,
        );
        sim_assert_eq!(
            have: resolved.iter().all(|lookup| matches!(lookup.resolution, super::TemplateResolution::Unknown)),
            want: !expect_missing,
        );
    }
}

#[test]
fn raw_definition_names_follow_the_central_carriage_return_decoder() -> eyre::Result<()> {
    let source = "{{ define `h\rname` }}known{{ end }}{{ include \"hname\" . }}";
    sim_assert_eq!(have: text(&execute(&DefineIndex::new(), source)?), want: vec![Some("known".to_string())]);
    Ok(())
}

#[test]
fn discarded_include_retains_observations_without_emitting_its_text_twice() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        {{- define "h" -}}{{ .value }}{{- end -}}
        {{- $d := dict "value" "first" -}}
        {{- $_ := include "h" $d -}}
        {{- $_ := set $d "value" "second" -}}
        {{- include "h" $d -}}
    "#};
    let execution = execute(&DefineIndex::new(), source)?;
    sim_assert_eq!(have: text(&execution), want: vec![Some("second".to_string())]);
    let includes = execution
        .invocations()
        .filter(|invocation| invocation.function() == "include")
        .count();
    sim_assert_eq!(have: includes, want: 2);
    let observed: BTreeSet<_> = execution
        .sites()
        .filter_map(|site| site.value().exact_text())
        .filter(|text| matches!(text.as_str(), "first" | "second"))
        .collect();
    sim_assert_eq!(have: observed, want: BTreeSet::from(["first".to_string(), "second".to_string()]));
    Ok(())
}

#[test]
fn unknown_tpl_keeps_intrinsic_incompleteness_after_a_conditional_continuation() -> eyre::Result<()>
{
    let source = indoc::indoc! {r#"
        {{- define "h" -}}known{{- end -}}
        {{- $_ := tpl .Values.program . -}}
        {{- if .Values.enabled -}}{{ include "h" . }}{{- else -}}{{ include "h" . }}{{- end -}}
    "#};
    let execution = execute(&DefineIndex::new(), source)?;
    sim_assert_eq!(have: execution.invocation_records_complete(), want: false);
    assert!(
        execution
            .sites()
            .any(|site| site.value().exact_text().as_deref() == Some("known"))
    );
    Ok(())
}

#[test]
fn tpl_parse_abort_restores_the_caller_namespace_and_frame() -> eyre::Result<()> {
    let source = r#"{{ define "h" }}caller{{ end }}{{ tpl "{{ define \"h\" }}first{{ end }}{{ define \"h\" }}second{{ end }}" . }}"#;
    let execution = execute(&DefineIndex::new(), source)?;
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::True);
    for exit in &execution.exits {
        sim_assert_eq!(have: exit.state.frames.len(), want: 1);
        sim_assert_eq!(have: exit.state.calls.len(), want: 0);
    }
    Ok(())
}

#[test]
fn incomplete_roster_retains_known_bindings_and_abstains_on_missing_names() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let mut state = State::new(Value::nil());
    let source = db.parsed_source(
        r#"{{ define "h" }}known{{ end }}"#,
        Some("chart/templates/_h.tpl"),
        "chart/templates/_h.tpl",
    );
    let build = Namespace::from_sources(
        vec![source],
        DefinitionInventory::Unknown,
        &state.heap.arena,
    );
    state.namespace = Rc::clone(&build.namespace);
    let mut found = build.namespace.resolve("h", state);
    sim_assert_eq!(have: found.len(), want: 1);
    let known = found.pop().ok_or_eyre("missing supplied binding")?;
    match known.resolution {
        super::TemplateResolution::Found(program) => {
            sim_assert_eq!(have: program.source(), want: "known");
        }
        super::TemplateResolution::Missing | super::TemplateResolution::Unknown => {
            eyre::bail!("incomplete roster hid its supplied binding")
        }
    }
    let mut absent = build.namespace.resolve("absent", known.state);
    sim_assert_eq!(have: absent.len(), want: 1);
    match absent
        .pop()
        .ok_or_eyre("missing absent-name alternative")?
        .resolution
    {
        super::TemplateResolution::Unknown => {}
        super::TemplateResolution::Found(_) | super::TemplateResolution::Missing => {
            eyre::bail!("incomplete roster claimed a missing name was resolved")
        }
    }
    Ok(())
}

#[test]
fn incomplete_parsed_source_prevents_known_and_missing_resolution() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let state = State::new(Value::nil());
    let known = db.parsed_source(
        r#"{{ define "h" }}known{{ end }}"#,
        Some("chart/templates/_h.tpl"),
        "chart/templates/_h.tpl",
    );
    let broken = db.parsed_source(
        "{{ if }}",
        Some("chart/templates/_broken.tpl"),
        "chart/templates/_broken.tpl",
    );
    let build = Namespace::from_sources(
        vec![known, broken],
        DefinitionInventory::Complete,
        &state.heap.arena,
    );
    for name in ["h", "absent"] {
        let lookups = build.namespace.resolve(name, state.clone());
        sim_assert_eq!(have: lookups.len(), want: 1);
        assert!(matches!(
            lookups[0].resolution,
            super::TemplateResolution::Unknown
        ));
        sim_assert_eq!(have: lookups[0].state.path_predicate(), want: Predicate::True);
    }
}
