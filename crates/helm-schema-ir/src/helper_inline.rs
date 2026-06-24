use helm_schema_ast::{DefineIndex, HelmAst, TemplateExpr};

use crate::analysis_db::IrAnalysisDb;
use crate::analysis_db::ParsedHelperBody;
use crate::expr_eval::expr_literal_helper_call_callee;
use crate::resource_identity::ResourceIdentityDetector;

pub(crate) struct ExactHelperInlinePlan<'a> {
    pub(crate) body: ParsedHelperBody<'a>,
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

    let body = analysis_db.parsed_helper_body(name)?;
    let token = format!("define:{name}");
    if inline_stack.iter().any(|entry| entry == &token) {
        return None;
    }

    Some(ExactHelperInlinePlan {
        body,
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
