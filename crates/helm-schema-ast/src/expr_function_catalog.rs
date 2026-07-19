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

/// Returns whether a function is one of Sprig's checksum digests: a typed Go
/// string subject (any other kind aborts rendering) producing derived hex
/// text with no reverse identity. They are NOT string transforms — an
/// `include … | sha256sum` checksum annotation must keep the include's
/// serialized placement semantics rather than a text derivation of it.
pub fn is_checksum_function(function: &str) -> bool {
    matches!(
        function,
        "sha1sum" | "sha256sum" | "sha512sum" | "adler32sum"
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
        // `substr`'s start/end offsets are numeric. The checksum family is
        // unary, so subject-last covers both call and pipeline forms.
        "trunc" | "substr" | "trim" | "lower" | "indent" | "nindent" | "repeat" | "sha1sum"
        | "sha256sum" | "sha512sum" | "adler32sum" => {
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
            // metadata stays unvalidated. Core components parse through
            // `ParseUint(…, 10, 64)`, which overflow-checks the VALUE, not
            // the spelling: leading zeros never overflow, so the bound
            // applies to the significant digits only — up to 20 may fit
            // uint64, while 21+ certainly overflow and abort (still a
            // superset of the accepted language).
            Some((
                argument_count - 1,
                r"^v?(0*[0-9]{1,20})(\.0*[0-9]{1,20})?(\.0*[0-9]{1,20})?(-(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(\.(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?(\+([0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*))?$",
            ))
        }
        // `time.ParseDuration` overflow-checks each term twice: the raw
        // digit scan caps int64 (~19 significant digits) and the unit
        // scaling caps int64 NANOSECONDS, so a term whose significant
        // integer digits exceed the unit's may-fit count certainly aborts
        // (2562047h fits, 8-digit hour terms cannot). Leading zeros carry
        // no value and stay unbounded, as do fractional digits (the
        // fraction scan drops precision instead of overflowing). Multi-term
        // sums may still overflow inside the bounds; the pattern stays a
        // superset of the accepted language.
        "mustDateModify" if argument_count == 2 => Some((
            0,
            r"^[+-]?(0|((0*[0-9]{1,19}(\.[0-9]*)?|\.[0-9]+)ns|(0*[0-9]{1,16}(\.[0-9]*)?|\.[0-9]+)(us|µs|μs)|(0*[0-9]{1,13}(\.[0-9]*)?|\.[0-9]+)ms|(0*[0-9]{1,10}(\.[0-9]*)?|\.[0-9]+)s|(0*[0-9]{1,9}(\.[0-9]*)?|\.[0-9]+)m|(0*[0-9]{1,7}(\.[0-9]*)?|\.[0-9]+)h)+)$",
        )),
        "urlParse" if argument_count == 1 => {
            Some((0, r"^([^\u0000-\u001F\u007F%]|%[0-9A-Fa-f]{2})*$"))
        }
        _ => None,
    }
}

/// Returns the lexical language required of every ITEM of a strict
/// collection operand, keyed by the zero-based operand index.
///
/// Like [`strict_parser_operand_pattern`], the pattern is a conservative
/// superset of every string the runtime parser accepts, so lowering it may
/// miss some invalid inputs but never rejects one the parser accepts.
pub fn strict_collection_item_pattern(function: &str, index: usize) -> Option<&'static str> {
    match (function, index) {
        // genSignedCert/genSelfSignedCert pass every ip-list entry through
        // net.ParseIP and abort rendering on nil. The pattern is the
        // parser's EXACT accepted language (fuzz-differentialed against
        // `net.ParseIP`): dotted-quad IPv4 without leading zeros, and IPv6
        // enumerated per RFC 4291 textual form under Go's rules — 1-4 hex
        // digits per group, at most one `::` expanding at least one zero
        // group, an embedded dotted quad only as the final 4 bytes, and no
        // zone suffix. The v4-embedded arms enumerate the left/right group
        // splits because a regex cannot count the 8-group budget globally.
        ("genSignedCert" | "genSelfSignedCert", 1) => Some(concat!(
            "^(((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\\.){3}(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])",
            "|([0-9A-Fa-f]{1,4}:){7}[0-9A-Fa-f]{1,4}",
            "|([0-9A-Fa-f]{1,4}:){1,7}:",
            "|([0-9A-Fa-f]{1,4}:){1,6}:[0-9A-Fa-f]{1,4}",
            "|([0-9A-Fa-f]{1,4}:){1,5}(:[0-9A-Fa-f]{1,4}){1,2}",
            "|([0-9A-Fa-f]{1,4}:){1,4}(:[0-9A-Fa-f]{1,4}){1,3}",
            "|([0-9A-Fa-f]{1,4}:){1,3}(:[0-9A-Fa-f]{1,4}){1,4}",
            "|([0-9A-Fa-f]{1,4}:){1,2}(:[0-9A-Fa-f]{1,4}){1,5}",
            "|[0-9A-Fa-f]{1,4}:(:[0-9A-Fa-f]{1,4}){1,6}",
            "|:(:[0-9A-Fa-f]{1,4}){1,7}",
            "|::",
            "|([0-9A-Fa-f]{1,4}:){6}((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\\.){3}(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])",
            "|::([0-9A-Fa-f]{1,4}:){0,5}((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\\.){3}(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])",
            "|([0-9A-Fa-f]{1,4}:){1}:([0-9A-Fa-f]{1,4}:){0,4}((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\\.){3}(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])",
            "|([0-9A-Fa-f]{1,4}:){2}:([0-9A-Fa-f]{1,4}:){0,3}((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\\.){3}(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])",
            "|([0-9A-Fa-f]{1,4}:){3}:([0-9A-Fa-f]{1,4}:){0,2}((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\\.){3}(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])",
            "|([0-9A-Fa-f]{1,4}:){4}:([0-9A-Fa-f]{1,4}:){0,1}((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\\.){3}(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])",
            "|([0-9A-Fa-f]{1,4}:){5}:((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\\.){3}(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])",
            ")$",
        )),
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
