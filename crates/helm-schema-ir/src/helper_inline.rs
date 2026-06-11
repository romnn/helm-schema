use helm_schema_ast::{DefineIndex, HelmAst, Literal, TemplateExpr};

use crate::define_body_cache::DefineBodyCache;
use crate::resource_detector::AstResourceDetector;
use crate::template_expr_cache::parse_expr_text;

pub(crate) struct ExactHelperInlinePlan<'a> {
    pub(crate) source: &'a str,
    pub(crate) tree: tree_sitter::Tree,
    pub(crate) token: String,
    pub(crate) arg: Option<TemplateExpr>,
}

pub(crate) fn plan_exact_helper_inline<'a>(
    text: &str,
    defines: &'a DefineIndex,
    define_bodies: &'a DefineBodyCache,
    inline_stack: &[String],
) -> Option<ExactHelperInlinePlan<'a>> {
    let exprs = parse_expr_text(text);
    if exprs.len() != 1 {
        return None;
    }

    let TemplateExpr::Call { function, args } = &exprs[0] else {
        return None;
    };
    if !matches!(function.as_str(), "include" | "template") {
        return None;
    }
    let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
        return None;
    };
    define_body_resource(defines, name)?;

    let source = define_bodies.source(name)?;
    let token = format!("define:{name}");
    if inline_stack.iter().any(|entry| entry == &token) {
        return None;
    }
    let tree = define_bodies.tree(name)?;

    Some(ExactHelperInlinePlan {
        source,
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
    AstResourceDetector::new(defines).detect(&ast)
}
