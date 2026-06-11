use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{HelmAst, parse_action_expressions};
use serde::{Deserialize, Serialize};

use crate::eval_env::EvalEnv;
use crate::expr_eval::{apply_assignment_expr, eval_expr, eval_expr_value};
use crate::walker::is_fragment_expr;
use crate::{Guard, ValueKind, ValueUse};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartFacts {
    pub path_facts: BTreeMap<String, PathFact>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathFact {
    pub has_render_use: bool,
    pub all_render_uses_self_guarded: bool,
    pub has_fragment_render: bool,
    pub descendant_accessed: bool,
    pub has_self_range_guard_render_use: bool,
}

#[must_use]
#[tracing::instrument(skip_all)]
pub fn derive_chart_facts_from_ast(ast: &HelmAst) -> ChartFacts {
    #[derive(Clone, Debug)]
    struct ControlFrame {
        path: String,
        self_guarded: bool,
        is_range: bool,
    }

    #[derive(Default)]
    struct Acc {
        has_render_use: bool,
        all_render_uses_self_guarded: bool,
        has_fragment_render: bool,
        has_self_range_guard_render_use: bool,
    }

    fn update_descendant_paths(descendant_paths: &mut BTreeSet<String>, path: &str) {
        let mut segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            descendant_paths.insert(segments.join("."));
        }
    }

    fn record_render(
        facts: &mut BTreeMap<String, Acc>,
        descendant_paths: &mut BTreeSet<String>,
        paths: BTreeSet<String>,
        active_controls: &[ControlFrame],
        is_fragment: bool,
    ) {
        for path in paths {
            if path.trim().is_empty() {
                continue;
            }
            let self_guarded = active_controls
                .iter()
                .any(|frame| frame.self_guarded && frame.path == path);
            let entry = facts.entry(path.clone()).or_insert_with(|| Acc {
                all_render_uses_self_guarded: true,
                ..Acc::default()
            });
            entry.has_render_use = true;
            entry.has_fragment_render |= is_fragment;
            entry.has_self_range_guard_render_use |= active_controls
                .iter()
                .any(|frame| frame.is_range && frame.path == path);
            entry.all_render_uses_self_guarded &= self_guarded;
            update_descendant_paths(descendant_paths, &path);
        }

        for frame in active_controls {
            if frame.path.trim().is_empty() {
                continue;
            }
            let entry = facts.entry(frame.path.clone()).or_insert_with(|| Acc {
                all_render_uses_self_guarded: true,
                ..Acc::default()
            });
            entry.has_render_use = true;
            entry.has_fragment_render |= is_fragment;
            entry.has_self_range_guard_render_use |= frame.is_range;
            entry.all_render_uses_self_guarded &= frame.self_guarded;
        }
    }

    fn walk(
        node: &HelmAst,
        env: &mut EvalEnv,
        active_controls: &[ControlFrame],
        facts: &mut BTreeMap<String, Acc>,
        descendant_paths: &mut BTreeSet<String>,
    ) {
        match node {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Sequence { items } => {
                let mut scoped_env = env.clone();
                for item in items {
                    walk(
                        item,
                        &mut scoped_env,
                        active_controls,
                        facts,
                        descendant_paths,
                    );
                }
            }
            HelmAst::Pair { key: _, value } => {
                if let Some(value) = value.as_deref() {
                    walk(value, env, active_controls, facts, descendant_paths);
                }
            }
            HelmAst::HelmExpr { text } => {
                let exprs = parse_action_expressions(&format!("{{{{ {text} }}}}"));
                let mut paths = BTreeSet::new();
                for expr in &exprs {
                    if apply_assignment_expr(expr, env) {
                        continue;
                    }
                    let result = eval_expr(expr, env);
                    paths.extend(result.effects.reads);
                }
                if !paths.is_empty() {
                    record_render(
                        facts,
                        descendant_paths,
                        paths,
                        active_controls,
                        is_fragment_expr(text),
                    );
                }
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let mut then_controls = active_controls.to_vec();
                for guard in crate::walker::parse_condition(cond) {
                    match guard {
                        Guard::Truthy { path }
                        | Guard::Eq { path, .. }
                        | Guard::With { path }
                        | Guard::Default { path } => then_controls.push(ControlFrame {
                            path,
                            self_guarded: true,
                            is_range: false,
                        }),
                        Guard::Range { path } => then_controls.push(ControlFrame {
                            path,
                            self_guarded: true,
                            is_range: true,
                        }),
                        Guard::Not { .. } | Guard::Or { .. } | Guard::TypeIs { .. } => {}
                    }
                }
                let entry_env = env.clone();
                let then_env = walk_scoped_branch(
                    &entry_env,
                    then_branch,
                    &then_controls,
                    facts,
                    descendant_paths,
                );
                let else_env = walk_scoped_branch(
                    &entry_env,
                    else_branch,
                    active_controls,
                    facts,
                    descendant_paths,
                );
                *env = EvalEnv::join_branch_outcomes(&entry_env, vec![then_env, else_env]);
            }
            HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                let exprs = parse_action_expressions(&format!("{{{{ {header} }}}}"));
                let entry_env = env.clone();
                let mut body_env = entry_env.clone();
                let mut body_controls = active_controls.to_vec();
                if let Some(binding) = exprs.first().and_then(|expr| eval_expr_value(expr, env)) {
                    let paths = binding.paths();
                    for path in paths {
                        if path.trim().is_empty() {
                            continue;
                        }
                        body_controls.push(ControlFrame {
                            path,
                            self_guarded: true,
                            is_range: false,
                        });
                    }
                    body_env.dot = Some(binding);
                }
                body_env.enter_local_scope();
                for item in body {
                    walk(item, &mut body_env, &body_controls, facts, descendant_paths);
                }
                body_env.exit_local_scope();
                let else_env = walk_scoped_branch(
                    &entry_env,
                    else_branch,
                    active_controls,
                    facts,
                    descendant_paths,
                );
                *env = EvalEnv::join_branch_outcomes(&entry_env, vec![body_env, else_env]);
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            } => {
                let exprs = parse_action_expressions(&format!("{{{{ {header} }}}}"));
                let entry_env = env.clone();
                let mut body_env = entry_env.clone();
                let mut body_controls = active_controls.to_vec();
                if let Some(binding) = exprs.first().and_then(|expr| eval_expr_value(expr, env)) {
                    let paths = binding.paths();
                    for path in paths {
                        if path.trim().is_empty() {
                            continue;
                        }
                        body_controls.push(ControlFrame {
                            path,
                            self_guarded: true,
                            is_range: true,
                        });
                    }
                    body_env.dot = binding.item();
                }
                body_env.enter_local_scope();
                for item in body {
                    walk(item, &mut body_env, &body_controls, facts, descendant_paths);
                }
                body_env.exit_local_scope();
                let else_env = walk_scoped_branch(
                    &entry_env,
                    else_branch,
                    active_controls,
                    facts,
                    descendant_paths,
                );
                *env = EvalEnv::join_branch_outcomes(&entry_env, vec![body_env, else_env]);
            }
            HelmAst::Define { .. }
            | HelmAst::Block { .. }
            | HelmAst::Scalar { .. }
            | HelmAst::HelmComment { .. } => {}
        }
    }

    fn walk_scoped_branch(
        entry_env: &EvalEnv,
        items: &[HelmAst],
        active_controls: &[ControlFrame],
        facts: &mut BTreeMap<String, Acc>,
        descendant_paths: &mut BTreeSet<String>,
    ) -> EvalEnv {
        let mut branch_env = entry_env.clone();
        branch_env.enter_local_scope();
        for item in items {
            walk(
                item,
                &mut branch_env,
                active_controls,
                facts,
                descendant_paths,
            );
        }
        branch_env.exit_local_scope();
        branch_env
    }

    let mut facts = BTreeMap::new();
    let mut descendant_paths = BTreeSet::new();
    let mut env = EvalEnv::default();
    walk(ast, &mut env, &[], &mut facts, &mut descendant_paths);

    ChartFacts {
        path_facts: facts
            .into_iter()
            .map(|(path, acc)| {
                (
                    path.clone(),
                    PathFact {
                        has_render_use: acc.has_render_use,
                        all_render_uses_self_guarded: acc.all_render_uses_self_guarded,
                        has_fragment_render: acc.has_fragment_render,
                        descendant_accessed: descendant_paths.contains(&path),
                        has_self_range_guard_render_use: acc.has_self_range_guard_render_use,
                    },
                )
            })
            .collect(),
    }
}

#[must_use]
pub fn derive_chart_facts(uses: &[ValueUse]) -> ChartFacts {
    #[derive(Default)]
    struct Acc {
        has_render_use: bool,
        all_render_uses_self_guarded: bool,
        has_fragment_render: bool,
        has_self_range_guard_render_use: bool,
    }

    fn use_is_self_guarded(use_: &ValueUse) -> bool {
        if use_.path.0.is_empty() {
            return true;
        }

        use_.guards.iter().any(|guard| match guard {
            Guard::Truthy { path }
            | Guard::Eq { path, .. }
            | Guard::Range { path }
            | Guard::With { path }
            | Guard::Default { path } => path == &use_.source_expr,
            Guard::Not { .. } | Guard::Or { .. } | Guard::TypeIs { .. } => false,
        })
    }

    let mut by_path: BTreeMap<String, Acc> = BTreeMap::new();
    let mut descendant_paths: BTreeSet<String> = BTreeSet::new();

    for use_ in uses {
        if use_.source_expr.trim().is_empty() {
            for guard in &use_.guards {
                for path in guard.value_paths() {
                    if path.trim().is_empty() {
                        continue;
                    }
                    let acc = by_path.entry(path.to_string()).or_insert_with(|| Acc {
                        all_render_uses_self_guarded: true,
                        ..Acc::default()
                    });
                    if !use_.path.0.is_empty() {
                        acc.has_render_use = true;
                        acc.has_fragment_render |= use_.kind == ValueKind::Fragment;
                        acc.has_self_range_guard_render_use |= matches!(guard, Guard::Range { .. });
                    }
                }
            }
            continue;
        }

        let acc = by_path
            .entry(use_.source_expr.clone())
            .or_insert_with(|| Acc {
                all_render_uses_self_guarded: true,
                ..Acc::default()
            });

        if !use_.path.0.is_empty() {
            acc.has_render_use = true;
            acc.has_fragment_render |= use_.kind == ValueKind::Fragment;
            acc.has_self_range_guard_render_use |= use_
                .guards
                .iter()
                .any(|guard| matches!(guard, Guard::Range { path } if path == &use_.source_expr));
            acc.all_render_uses_self_guarded &= use_is_self_guarded(use_);
        }

        for guard in &use_.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() || path == use_.source_expr {
                    continue;
                }
                let acc = by_path.entry(path.to_string()).or_insert_with(|| Acc {
                    all_render_uses_self_guarded: true,
                    ..Acc::default()
                });
                if !use_.path.0.is_empty() {
                    acc.has_render_use = true;
                    acc.has_fragment_render |= use_.kind == ValueKind::Fragment;
                    acc.has_self_range_guard_render_use |= matches!(guard, Guard::Range { .. });
                }
            }
        }

        let mut segments: Vec<&str> = use_
            .source_expr
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            descendant_paths.insert(segments.join("."));
        }
    }

    let path_facts = by_path
        .into_iter()
        .map(|(path, acc)| {
            (
                path.clone(),
                PathFact {
                    has_render_use: acc.has_render_use,
                    all_render_uses_self_guarded: acc.all_render_uses_self_guarded,
                    has_fragment_render: acc.has_fragment_render,
                    descendant_accessed: descendant_paths.contains(&path),
                    has_self_range_guard_render_use: acc.has_self_range_guard_render_use,
                },
            )
        })
        .collect();

    ChartFacts { path_facts }
}

#[cfg(test)]
mod tests {
    use helm_schema_ast::{HelmParser, TreeSitterParser};

    use super::*;

    #[test]
    fn chart_facts_follow_local_assignment_selectors() {
        let src = r#"
{{- if .Values.enabled }}
{{- $image := .Values.image }}
image: {{ $image.repository }}
imagePullPolicy: {{ $image.tag }}
{{- end }}
"#;
        let ast = TreeSitterParser.parse(src).expect("parse template");
        let ast_dump = ast.to_sexpr();

        let facts = derive_chart_facts_from_ast(&ast);

        assert!(
            facts.path_facts.contains_key("image.repository"),
            "local-bound repository selector should be attributed, ast={ast_dump}, got {facts:?}"
        );
        assert!(
            facts.path_facts.contains_key("image.tag"),
            "local-bound tag selector should be attributed, ast={ast_dump}, got {facts:?}"
        );
    }

    #[test]
    fn chart_facts_apply_values_root_path_sets_without_leading_dot() {
        let src = r#"
{{- $root := merge .Values .Values.global }}
image: {{ $root.image.repository }}
"#;
        let ast = TreeSitterParser.parse(src).expect("parse template");

        let facts = derive_chart_facts_from_ast(&ast);

        assert!(
            facts.path_facts.contains_key("image.repository"),
            "values-root path set selector should not produce a leading-dot path, got {facts:?}"
        );
        assert!(
            facts.path_facts.contains_key("global.image.repository"),
            "merged values path set should retain non-root arms, got {facts:?}"
        );
        assert!(
            !facts.path_facts.keys().any(|path| path.starts_with('.')),
            "values paths should never start with a dot, got {facts:?}"
        );
    }

    #[test]
    fn chart_facts_join_assignment_outcomes_after_if_else() {
        let src = r#"
{{- $image := .Values.primaryImage }}
{{- if .Values.useCanary }}
{{- $image = .Values.canaryImage }}
{{- else }}
{{- $image = .Values.stableImage }}
{{- end }}
repository: {{ $image.repository }}
"#;
        let ast = TreeSitterParser.parse(src).expect("parse template");

        let facts = derive_chart_facts_from_ast(&ast);

        assert!(
            facts.path_facts.contains_key("canaryImage.repository"),
            "then-branch assignment should remain visible after branch join, got {facts:?}"
        );
        assert!(
            facts.path_facts.contains_key("stableImage.repository"),
            "else-branch assignment should remain visible after branch join, got {facts:?}"
        );
        assert!(
            !facts.path_facts.contains_key("primaryImage.repository"),
            "fully assigned branches should not keep the pre-branch local value, got {facts:?}"
        );
    }

    #[test]
    fn chart_facts_do_not_leak_branch_local_declarations() {
        let src = r#"
{{- if .Values.enabled }}
{{- $image := .Values.branchImage }}
{{- end }}
repository: {{ $image.repository }}
"#;
        let ast = TreeSitterParser.parse(src).expect("parse template");

        let facts = derive_chart_facts_from_ast(&ast);

        assert!(
            !facts.path_facts.contains_key("branchImage.repository"),
            "block-local declaration should not be visible after the branch, got {facts:?}"
        );
    }

    #[test]
    fn chart_facts_join_assignment_with_untaken_branch_entry_state() {
        let src = r#"
{{- $image := .Values.primaryImage }}
{{- if .Values.useCanary }}
{{- $image = .Values.canaryImage }}
{{- end }}
repository: {{ $image.repository }}
"#;
        let ast = TreeSitterParser.parse(src).expect("parse template");

        let facts = derive_chart_facts_from_ast(&ast);

        assert!(
            facts.path_facts.contains_key("canaryImage.repository"),
            "then-branch assignment should be preserved, got {facts:?}"
        );
        assert!(
            facts.path_facts.contains_key("primaryImage.repository"),
            "implicit else branch should preserve the entry local value, got {facts:?}"
        );
    }

    #[test]
    fn chart_facts_keep_chart_root_distinct_from_values_root() {
        let src = r#"
{{- $ctx := . }}
image: {{ $ctx.Values.image.repository }}
chart: {{ $ctx.Chart.Name }}
{{- with .Values.serviceAccount }}
name: {{ .name }}
{{- end }}
"#;
        let ast = TreeSitterParser.parse(src).expect("parse template");

        let facts = derive_chart_facts_from_ast(&ast);

        assert!(
            facts.path_facts.contains_key("image.repository"),
            "root-context Values selector should be attributed, got {facts:?}"
        );
        assert!(
            facts.path_facts.contains_key("serviceAccount.name"),
            "with-shifted dot selector should still be attributed, got {facts:?}"
        );
        assert!(
            !facts.path_facts.contains_key("Chart.Name"),
            "chart-root fields must not be treated as values paths, got {facts:?}"
        );
        assert!(
            !facts.path_facts.contains_key("Chart"),
            "chart-root fields must not create parent values facts, got {facts:?}"
        );
    }

    #[test]
    fn chart_facts_do_not_treat_unbound_variables_as_root_context() {
        let src = r#"
{{- $image := .Values.image }}
repository: {{ $image.Values.repository }}
"#;
        let ast = TreeSitterParser.parse(src).expect("parse template");

        let facts = derive_chart_facts_from_ast(&ast);

        assert!(
            facts.path_facts.contains_key("image.Values.repository"),
            "selector on a values-bound local should stay relative to that local, got {facts:?}"
        );
        assert!(
            !facts.path_facts.contains_key("repository"),
            "a local variable named before .Values must not be assumed to be chart root, got {facts:?}"
        );
    }
}
