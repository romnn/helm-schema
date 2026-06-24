use helm_schema_ast::{DefineIndex, HelmAst, TemplateExpr};

use crate::analysis_db::IrAnalysisDb;
use crate::expr_eval::expr_literal_helper_call_callee;
use crate::resource_identity::ResourceIdentityDetector;

pub(crate) struct ExactHelperInlinePlan<'a> {
    pub(crate) source: &'a str,
    pub(crate) source_path: Option<&'a str>,
    pub(crate) source_offset: usize,
    pub(crate) tree: tree_sitter::Tree,
    pub(crate) token: String,
    pub(crate) arg: Option<TemplateExpr>,
}

pub(crate) fn plan_exact_helper_inline_from_exprs<'a>(
    exprs: &[TemplateExpr],
    defines: &'a DefineIndex,
    analysis_db: &'a IrAnalysisDb,
    inline_stack: &[String],
) -> Option<ExactHelperInlinePlan<'a>> {
    if exprs.len() != 1 {
        return None;
    }

    let TemplateExpr::Call { args, .. } = &exprs[0] else {
        return None;
    };
    let name = expr_literal_helper_call_callee(&exprs[0])?;
    define_body_resource(defines, name)?;

    let source = analysis_db.define_source(name)?;
    let source_path = analysis_db.define_source_path(name);
    let source_offset = analysis_db.define_body_offset(name).unwrap_or(0);
    let token = format!("define:{name}");
    if inline_stack.iter().any(|entry| entry == &token) {
        return None;
    }
    let tree = analysis_db.define_tree(name)?;

    Some(ExactHelperInlinePlan {
        source,
        source_path,
        source_offset,
        tree,
        token,
        arg: args.get(1).cloned(),
    })
}

fn define_body_resource(defines: &DefineIndex, name: &str) -> Option<crate::ResourceRef> {
    let body = defines.get(name)?;
    let ast = HelmAst::Document {
        items: body.to_vec(),
    };
    ResourceIdentityDetector::new(defines).detect(&ast)
}
