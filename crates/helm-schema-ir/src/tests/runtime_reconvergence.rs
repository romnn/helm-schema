use std::fmt::Write as _;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{GuardValue, Predicate};
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::symbolic::SymbolicTemplate;
use crate::value::{
    Coverage, DecisionEdge, Exit, Field, Flow, Normal, Remainder, State, Value, ValueNode,
    execute_documents,
};

#[test]
fn exact_guard_projection_stays_exact_across_normalization_limits() {
    let initial = State::new(Value::nil());
    let arena = &initial.heap.arena;
    let mut union = arena.decision(Predicate::False, Predicate::True, Coverage::Exact);
    for index in 0..160 {
        let condition = Predicate::from(helm_schema_core::Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("branch"),
            value: GuardValue::Int(index),
        });
        let atom = arena.decision(condition.clone(), condition.negated(), Coverage::Exact);
        union = arena.surviving_decision(
            &[DecisionEdge {
                test: union,
                taken: true,
            }],
            &[DecisionEdge {
                test: Rc::clone(&atom),
                taken: true,
            }],
        );
        sim_assert_eq!(have: union.coverage, want: Coverage::Exact);
        let selected = arena.runtime_path(&[DecisionEdge {
            test: atom,
            taken: true,
        }]);
        sim_assert_eq!(have: selected.implies_test(&union), want: Some(true));
    }
}

#[test]
fn growing_branch_chains_keep_one_normal_continuation() -> eyre::Result<()> {
    helm_schema_test_support::init_perfetto_from_env()?;
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for count in [1, 2, 4, 8, 16, 32] {
        let span = tracing::info_span!("branch_chain", count);
        let _entered = span.enter();
        let mut source = String::from("{{ $value := float64 .Values.input }}");
        for index in 0..count {
            let keyword = if index == 0 { "if" } else { "else if" };
            write!(
                source,
                "{{{{ {keyword} eq .Values.branch {index} }}}}{{{{ $value = float64 .Values.input }}}}"
            )?;
        }
        source.push_str("{{ end }}{{ $value }}");
        let execution = crate::value::tests::execute_document(&source, None, &db);
        sim_assert_eq!(have: execution.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
    }
    Ok(())
}

#[test]
fn three_surviving_paths_reconverge_in_every_order_without_losing_correlation() -> eyre::Result<()>
{
    let mut initial = State::new(Value::nil());
    let map = initial.heap.map(Remainder::Closed);
    let target = initial.map_target(&map).ok_or_eyre("map missing")?;
    initial.bind("$map".to_owned(), map.clone());
    let atoms: [_; 3] = std::array::from_fn(|_| {
        initial
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial)
    });
    let [branch, left, right] = &atoms;
    let mut states = [
        initial
            .fork(Rc::clone(branch), true)
            .fork(Rc::clone(left), true),
        initial
            .fork(Rc::clone(branch), false)
            .fork(Rc::clone(right), true),
        initial
            .fork(Rc::clone(branch), false)
            .fork(Rc::clone(right), false),
    ];
    for (index, state) in states.iter_mut().enumerate() {
        state.heap.write(
            &target,
            "value",
            Field::Present(Value::string(&index.to_string())),
        );
    }
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let exits = order
            .into_iter()
            .map(|index| Exit {
                state: states[index].clone(),
                flow: Flow::Normal(Normal {
                    value: Value::string(&index.to_string()),
                }),
            })
            .collect();
        let joined = crate::value::control::reconverge(exits);
        sim_assert_eq!(have: joined.len(), want: 1);
        let exit = joined.first().ok_or_eyre("normal continuation missing")?;
        let Flow::Normal(normal) = &exit.flow else {
            eyre::bail!("continuation is terminal");
        };
        for assignment in 0_u8..8 {
            let mut selected = exit.state.clone();
            selected
                .active
                .extend(atoms.iter().enumerate().map(|(index, test)| DecisionEdge {
                    test: Rc::clone(test),
                    taken: assignment & (1 << index) != 0,
                }));
            let possible = assignment & 1 == 0 || assignment & 2 != 0;
            sim_assert_eq!(have: selected.path_predicate() != Predicate::False, want: possible);
            if possible {
                let expected = if assignment & 1 != 0 {
                    "0"
                } else if assignment & 4 != 0 {
                    "1"
                } else {
                    "2"
                };
                sim_assert_eq!(have: selected.consumed(&normal.value).constant().cloned(), want: Some(GuardValue::String(expected.to_owned())));
                let field = selected.field(&map, "value");
                sim_assert_eq!(have: selected.consumed(&field).constant().cloned(), want: Some(GuardValue::String(expected.to_owned())));
            }
        }
    }
    Ok(())
}

#[test]
fn overlapping_paths_preserve_both_outcomes_only_in_their_original_domains() -> eyre::Result<()> {
    let initial = State::new(Value::nil());
    let atoms: [_; 2] = std::array::from_fn(|_| {
        initial
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial)
    });
    let exits = atoms
        .iter()
        .enumerate()
        .map(|(index, test)| Exit {
            state: initial.fork(Rc::clone(test), true),
            flow: Flow::Normal(Normal {
                value: Value::string(&index.to_string()),
            }),
        })
        .collect();
    let exits = crate::value::control::reconverge(exits);
    sim_assert_eq!(have: exits.len(), want: 1);
    let exit = exits.first().ok_or_eyre("normal continuation missing")?;
    let Flow::Normal(normal) = &exit.flow else {
        eyre::bail!("continuation is terminal");
    };
    for assignment in 0_u8..4 {
        let mut selected = exit.state.clone();
        selected
            .active
            .extend(atoms.iter().enumerate().map(|(index, test)| DecisionEdge {
                test: Rc::clone(test),
                taken: assignment & (1 << index) != 0,
            }));
        sim_assert_eq!(have: selected.path_predicate() != Predicate::False, want: assignment != 0);
        if assignment == 0 {
            continue;
        }
        let value = selected.consumed(&normal.value);
        let mut pending = vec![&value];
        let mut outcomes = std::collections::BTreeSet::new();
        while let Some(value) = pending.pop() {
            match value.node.as_ref() {
                ValueNode::Leaf(crate::value::Leaf {
                    datum: _,
                    origin: _,
                }) => {
                    outcomes.insert(value.constant().cloned());
                }
                ValueNode::Select {
                    decision: _,
                    yes,
                    no,
                } => {
                    pending.extend([yes, no]);
                }
            }
        }
        let expected = (0..2)
            .filter(|index| assignment & (1 << index) != 0)
            .map(|index| Some(GuardValue::String(index.to_string())))
            .collect();
        sim_assert_eq!(have: outcomes, want: expected);
    }
    Ok(())
}

#[test]
fn template_identity_is_shared_within_one_file_and_fresh_for_the_next() -> eyre::Result<()> {
    use crate::value::session::{FileOccurrence, begin_file};

    let mut initial = State::new(Value::nil());
    let root = initial.heap.map(Remainder::Closed);
    let decision =
        initial
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial);
    let occurrence = FileOccurrence::new(&initial);
    let mut targets = Vec::new();
    for taken in [true, false] {
        let mut branch = initial.fork(Rc::clone(&decision), taken);
        begin_file(
            &mut branch,
            &root,
            Some("chart/templates/z.yaml"),
            &occurrence,
        );
        let template = branch.field(&root, "Template");
        let target = branch
            .map_target(&template)
            .ok_or_eyre("template missing")?;
        targets.push(target.clone());
        let next = FileOccurrence::new(&branch);
        begin_file(&mut branch, &root, Some("chart/templates/a.yaml"), &next);
        let replacement = branch.field(&root, "Template");
        assert!(
            branch
                .map_target(&replacement)
                .is_some_and(|next| next != target)
        );
        let previous_name = branch.field(&template, "Name");
        sim_assert_eq!(have: previous_name.constant().cloned(), want: Some(GuardValue::String("chart/templates/z.yaml".to_owned())));
    }
    sim_assert_eq!(have: targets.first(), want: targets.last());
    Ok(())
}

#[test]
fn independent_helper_regions_and_files_have_one_normal_continuation() {
    let mut defines = DefineIndex::new();
    defines.add_file_source("chart/templates/_helpers.tpl", indoc! {r#"
        {{- define "vpa.name" -}}
        {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
        {{- end }}
        {{- define "vpa.chart" -}}
        {{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
        {{- end }}
        {{- define "vpa.labels" -}}
        helm.sh/chart: {{ include "vpa.chart" . }}
        {{ include "vpa.selectorLabels" . }}
        {{- if .Chart.AppVersion }}
        app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
        {{- end }}
        app.kubernetes.io/managed-by: {{ .Release.Service }}
        {{- if .Values.podLabels }}
        {{ toYaml .Values.podLabels }}
        {{- end }}
        {{- end }}
        {{- define "vpa.selectorLabels" -}}
        app.kubernetes.io/name: {{ include "vpa.name" . }}
        app.kubernetes.io/instance: {{ .Release.Name }}
        {{- end }}
    "#});
    let db = IrAnalysisDb::new(&defines);
    for count in [1, 2, 4, 8] {
        let owned: Vec<_> = (0..count)
            .map(|index| {
                // The first file is the profiler's VPA reproducer; later files add
                // independent branches to check that continuations do not multiply.
                let condition = if index == 0 {
                    "admissionController.enabled".to_owned()
                } else {
                    format!("enabled{index}")
                };
                (
                    formatdoc! {r#"
                {{{{- if .Values.{condition} }}}}
                x:
                  {{{{- include "vpa.labels" . | nindent 4 }}}}
                {{{{- end }}}}
            "#},
                    format!("chart/templates/{index:02}.yaml"),
                )
            })
            .collect();
        let sources: Vec<_> = owned
            .iter()
            .map(|(source, source_path)| SymbolicTemplate {
                source,
                source_path,
            })
            .collect();
        for document in execute_documents(&sources, &db) {
            sim_assert_eq!(have: document.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
        }
    }
}
