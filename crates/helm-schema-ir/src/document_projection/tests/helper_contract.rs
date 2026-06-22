use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::DefineIndex;
use helm_schema_ast::parse_action_expressions;
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::define_body_cache::DefineBodyCache;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_summary::HelperSummaryCache;
use crate::value_path_context::ValuePathContext;

fn empty_fragment_context<'a>(
    defines: &'a DefineIndex,
    define_bodies: &'a DefineBodyCache,
    helper_summaries: &'a HelperSummaryCache,
) -> FragmentEvalContext<'a> {
    FragmentEvalContext::new(defines, define_bodies, helper_summaries)
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
    let define_bodies = DefineBodyCache::new(&defines);
    let helper_summaries = HelperSummaryCache::new();
    let context = ValuePathContext {
        root_bindings: &root_bindings,
        template_bindings: &template_bindings,
        template_default_paths: &template_default_paths,
        template_output_meta: &template_output_meta,
        fragment_context: empty_fragment_context(&defines, &define_bodies, &helper_summaries),
        current_dot_fragment: None,
        current_dot_binding: None,
    };

    let facts = context.expression_path_facts(&exprs);

    sim_assert_eq!(
        have: facts.type_hints,
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
        have: facts.encoded_output_values,
        want: BTreeSet::from([
            "global.service.port".to_string(),
            "service.port".to_string()
        ])
    );
}
