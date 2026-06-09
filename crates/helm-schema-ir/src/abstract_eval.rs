use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{HelmAst, Literal, TemplateExpr, parse_action_expressions};
use serde::{Deserialize, Serialize};

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
pub fn derive_chart_facts_from_ast(ast: &HelmAst) -> ChartFacts {
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Binding {
        ValuesPath(String),
        ValuesRoot,
        RootContext,
        PathSet(BTreeSet<String>),
        Choice(BTreeSet<Binding>),
        Dict(BTreeMap<String, Binding>),
        List(Vec<Binding>),
    }

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

    #[derive(Clone, Default)]
    struct Env {
        dot: Option<Binding>,
        locals: HashMap<String, Binding>,
    }

    fn binding_paths(binding: &Binding) -> BTreeSet<String> {
        match binding {
            Binding::ValuesPath(path) => [path.clone()].into_iter().collect(),
            Binding::ValuesRoot => [String::new()].into_iter().collect(),
            Binding::RootContext => BTreeSet::new(),
            Binding::PathSet(paths) => paths.clone(),
            Binding::Choice(choices) => choices.iter().flat_map(binding_paths).collect(),
            Binding::Dict(map) => map.values().flat_map(binding_paths).collect(),
            Binding::List(items) => items.iter().flat_map(binding_paths).collect(),
        }
    }

    fn choice(bindings: Vec<Binding>) -> Option<Binding> {
        let mut flat = BTreeSet::new();
        for binding in bindings {
            match binding {
                Binding::Choice(inner) => flat.extend(inner),
                other => {
                    flat.insert(other);
                }
            }
        }
        match flat.len() {
            0 => None,
            1 => flat.into_iter().next(),
            _ => Some(Binding::Choice(flat)),
        }
    }

    fn apply(binding: &Binding, rest: &[String]) -> Option<Binding> {
        match binding {
            Binding::ValuesPath(prefix) => {
                if rest.is_empty() {
                    Some(Binding::ValuesPath(prefix.clone()))
                } else if prefix.is_empty() {
                    Some(Binding::ValuesPath(rest.join(".")))
                } else {
                    Some(Binding::ValuesPath(format!("{prefix}.{}", rest.join("."))))
                }
            }
            Binding::ValuesRoot => {
                if rest.is_empty() {
                    Some(Binding::ValuesRoot)
                } else {
                    Some(Binding::ValuesPath(rest.join(".")))
                }
            }
            Binding::RootContext => {
                if rest.first().is_some_and(|segment| segment == "Values") {
                    if rest.len() == 1 {
                        Some(Binding::ValuesRoot)
                    } else {
                        Some(Binding::ValuesPath(rest[1..].join(".")))
                    }
                } else {
                    None
                }
            }
            Binding::PathSet(paths) => {
                let appended = paths
                    .iter()
                    .map(|path| {
                        if rest.is_empty() {
                            path.clone()
                        } else if path.is_empty() {
                            rest.join(".")
                        } else {
                            format!("{path}.{}", rest.join("."))
                        }
                    })
                    .collect();
                Some(Binding::PathSet(appended))
            }
            Binding::Choice(choices) => {
                let mut out = Vec::new();
                for binding in choices {
                    if let Some(bound) = apply(binding, rest) {
                        out.push(bound);
                    }
                }
                choice(out)
            }
            Binding::Dict(map) if rest.len() == 1 => map.get(&rest[0]).cloned(),
            Binding::List(items) if rest.len() == 1 => {
                let index = rest[0].parse::<usize>().ok()?;
                items.get(index).cloned()
            }
            Binding::Dict(_) | Binding::List(_) => None,
        }
    }

    fn item_binding(binding: &Binding) -> Option<Binding> {
        match binding {
            Binding::ValuesPath(path) => Some(Binding::ValuesPath(format!("{path}.*"))),
            Binding::ValuesRoot => Some(Binding::ValuesPath("*".to_string())),
            Binding::RootContext => None,
            Binding::PathSet(paths) => Some(Binding::PathSet(
                paths
                    .iter()
                    .map(|path| {
                        if path.is_empty() {
                            "*".to_string()
                        } else {
                            format!("{path}.*")
                        }
                    })
                    .collect(),
            )),
            Binding::Choice(choices) => {
                let mut out = Vec::new();
                for choice_binding in choices {
                    if let Some(bound) = item_binding(choice_binding) {
                        out.push(bound);
                    }
                }
                choice(out)
            }
            Binding::List(items) => choice(items.clone()),
            Binding::Dict(map) => choice(map.values().cloned().collect()),
        }
    }

    fn eval_expr(expr: &TemplateExpr, env: &Env) -> Option<Binding> {
        match expr {
            TemplateExpr::Parenthesized(inner) => eval_expr(inner, env),
            TemplateExpr::Field(path)
                if path.first().is_some_and(|segment| segment == "Values") =>
            {
                if path.len() == 1 {
                    Some(Binding::ValuesRoot)
                } else {
                    Some(Binding::ValuesPath(path[1..].join(".")))
                }
            }
            TemplateExpr::Field(path) if path.is_empty() => {
                env.dot.clone().or(Some(Binding::RootContext))
            }
            TemplateExpr::Field(path) => env.dot.as_ref().and_then(|binding| apply(binding, path)),
            TemplateExpr::Selector { operand, path }
                if matches!(operand.as_ref(), TemplateExpr::Variable(var) if var.is_empty())
                    && path.first().is_some_and(|segment| segment == "Values") =>
            {
                if path.len() == 1 {
                    Some(Binding::ValuesRoot)
                } else {
                    Some(Binding::ValuesPath(path[1..].join(".")))
                }
            }
            TemplateExpr::Variable(var) if var.is_empty() => Some(Binding::RootContext),
            TemplateExpr::Variable(var) if !var.is_empty() => env.locals.get(var).cloned(),
            TemplateExpr::Selector { operand, path } => {
                let base = eval_expr(operand, env)?;
                apply(&base, path)
            }
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let mut options = Vec::new();
                if let Some(primary) = eval_expr(&args[1], env) {
                    options.push(primary);
                }
                if let Some(fallback) = eval_expr(&args[0], env) {
                    options.push(fallback);
                }
                choice(options)
            }
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut map = BTreeMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                        &args[index]
                    else {
                        index += 1;
                        continue;
                    };
                    if let Some(value) = eval_expr(&args[index + 1], env) {
                        map.insert(key.clone(), value);
                    }
                    index += 2;
                }
                Some(Binding::Dict(map))
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "list" | "tuple") =>
            {
                let mut items = Vec::new();
                for arg in args {
                    if let Some(value) = eval_expr(arg, env) {
                        items.push(value);
                    }
                }
                Some(Binding::List(items))
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "merge" | "mergeOverwrite") =>
            {
                let mut paths = BTreeSet::new();
                for arg in args {
                    if let Some(value) = eval_expr(arg, env) {
                        paths.extend(binding_paths(&value));
                    }
                }
                Some(Binding::PathSet(paths))
            }
            TemplateExpr::Call { function, args } if function == "index" => {
                let base = eval_expr(args.first()?, env)?;
                match base {
                    Binding::Dict(map) if args.len() == 2 => {
                        let key = match &args[1] {
                            TemplateExpr::Literal(
                                Literal::String(value) | Literal::RawString(value),
                            ) => value.clone(),
                            _ => return None,
                        };
                        map.get(&key).cloned()
                    }
                    Binding::List(items) if args.len() == 2 => {
                        let index = match &args[1] {
                            TemplateExpr::Literal(Literal::Int(value)) => {
                                usize::try_from(*value).ok()?
                            }
                            _ => return None,
                        };
                        items.get(index).cloned()
                    }
                    binding => {
                        let mut segments = Vec::new();
                        for arg in &args[1..] {
                            let TemplateExpr::Literal(
                                Literal::String(value) | Literal::RawString(value),
                            ) = arg
                            else {
                                return None;
                            };
                            segments.push(value.clone());
                        }
                        apply(&binding, &segments)
                    }
                }
            }
            TemplateExpr::Call { function, args }
                if matches!(
                    function.as_str(),
                    "toYaml"
                        | "fromYaml"
                        | "quote"
                        | "indent"
                        | "nindent"
                        | "tpl"
                        | "printf"
                        | "trimPrefix"
                        | "trimSuffix"
                        | "trunc"
                        | "replace"
                        | "int"
                ) =>
            {
                args.iter().find_map(|arg| eval_expr(arg, env))
            }
            TemplateExpr::Pipeline(stages) => {
                let Some(first_stage) = stages.first() else {
                    return None;
                };
                let mut current = eval_expr(first_stage, env);
                for stage in &stages[1..] {
                    let TemplateExpr::Call { function, args } = stage else {
                        continue;
                    };
                    current = match function.as_str() {
                        "default" => {
                            let mut options = Vec::new();
                            if let Some(current) = current {
                                options.push(current);
                            }
                            for arg in args {
                                if let Some(binding) = eval_expr(arg, env) {
                                    options.push(binding);
                                }
                            }
                            choice(options)
                        }
                        "merge" | "mergeOverwrite" => {
                            let mut paths = current.as_ref().map(binding_paths).unwrap_or_default();
                            for arg in args {
                                if let Some(binding) = eval_expr(arg, env) {
                                    paths.extend(binding_paths(&binding));
                                }
                            }
                            Some(Binding::PathSet(paths))
                        }
                        "toYaml" | "fromYaml" | "quote" | "indent" | "nindent" | "tpl"
                        | "printf" | "trimPrefix" | "trimSuffix" | "trunc" | "replace" | "int" => {
                            current
                        }
                        _ => None,
                    };
                }
                current
            }
            TemplateExpr::Literal(_)
            | TemplateExpr::Variable(_)
            | TemplateExpr::Call { .. }
            | TemplateExpr::Unknown(_)
            | TemplateExpr::VariableDefinition { .. }
            | TemplateExpr::Assignment { .. } => None,
        }
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

    fn update_env_from_expr(expr: &TemplateExpr, env: &mut Env) -> bool {
        match expr {
            TemplateExpr::VariableDefinition { name, value }
            | TemplateExpr::Assignment { name, value } => {
                let name = name.trim_start_matches('$');
                if let Some(binding) = eval_expr(value, env) {
                    env.locals.insert(name.to_string(), binding);
                } else {
                    env.locals.remove(name);
                }
                true
            }
            _ => false,
        }
    }

    fn walk(
        node: &HelmAst,
        env: &mut Env,
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
                    if update_env_from_expr(expr, env) {
                        continue;
                    }
                    if let Some(binding) = eval_expr(expr, env) {
                        paths.extend(binding_paths(&binding));
                    }
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
                {
                    let mut branch_env = env.clone();
                    for item in then_branch {
                        walk(
                            item,
                            &mut branch_env,
                            &then_controls,
                            facts,
                            descendant_paths,
                        );
                    }
                }
                {
                    let mut branch_env = env.clone();
                    for item in else_branch {
                        walk(
                            item,
                            &mut branch_env,
                            active_controls,
                            facts,
                            descendant_paths,
                        );
                    }
                }
            }
            HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                let exprs = parse_action_expressions(&format!("{{{{ {header} }}}}"));
                let mut body_env = env.clone();
                let mut body_controls = active_controls.to_vec();
                if let Some(binding) = exprs.first().and_then(|expr| eval_expr(expr, env)) {
                    let paths = binding_paths(&binding);
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
                for item in body {
                    walk(item, &mut body_env, &body_controls, facts, descendant_paths);
                }
                {
                    let mut branch_env = env.clone();
                    for item in else_branch {
                        walk(
                            item,
                            &mut branch_env,
                            active_controls,
                            facts,
                            descendant_paths,
                        );
                    }
                }
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            } => {
                let exprs = parse_action_expressions(&format!("{{{{ {header} }}}}"));
                let mut body_env = env.clone();
                let mut body_controls = active_controls.to_vec();
                if let Some(binding) = exprs.first().and_then(|expr| eval_expr(expr, env)) {
                    let paths = binding_paths(&binding);
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
                    body_env.dot = item_binding(&binding);
                }
                for item in body {
                    walk(item, &mut body_env, &body_controls, facts, descendant_paths);
                }
                {
                    let mut branch_env = env.clone();
                    for item in else_branch {
                        walk(
                            item,
                            &mut branch_env,
                            active_controls,
                            facts,
                            descendant_paths,
                        );
                    }
                }
            }
            HelmAst::Define { .. }
            | HelmAst::Block { .. }
            | HelmAst::Scalar { .. }
            | HelmAst::HelmComment { .. } => {}
        }
    }

    let mut facts = BTreeMap::new();
    let mut descendant_paths = BTreeSet::new();
    let mut env = Env::default();
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
        }
        acc.all_render_uses_self_guarded &= use_is_self_guarded(use_);

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
