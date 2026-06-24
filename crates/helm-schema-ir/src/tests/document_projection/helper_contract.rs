use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::DefineIndex;
use helm_schema_ast::parse_action_expressions;
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::value_path_context::ValuePathContext;

fn empty_fragment_context<'a>(
    defines: &'a DefineIndex,
    analysis_db: &'a IrAnalysisDb,
) -> FragmentEvalContext<'a> {
    FragmentEvalContext::new(defines, analysis_db)
}

#[test]
fn document_type_hints_resolve_template_local_aliases() {
    let exprs = parse_action_expressions("{{ $port | b64enc | quote }}");
    let root_bindings = HashMap::new();
    let template_bindings = HashMap::from([(
        "port".to_string(),
        AbstractValue::choice(vec![
            AbstractValue::ValuesPath("global.service.port".to_string()),
            AbstractValue::ValuesPath("service.port".to_string()),
        ])
        .expect("choice has paths"),
    )]);
    let template_default_paths = HashMap::new();
    let template_output_meta = HashMap::new();
    let defines = DefineIndex::new();
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = ValuePathContext {
        root_bindings: &root_bindings,
        template_bindings: &template_bindings,
        template_default_paths: &template_default_paths,
        template_output_meta: &template_output_meta,
        fragment_context: empty_fragment_context(&defines, &analysis_db),
        current_dot_fragment: None,
        current_dot_binding: None,
    };

    let effects = context.expression_output_effects(&exprs);

    sim_assert_eq!(
        have: effects.schema_type_hints(),
        want: BTreeMap::from([
            (
                "global.service.port".to_string(),
                BTreeSet::from(["string".to_string()])
            ),
            (
                "service.port".to_string(),
                BTreeSet::from(["string".to_string()])
            )
        ])
    );
    sim_assert_eq!(
        have: effects.encoded_paths,
        want: BTreeSet::from([
            "global.service.port".to_string(),
            "service.port".to_string()
        ])
    );
}
