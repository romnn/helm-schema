use helm_schema_ast::{Literal, TemplateExpr};

use crate::YamlPath;
use crate::yaml_syntax::parse_yaml_key;

pub(crate) fn static_yaml_fragment_output_path_from_exprs(
    exprs: &[TemplateExpr],
) -> Option<YamlPath> {
    fn printf_format(expr: &TemplateExpr) -> Option<&str> {
        match expr {
            TemplateExpr::Parenthesized(inner) => printf_format(inner),
            TemplateExpr::Call { function, args } if function == "printf" => {
                let TemplateExpr::Literal(Literal::String(format) | Literal::RawString(format)) =
                    args.first()?
                else {
                    return None;
                };
                Some(format)
            }
            TemplateExpr::Pipeline(stages) => stages.first().and_then(printf_format),
            _ => None,
        }
    }

    let [expr] = exprs else {
        return None;
    };
    let format = printf_format(expr)?;
    let key = parse_yaml_key(format.trim_start())?.into_key();
    Some(YamlPath(vec![key]))
}
