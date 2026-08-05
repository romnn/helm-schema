use helm_schema_ast::{Literal, TemplateExpr};

/// Reports whether a function is Helm's `.Files.Get` method, including a receiver-qualified call.
pub(crate) fn is_files_get(function: &str) -> bool {
    function == "Files.Get" || function.ends_with(".Files.Get")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StringOperands {
    None,
    All,
    First,
    Last,
    FirstAndLast,
    FirstTwo,
}

/// Classifies whether a nil operand aborts, including the direct-access distinction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NilBehavior {
    Tolerates,
    AlwaysAborts,
    DirectAccessAborts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputSemantics {
    Opaque,
    StringTransform,
    TotalStringification,
    Checksum,
    TotalNumericCast,
    CoercingArithmetic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CollectionShape {
    None,
    Merge,
    StringSplit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProvenanceBehavior {
    Discard,
    Preserve,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PredicateSemantics {
    None,
    String,
    TypeDescriptor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParserSemantics {
    None,
    Semver,
    Duration,
    Url,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectionItemSemantics {
    None,
    CertificateIpList,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FunctionSemantics {
    known: bool,
    pub(crate) string_operands: StringOperands,
    pub(crate) nil_behavior: NilBehavior,
    pub(crate) output: OutputSemantics,
    pub(crate) collection: CollectionShape,
    pub(crate) provenance: ProvenanceBehavior,
    pub(crate) predicate: PredicateSemantics,
    parser: ParserSemantics,
    collection_items: CollectionItemSemantics,
}

const UNKNOWN: FunctionSemantics = FunctionSemantics {
    known: false,
    string_operands: StringOperands::None,
    nil_behavior: NilBehavior::Tolerates,
    output: OutputSemantics::Opaque,
    collection: CollectionShape::None,
    provenance: ProvenanceBehavior::Discard,
    predicate: PredicateSemantics::None,
    parser: ParserSemantics::None,
    collection_items: CollectionItemSemantics::None,
};

const KNOWN: FunctionSemantics = FunctionSemantics {
    known: true,
    ..UNKNOWN
};

impl FunctionSemantics {
    const fn with_strings(self, string_operands: StringOperands) -> Self {
        Self {
            string_operands,
            ..self
        }
    }

    const fn with_nil(self, nil_behavior: NilBehavior) -> Self {
        Self {
            nil_behavior,
            ..self
        }
    }

    const fn with_output(self, output: OutputSemantics) -> Self {
        Self { output, ..self }
    }

    const fn with_collection(self, collection: CollectionShape) -> Self {
        Self { collection, ..self }
    }

    const fn with_provenance(self, provenance: ProvenanceBehavior) -> Self {
        Self { provenance, ..self }
    }

    const fn with_predicate(self, predicate: PredicateSemantics) -> Self {
        Self { predicate, ..self }
    }

    const fn with_parser(self, parser: ParserSemantics) -> Self {
        Self { parser, ..self }
    }

    const fn with_collection_items(self, collection_items: CollectionItemSemantics) -> Self {
        Self {
            collection_items,
            ..self
        }
    }
}

/// Returns the analyzer-owned semantics for one Helm or Sprig function.
///
/// Each recognized name occurs in exactly one match arm. Facets that overlap
/// therefore describe one intentional row instead of independent classifiers
/// that can drift apart.
#[must_use]
pub(crate) fn function_semantics(function: &str) -> FunctionSemantics {
    use CollectionShape::{Merge, StringSplit};
    use NilBehavior::{AlwaysAborts, DirectAccessAborts};
    use OutputSemantics::{
        Checksum, CoercingArithmetic, StringTransform, TotalNumericCast, TotalStringification,
    };
    use PredicateSemantics::{String as StringPredicate, TypeDescriptor};
    use ProvenanceBehavior::Preserve;
    use StringOperands::{All, First, FirstAndLast, FirstTwo, Last};

    match function {
        // Total stringifiers are included so consumers can share the operand-position catalog
        // while deciding separately whether the input is strict.
        "quote" | "squote" | "toString" | "urlquery" => {
            KNOWN.with_strings(All).with_output(TotalStringification)
        }
        "b64enc"
        | "b64dec"
        | "trimAll"
        | "trimPrefix"
        | "trimSuffix"
        | "replace"
        | "regexReplaceAll"
        | "mustRegexReplaceAll"
        | "regexReplaceAllLiteral"
        | "mustRegexReplaceAllLiteral"
        | "htpasswd" => KNOWN.with_strings(All).with_output(StringTransform),
        "lower" | "upper" | "trunc" | "substr" | "trim" | "repeat" => {
            KNOWN.with_strings(Last).with_output(StringTransform)
        }
        "indent" | "nindent" => KNOWN
            .with_strings(Last)
            .with_output(StringTransform)
            .with_provenance(Preserve),
        "sha1sum" | "sha256sum" | "sha512sum" | "adler32sum" => {
            KNOWN.with_strings(Last).with_output(Checksum)
        }
        "regexMatch" | "mustRegexMatch" | "contains" | "hasPrefix" | "hasSuffix" => {
            KNOWN.with_strings(All).with_predicate(StringPredicate)
        }
        "semverCompare" => KNOWN
            .with_strings(All)
            .with_predicate(StringPredicate)
            .with_parser(ParserSemantics::Semver),
        "mustDateModify" => KNOWN
            .with_strings(First)
            .with_predicate(StringPredicate)
            .with_parser(ParserSemantics::Duration),
        "urlParse" => KNOWN
            .with_strings(All)
            .with_predicate(StringPredicate)
            .with_parser(ParserSemantics::Url),
        "splitList" | "split" => KNOWN.with_strings(All).with_collection(StringSplit),
        "splitn" => KNOWN
            .with_strings(FirstAndLast)
            .with_collection(StringSplit),
        "regexSplit" => KNOWN.with_strings(FirstTwo).with_collection(StringSplit),
        "lookup" => KNOWN.with_strings(All),
        "int" => KNOWN
            .with_output(TotalNumericCast)
            .with_provenance(Preserve),
        "int64" | "float64" => KNOWN.with_output(TotalNumericCast),
        // Division and modulo stay excluded because a zero denominator is a genuine
        // precondition and must not be widened by analogy with total coercing arithmetic.
        "add" | "add1" | "sub" | "mul" | "max" | "min" | "floor" | "ceil" | "round" | "addf"
        | "add1f" | "subf" | "mulf" | "maxf" | "minf" => KNOWN.with_output(CoercingArithmetic),
        "merge" | "mustMerge" | "mergeOverwrite" | "mustMergeOverwrite" => {
            KNOWN.with_nil(DirectAccessAborts).with_collection(Merge)
        }
        "first" | "last" | "initial" | "rest" | "compact" | "reverse" | "append" | "push"
        | "prepend" | "concat" | "slice" | "mustSlice" | "index" | "len" | "ternary" | "unset" => {
            KNOWN.with_nil(AlwaysAborts)
        }
        "uniq" | "mustUniq" | "deepCopy" | "mustDeepCopy" => {
            KNOWN.with_nil(AlwaysAborts).with_provenance(Preserve)
        }
        "hasKey" | "pick" | "omit" | "keys" | "values" | "pluck" | "get" => {
            KNOWN.with_nil(DirectAccessAborts)
        }
        "toYaml" | "fromYaml" | "tpl" => KNOWN.with_provenance(Preserve),
        "printf" => KNOWN
            .with_provenance(Preserve)
            .with_predicate(TypeDescriptor),
        "typeOf" | "kindOf" => KNOWN.with_predicate(TypeDescriptor),
        "genSignedCert" | "genSelfSignedCert" => {
            KNOWN.with_collection_items(CollectionItemSemantics::CertificateIpList)
        }
        _ => UNKNOWN,
    }
}

impl FunctionSemantics {
    #[must_use]
    pub(crate) const fn is_known(self) -> bool {
        self.known
    }

    #[must_use]
    pub(crate) const fn is_string_transform(self) -> bool {
        matches!(
            self.output,
            OutputSemantics::StringTransform | OutputSemantics::TotalStringification
        )
    }

    #[must_use]
    pub(crate) const fn is_total_stringification(self) -> bool {
        matches!(self.output, OutputSemantics::TotalStringification)
    }

    /// Reports whether a nil operand aborts at a strict-kind position.
    ///
    /// `direct_access` says how nil reaches the parameter, which decides one whole class of
    /// these functions.
    /// A missing key read as a field (`.Values.x`, or a `range` member variable, which comes
    /// straight from `MapIndex`) arrives as a valid `interface{}` holding nil.
    /// Anything that passed through a pipeline stage or a `:=` binding arrives invalid instead.
    /// Go stores both through a step that unwraps the interface, and `validateType` then
    /// substitutes the parameter's zero value whenever the declared type can be nil.
    ///
    /// Three mechanisms decide the verdict:
    ///
    /// * A declared map parameter aborts on the interface spelling and silently takes an empty
    ///   map on the invalid one.
    ///   `hasKey $local "k"` renders false where `hasKey .Values.absent "k"` aborts.
    ///   `set` aborts in both spellings, but it is deliberately absent from this table because a
    ///   chart's own earlier `set` routinely creates the destination and nothing here proves that
    ///   mutation did not run.
    /// * A declared non-nilable parameter such as `string` or `bool` aborts either way.
    /// * Sprig's reflection faults either way too: its list helpers inspect the raw operand,
    ///   `deepCopy` walks it with `copystructure`, and Go's `index` and `len` reject an untyped nil
    ///   subject outright.
    ///   `has` is the exception because it tests `haystack == nil` first and answers false.
    ///
    /// The verdicts are measured against Helm 4.2.3 rather than inferred from signatures because
    /// the mechanisms cross.
    /// `len` declares `interface{}` yet rejects nil, while `join` declares `[]interface{}` yet
    /// renders empty text for it.
    #[must_use]
    pub(crate) const fn nil_aborts(self, direct_access: bool) -> bool {
        match self.nil_behavior {
            NilBehavior::Tolerates => false,
            NilBehavior::AlwaysAborts => true,
            NilBehavior::DirectAccessAborts => direct_access,
        }
    }

    /// Returns the argument positions that must be Go strings.
    ///
    /// `argument_count` includes a pipeline input, which Go templates append as the final
    /// argument.
    /// An empty result means the function has no catalogued string operands.
    #[must_use]
    pub(crate) fn string_operand_indices(self, argument_count: usize) -> Vec<usize> {
        if argument_count == 0 {
            return Vec::new();
        }
        match self.string_operands {
            StringOperands::All => (0..argument_count).collect(),
            StringOperands::First => vec![0],
            StringOperands::Last => vec![argument_count - 1],
            StringOperands::FirstAndLast if argument_count >= 3 => {
                vec![0, argument_count - 1]
            }
            StringOperands::FirstTwo if argument_count >= 3 => vec![0, 1],
            StringOperands::None | StringOperands::FirstAndLast | StringOperands::FirstTwo => {
                Vec::new()
            }
        }
    }
}

/// Map Helm/Sprig `typeIs` names to JSON Schema scalar/container names.
pub(crate) fn type_is_schema_type(expr: Option<&TemplateExpr>) -> Option<String> {
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
#[must_use]
pub(crate) fn go_type_schema_type(type_name: &str) -> Option<&'static str> {
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
#[must_use]
pub(crate) fn go_type_descriptor_spellings(schema_type: &str) -> &'static [&'static str] {
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
#[must_use]
pub(crate) fn type_descriptor_call_subject<'a>(
    function: &str,
    args: &'a [TemplateExpr],
) -> Option<&'a TemplateExpr> {
    match (function_semantics(function).predicate, args) {
        (PredicateSemantics::TypeDescriptor, [subject]) if function != "printf" => Some(subject),
        (PredicateSemantics::TypeDescriptor, [format_expr, subject]) if function == "printf" => {
            match format_expr.deparen() {
                TemplateExpr::Literal(Literal::String(format) | Literal::RawString(format))
                    if format == "%T" =>
                {
                    Some(subject)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns the lexical language required by a strict string parser operand.
///
/// The pattern is a conservative superset of every string accepted by the
/// runtime parser, so lowering it may miss some invalid inputs but never
/// rejects an input solely because the parser accepts a wider spelling.
#[must_use]
pub(crate) fn strict_parser_operand_pattern(
    semantics: FunctionSemantics,
    argument_count: usize,
) -> Option<(usize, &'static str)> {
    match semantics.parser {
        ParserSemantics::Semver if argument_count == 2 => {
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
        ParserSemantics::Duration if argument_count == 2 => Some((
            0,
            r"^[+-]?(0|((0*[0-9]{1,19}(\.[0-9]*)?|\.[0-9]+)ns|(0*[0-9]{1,16}(\.[0-9]*)?|\.[0-9]+)(us|µs|μs)|(0*[0-9]{1,13}(\.[0-9]*)?|\.[0-9]+)ms|(0*[0-9]{1,10}(\.[0-9]*)?|\.[0-9]+)s|(0*[0-9]{1,9}(\.[0-9]*)?|\.[0-9]+)m|(0*[0-9]{1,7}(\.[0-9]*)?|\.[0-9]+)h)+)$",
        )),
        // Go `url.Parse`'s accepted language, differential-verified against
        // ~900k fuzz candidates plus a fixed battery (zero mismatches and
        // zero widenings against the lenient oracle; the battery is pinned
        // by `url_parse_pattern_matches_the_go_verdicts`). Structure: an
        // authority form (`scheme://…` or `//…`) parses userinfo (ASCII
        // charset, valid escapes), then either a bracketed host — a valid
        // IPv6 literal (netip's language, the F87 enumeration) with an
        // optional nonempty `%25` zone — or a plain host whose raw bytes
        // are Go's host-legal set (`[` forbidden, `]` legal), whose
        // escapes decode only to `%25` or non-ASCII bytes, and whose text
        // after the LAST colon must be digits (the pre-Go-1.26 port rule
        // for every scheme; Go 1.26 hardened http/https to a single
        // colon, so a multi-colon http host is a deliberate cross-version
        // widening). Paths validate escapes, queries stay raw, and
        // fragments validate escapes but accept control characters — Go
        // splits the fragment off before its control-byte check.
        // Non-authority forms: `scheme:` raw opaque text, rooted
        // single-slash paths, and relative references whose first segment
        // is colon-free.
        ParserSemantics::Url if argument_count == 1 => Some((0, URL_PARSE_PATTERN)),
        _ => None,
    }
}

macro_rules! ipv4_pattern {
    () => {
        concat!(
            r"((25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])\.){3}",
            "(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])"
        )
    };
}

/// The IPv6 textual language of `net.ParseIP`/`netip.ParseAddr` (no
/// zone), shared by the certificate ip-list items and `urlParse` bracket
/// hosts: 1-4 hex digits per group, at most one `::` expanding at least
/// one zero group, an embedded dotted quad only as the final 4 bytes. The
/// v4-embedded arms enumerate the group splits because a regex cannot
/// count the 8-group budget globally.
macro_rules! ipv6_pattern {
    () => {
        concat!(
            "([0-9A-Fa-f]{1,4}:){7}[0-9A-Fa-f]{1,4}",
            "|([0-9A-Fa-f]{1,4}:){1,7}:",
            "|([0-9A-Fa-f]{1,4}:){1,6}:[0-9A-Fa-f]{1,4}",
            "|([0-9A-Fa-f]{1,4}:){1,5}(:[0-9A-Fa-f]{1,4}){1,2}",
            "|([0-9A-Fa-f]{1,4}:){1,4}(:[0-9A-Fa-f]{1,4}){1,3}",
            "|([0-9A-Fa-f]{1,4}:){1,3}(:[0-9A-Fa-f]{1,4}){1,4}",
            "|([0-9A-Fa-f]{1,4}:){1,2}(:[0-9A-Fa-f]{1,4}){1,5}",
            "|[0-9A-Fa-f]{1,4}:(:[0-9A-Fa-f]{1,4}){1,6}",
            "|:(:[0-9A-Fa-f]{1,4}){1,7}",
            "|::",
            "|([0-9A-Fa-f]{1,4}:){6}",
            ipv4_pattern!(),
            "|::([0-9A-Fa-f]{1,4}:){0,5}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){1}:([0-9A-Fa-f]{1,4}:){0,4}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){2}:([0-9A-Fa-f]{1,4}:){0,3}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){3}:([0-9A-Fa-f]{1,4}:){0,2}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){4}:([0-9A-Fa-f]{1,4}:){0,1}",
            ipv4_pattern!(),
            "|([0-9A-Fa-f]{1,4}:){5}:",
            ipv4_pattern!()
        )
    };
}

/// Raw host-legal bytes (Go's unescaped host set: `[` forbidden, `]`
/// legal, non-ASCII free) beside the host escapes `%25` and non-ASCII
/// byte encodings.
macro_rules! url_host_char {
    () => {
        concat!(
            "(?:[A-Za-z0-9._~!$&'()*+,;=\\]<>\"\\-]",
            r"|[^\u0000-\u007F]",
            "|%25|%[89A-Fa-f][0-9A-Fa-f])"
        )
    };
}
/// Query (raw text) and fragment (escapes validated, control bytes legal
/// — Go splits the fragment off before its control-character check).
macro_rules! url_query_fragment {
    () => {
        r"(?:\?[^\u0000-\u001F\u007F#]*)?(?:#(?:[^%]|%[0-9A-Fa-f]{2})*)?"
    };
}
/// A rooted single-slash path: the body must not begin with a second `/`
/// (that spelling is the authority form). Escapes validate.
macro_rules! url_rooted {
    () => {
        r"/(?:(?:[^\u0000-\u001F\u007F%/?#]|%[0-9A-Fa-f]{2})(?:[^\u0000-\u001F\u007F%?#]|%[0-9A-Fa-f]{2})*)?"
    };
}
/// Userinfo, then a bracketed IPv6 (optional `%25` zone) or a plain host
/// under the last-colon port rule: with any colon present the text after
/// the last one must be digits; colon-free hosts are free.
macro_rules! url_authority {
    () => {
        concat!(
            r"(?:(?:[A-Za-z0-9._~!$&'()*+,;=:@\-]|%[0-9A-Fa-f]{2})*@)?",
            r"(?:\[(?:",
            ipv6_pattern!(),
            r")(?:%25(?:[^\u0000-\u001F\u007F%/?#\]]|%[0-9A-Fa-f]{2})+)?\](?::[0-9]*)?",
            "|(?:(?:",
            url_host_char!(),
            "|:)*:[0-9]*|",
            url_host_char!(),
            "*))"
        )
    };
}
/// The optional path/query/fragment after an authority.
macro_rules! url_authority_tail {
    () => {
        concat!(
            r"(?:/(?:[^\u0000-\u001F\u007F%?#]|%[0-9A-Fa-f]{2})*)?",
            url_query_fragment!()
        )
    };
}

const URL_PARSE_PATTERN: &str = concat!(
    "^(?:",
    r"[A-Za-z][A-Za-z0-9+.\-]*://",
    url_authority!(),
    url_authority_tail!(),
    "|//",
    url_authority!(),
    url_authority_tail!(),
    r"|[A-Za-z][A-Za-z0-9+.\-]*:(?:[^\u0000-\u001F\u007F/?#][^\u0000-\u001F\u007F?#]*)?",
    url_query_fragment!(),
    r"|[A-Za-z][A-Za-z0-9+.\-]*:",
    url_rooted!(),
    url_query_fragment!(),
    r"|(?:[^\u0000-\u001F\u007F%:/?#]|%[0-9A-Fa-f]{2})+(?:/(?:[^\u0000-\u001F\u007F%?#]|%[0-9A-Fa-f]{2})*)?",
    url_query_fragment!(),
    "|",
    url_rooted!(),
    url_query_fragment!(),
    "|",
    url_query_fragment!(),
    ")$",
);

/// Returns the lexical language required of every ITEM of a strict
/// collection operand, keyed by the zero-based operand index.
///
/// Like [`strict_parser_operand_pattern`], the pattern is a conservative
/// superset of every string the runtime parser accepts, so lowering it may
/// miss some invalid inputs but never rejects one the parser accepts.
#[must_use]
pub(crate) fn strict_collection_item_pattern(
    semantics: FunctionSemantics,
    index: usize,
) -> Option<&'static str> {
    match (semantics.collection_items, index) {
        // genSignedCert/genSelfSignedCert pass every ip-list entry through
        // net.ParseIP and abort rendering on nil. The pattern is the
        // parser's EXACT accepted language (fuzz-differentialed against
        // `net.ParseIP`): dotted-quad IPv4 without leading zeros, plus the
        // shared IPv6 enumeration (no zone suffix — ParseIP rejects them).
        (CollectionItemSemantics::CertificateIpList, 1) => {
            Some(concat!("^(", ipv4_pattern!(), "|", ipv6_pattern!(), ")$",))
        }
        _ => None,
    }
}
