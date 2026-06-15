use helm_schema_ast::{Literal, TemplateExpr};

/// Map Helm/Sprig `typeIs` names to JSON Schema scalar/container names.
pub(crate) fn type_is_schema_type(expr: Option<&TemplateExpr>) -> Option<String> {
    let TemplateExpr::Literal(Literal::String(type_name) | Literal::RawString(type_name)) =
        expr?.deparen()
    else {
        return None;
    };
    let schema_type = match type_name.as_str() {
        "bool" | "boolean" => "boolean",
        "float64" | "number" => "number",
        "int" | "int64" | "integer" => "integer",
        "list" | "slice" | "array" => "array",
        "map" | "dict" | "object" => "object",
        "string" => "string",
        _ => return None,
    };
    Some(schema_type.to_string())
}

pub(crate) fn is_string_transform_function(function: &str) -> bool {
    matches!(
        function,
        "quote"
            | "squote"
            | "b64enc"
            | "b64dec"
            | "toString"
            | "trunc"
            | "trim"
            | "trimAll"
            | "trimPrefix"
            | "trimSuffix"
            | "replace"
    )
}

pub(crate) fn is_provenance_preserving_function(function: &str) -> bool {
    matches!(
        function,
        "toYaml"
            | "fromYaml"
            | "deepCopy"
            | "tpl"
            | "indent"
            | "nindent"
            | "printf"
            | "int"
            | "uniq"
    )
}

pub(crate) fn transform_source_arg<'a>(
    function: &str,
    args: &'a [TemplateExpr],
) -> Option<&'a TemplateExpr> {
    match function {
        function if is_string_transform_function(function) => match function {
            "indent" | "nindent" | "trim" | "trimAll" | "trimPrefix" | "trimSuffix" | "trunc"
            | "replace" => args.last(),
            _ => args.first(),
        },
        function if is_provenance_preserving_function(function) => match function {
            "indent" | "nindent" => args.last(),
            "printf" => None,
            _ => args.first(),
        },
        _ => None,
    }
}

pub(crate) fn pipeline_preserves_current(function: &str) -> bool {
    is_string_transform_function(function) || is_provenance_preserving_function(function)
}
