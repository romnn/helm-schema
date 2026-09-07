use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::eval_effect::EvalResult;
use crate::eval_env::EvalEnv;
use helm_schema_core::{GuardValue, Predicate};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StaticTemplateSource {
    ValuesDefault {
        path: String,
        program: String,
        condition: Predicate,
    },
    Constructed {
        program: String,
        condition: Predicate,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct StaticTemplateProgram {
    pub(crate) source: StaticTemplateSource,
    pub(crate) dot: Option<AbstractValue>,
}

/// Resolves already-evaluated tpl inputs at their invocation boundary.
pub(crate) fn static_template_programs(
    template: &EvalResult,
    dot: Option<&AbstractValue>,
    env: &EvalEnv,
    db: &IrAnalysisDb,
) -> BTreeSet<StaticTemplateProgram> {
    let fields: BTreeMap<String, AbstractValue> = env
        .root_fields
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let dot = dot
        .cloned()
        .map(|value| crate::analysis_db::capture_root_context(value, &fields));
    let mut programs = BTreeSet::new();
    let Some(value) = &template.value else {
        return programs;
    };
    let source_paths = value.fragment_source_paths();
    let source_meta = value.output_meta();
    for path_pattern in &source_paths {
        let mut selections = Vec::new();
        if let Some(dispatch) = &template.scalar_dispatch {
            for (condition, source) in &dispatch.arms {
                if matches!(source, crate::scalar_value::ScalarValue::Identity(path) if path == path_pattern)
                {
                    selections.push(condition.clone());
                }
            }
        }
        if selections.is_empty()
            && let Some(meta) = source_meta
                .get(path_pattern)
                .or_else(|| template.effects.local_output_meta.get(path_pattern))
        {
            selections.extend(
                meta.predicates
                    .iter()
                    .map(|branch| Predicate::all(branch.iter().cloned().collect())),
            );
        }
        let condition = if selections.is_empty() {
            if source_paths.len() != 1 {
                continue;
            }
            Predicate::True
        } else {
            env.predicate_memo.normalize(Predicate::Or(selections))
        };
        for (path, program) in db.chart_default_programs_matching(&path_pattern.encode()) {
            programs.insert(StaticTemplateProgram {
                source: StaticTemplateSource::ValuesDefault {
                    path: path.to_string(),
                    program: program.to_string(),
                    condition: condition.clone(),
                },
                dot: dot.clone(),
            });
        }
    }
    for (condition, program) in literal_programs(template) {
        if !matches!(
            helm_schema_ast::contains_template_action(&program),
            Ok(true)
        ) {
            continue;
        }
        programs.insert(StaticTemplateProgram {
            source: StaticTemplateSource::Constructed { program, condition },
            dot: dot.clone(),
        });
    }
    programs
}

fn literal_programs(template: &EvalResult) -> Vec<(Predicate, String)> {
    let Some(value) = &template.value else {
        return Vec::new();
    };
    match &template.scalar_dispatch {
        Some(dispatch) => dispatch
            .arms
            .iter()
            .filter_map(|(condition, value)| {
                if let crate::scalar_value::ScalarValue::Literal(GuardValue::String(program)) =
                    value
                {
                    Some((condition.clone(), program.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>(),
        None => match value {
            AbstractValue::StringSet(strings) if strings.len() == 1 => strings
                .iter()
                .map(|program| (Predicate::True, program.clone()))
                .collect(),
            _ => Vec::new(),
        },
    }
}
