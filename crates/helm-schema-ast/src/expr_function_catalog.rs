use crate::{Literal, TemplateExpr};

/// Map Helm/Sprig `typeIs` names to JSON Schema scalar/container names.
pub fn type_is_schema_type(expr: Option<&TemplateExpr>) -> Option<String> {
    let TemplateExpr::Literal(Literal::String(type_name) | Literal::RawString(type_name)) =
        expr?.deparen()
    else {
        return None;
    };
    go_type_schema_type(type_name).map(str::to_string)
}

/// Map a Go type or reflect-kind name, as compared by `typeIs`, `kindIs`,
/// or an `eq (typeOf …)`/`eq (kindOf …)` test, to a JSON Schema type name.
/// Covers both the reflect-kind spellings (`slice`, `map`) and the exact
/// `typeOf` spellings of untyped YAML containers (`[]interface {}`,
/// `map[string]interface {}`).
pub fn go_type_schema_type(type_name: &str) -> Option<&'static str> {
    Some(match type_name {
        "bool" | "boolean" => "boolean",
        "float64" | "number" => "number",
        "int" | "int64" | "integer" => "integer",
        "list" | "slice" | "array" | "[]interface {}" => "array",
        "map" | "dict" | "object" | "map[string]interface {}" => "object",
        "string" => "string",
        _ => return None,
    })
}

/// The Go type spellings `typeOf`/`kindOf` can print for a chart value of
/// one JSON Schema kind. Integer values list both numeric spellings because
/// provenance decides the dynamic type: file-loaded values decode through
/// JSON as `float64`, while `--set` values parse as `int64`.
pub fn go_type_descriptor_spellings(schema_type: &str) -> &'static [&'static str] {
    match schema_type {
        "boolean" => &["bool"],
        "integer" => &["float64", "int64"],
        "number" => &["float64"],
        "string" => &["string"],
        "array" => &["[]interface {}", "slice"],
        "object" => &["map[string]interface {}", "map"],
        _ => &[],
    }
}

/// The subject of a Go type-descriptor call: `typeOf x`, `kindOf x`, or the
/// equivalent `printf "%T" x` (`typeOf` is exactly `fmt.Sprintf("%T", …)`;
/// signoz binds `printf "%T" $val` and dispatches on the result).
pub fn type_descriptor_call_subject<'a>(
    function: &str,
    args: &'a [TemplateExpr],
) -> Option<&'a TemplateExpr> {
    match function {
        "typeOf" | "kindOf" if args.len() == 1 => Some(&args[0]),
        "printf" if args.len() == 2 => match args[0].deparen() {
            TemplateExpr::Literal(Literal::String(format) | Literal::RawString(format))
                if format == "%T" =>
            {
                Some(&args[1])
            }
            _ => None,
        },
        _ => None,
    }
}

pub fn is_string_transform_function(function: &str) -> bool {
    matches!(
        function,
        "quote"
            | "squote"
            | "b64enc"
            | "b64dec"
            | "urlquery"
            | "toString"
            | "lower"
            | "indent"
            | "nindent"
            | "trunc"
            | "substr"
            | "trim"
            | "trimAll"
            | "trimPrefix"
            | "trimSuffix"
            | "replace"
            | "repeat"
            | "regexReplaceAll"
            | "mustRegexReplaceAll"
            | "regexReplaceAllLiteral"
            | "mustRegexReplaceAllLiteral"
            | "htpasswd"
    )
}

/// Returns the argument positions that a Helm/Sprig call requires to be Go strings.
///
/// `argument_count` includes a pipeline input, which Go templates append as the final
/// argument. An empty result means the function has no catalogued string operands.
pub fn string_operand_indices(function: &str, argument_count: usize) -> Vec<usize> {
    if argument_count == 0 {
        return Vec::new();
    }

    match function {
        // These functions accept only string arguments. Total stringifiers are included so
        // callers can share the position catalog while deciding whether the input is strict.
        "quote"
        | "squote"
        | "b64enc"
        | "b64dec"
        | "toString"
        | "trimAll"
        | "trimPrefix"
        | "trimSuffix"
        | "replace"
        | "regexReplaceAll"
        | "mustRegexReplaceAll"
        | "regexReplaceAllLiteral"
        | "mustRegexReplaceAllLiteral"
        | "regexMatch"
        | "mustRegexMatch"
        | "contains"
        | "hasPrefix"
        | "hasSuffix"
        | "semverCompare"
        | "urlParse"
        | "splitList"
        | "split"
        // Both the user and the password are bcrypt inputs; a non-string
        // aborts rendering with `expected string`.
        | "htpasswd" => (0..argument_count).collect(),
        // The duration is the first argument; the second is a time value.
        "mustDateModify" if argument_count >= 2 => vec![0],
        // The string subject is final; `trunc`'s preceding width and
        // `substr`'s start/end offsets are numeric.
        "trunc" | "substr" | "trim" | "lower" | "indent" | "nindent" | "repeat" => {
            vec![argument_count - 1]
        }
        // `splitn separator count subject` has a non-string middle argument.
        "splitn" if argument_count >= 3 => vec![0, argument_count - 1],
        // `regexSplit expression subject count` has a non-string final argument.
        "regexSplit" if argument_count >= 3 => vec![0, 1],
        _ => Vec::new(),
    }
}

/// Returns the lexical language required by a strict string parser operand.
///
/// The pattern is a conservative superset of every string accepted by the
/// runtime parser, so lowering it may miss some invalid inputs but never
/// rejects an input solely because the parser accepts a wider spelling.
pub fn strict_parser_operand_pattern(
    function: &str,
    argument_count: usize,
) -> Option<(usize, &'static str)> {
    match function {
        "semverCompare" if argument_count == 2 => {
            // Masterminds semver's coercing parser keeps the CORE segments
            // loose (leading zeros parse through `ParseUint`), but its
            // prerelease validation rejects a NUMERIC identifier with a
            // leading zero (`3.1.0-01` aborts while `3.1.0-rc.1` renders),
            // so the prerelease alternatives spell that rule out. Build
            // metadata stays unvalidated.
            Some((
                argument_count - 1,
                r"^v?([0-9]+)(\.[0-9]+)?(\.[0-9]+)?(-(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(\.(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?(\+([0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*))?$",
            ))
        }
        "mustDateModify" if argument_count == 2 => Some((
            0,
            r"^[+-]?(0|(([0-9]+(\.[0-9]*)?|\.[0-9]+)(ns|us|µs|μs|ms|s|m|h))+)$",
        )),
        "urlParse" if argument_count == 1 => {
            Some((0, r"^([^\u0000-\u001F\u007F%]|%[0-9A-Fa-f]{2})*$"))
        }
        _ => None,
    }
}

/// Returns whether a function stringifies ANY input. These call Sprig's
/// `strval` (fallback `fmt.Sprintf("%v")`) or format the interface directly,
/// so maps, lists, and nil all render as text: the function imposes no
/// input-type constraint, and the sink observes the rendered string, never
/// the input shape. (`join` shares this contract via `strslice` but has its
/// own eval arms.)
pub fn is_total_stringification_function(function: &str) -> bool {
    matches!(function, "quote" | "squote" | "toString" | "urlquery")
}

/// Returns whether a function is a total numeric cast: Sprig's `int`,
/// `int64`, and `float64` convert through `cast.ToXxx`, which coerces ANY
/// input (junk becomes zero) instead of failing. Like `toString`, they
/// erase input shape; their output is derived (numeric) content.
pub fn is_total_numeric_cast_function(function: &str) -> bool {
    matches!(function, "int" | "int64" | "float64")
}

/// Returns whether a function is a COERCING Sprig arithmetic operation:
/// its numeric operands pass through `cast.ToInt64`/`cast.ToFloat64`
/// before the computation, so any scalar (a numeric string, or junk that
/// coerces to zero) is accepted and the result is derived numeric content.
/// The raw operand shape is therefore not constrained by the arithmetic —
/// only its evaluated value participates. Division and modulo are
/// intentionally EXCLUDED: a zero denominator is a genuine precondition,
/// so they must not be widened by this analogy.
pub fn is_coercing_arithmetic_function(function: &str) -> bool {
    matches!(
        function,
        "add"
            | "add1"
            | "sub"
            | "mul"
            | "max"
            | "min"
            | "floor"
            | "ceil"
            | "round"
            | "addf"
            | "add1f"
            | "subf"
            | "mulf"
            | "maxf"
            | "minf"
    )
}

/// Returns whether a function consumes a Go `string` subject as its LAST
/// parameter (Sprig's subject-last convention) and fails template evaluation
/// for non-string values, without transforming template output flow itself.
pub fn is_string_predicate_function(function: &str) -> bool {
    matches!(
        function,
        "regexMatch"
            | "mustRegexMatch"
            | "contains"
            | "hasPrefix"
            | "hasSuffix"
            | "semverCompare"
            | "mustDateModify"
            | "urlParse"
    )
}

/// Returns whether a function consumes a Go `string` subject as its LAST
/// parameter but produces NON-STRING output (a list, a boolean): the input
/// contract is real, while the output carries no string provenance.
pub fn is_string_splitting_function(function: &str) -> bool {
    matches!(function, "splitList" | "split" | "splitn" | "regexSplit")
}

pub fn is_provenance_preserving_function(function: &str) -> bool {
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

pub fn is_merge_function(function: &str) -> bool {
    matches!(
        function,
        "merge" | "mustMerge" | "mergeOverwrite" | "mustMergeOverwrite"
    )
}
