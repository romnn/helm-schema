use crate::value::kinds::GoType;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SuccessType {
    Fixed(GoType),
    Selected,
}

/// Classifies whether a nil operand aborts, including the direct-access distinction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NilBehavior {
    Tolerates,
    AlwaysAborts,
    DirectAccessAborts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputBehavior {
    Opaque,
    NonEmpty,
    Transform,
    Stringify,
    Encoded(OutputAlphabet),
    NumericCast,
    Arithmetic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputAlphabet {
    Hexadecimal,
    Decimal,
    Base64,
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

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParserSemantics {
    None,
    Semver,
    Duration,
    Url,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectionItemSemantics {
    None,
    CertificateIpList,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputRead {
    Value,
    Contents,
    Keyed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HeapEffect {
    ReadOnly,
    Operands,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FunctionSemantics {
    known: bool,
    pub(crate) input_read: InputRead,
    pub(crate) heap_effect: HeapEffect,
    pub(crate) success: SuccessType,
    parameters: &'static [GoType],
    variadic: Option<GoType>,
    pub(crate) nil_behavior: NilBehavior,
    pub(crate) behavior: OutputBehavior,
    pub(crate) collection: CollectionShape,
    pub(crate) provenance: ProvenanceBehavior,
    pub(crate) predicate: PredicateSemantics,
    #[cfg(test)]
    parser: ParserSemantics,
    #[cfg(test)]
    collection_items: CollectionItemSemantics,
}

const UNKNOWN: FunctionSemantics = FunctionSemantics {
    known: false,
    input_read: InputRead::Value,
    heap_effect: HeapEffect::Unknown,
    success: SuccessType::Selected,
    parameters: &[],
    variadic: None,
    nil_behavior: NilBehavior::Tolerates,
    behavior: OutputBehavior::Opaque,
    collection: CollectionShape::None,
    provenance: ProvenanceBehavior::Discard,
    predicate: PredicateSemantics::None,
    #[cfg(test)]
    parser: ParserSemantics::None,
    #[cfg(test)]
    collection_items: CollectionItemSemantics::None,
};

const KNOWN: FunctionSemantics = FunctionSemantics {
    known: true,
    heap_effect: HeapEffect::ReadOnly,
    ..UNKNOWN
};

use CollectionShape::{Merge, StringSplit};
use GoType::{Bool, Float64, Int, Int64, Map, Slice, String as Text, Unknown};
use NilBehavior::{AlwaysAborts, DirectAccessAborts};
use OutputBehavior::{Arithmetic, Encoded, NonEmpty, NumericCast, Stringify, Transform};
use PredicateSemantics::{String as StringPredicate, TypeDescriptor};
use ProvenanceBehavior::Preserve;

const ARITHMETIC: FunctionSemantics = FunctionSemantics {
    behavior: Arithmetic,
    ..KNOWN
};

const fn encoded(alphabet: OutputAlphabet) -> FunctionSemantics {
    FunctionSemantics {
        behavior: Encoded(alphabet),
        ..KNOWN
    }
    .fixed(&[Text], Text)
}

const STRICT: FunctionSemantics = FunctionSemantics {
    nil_behavior: AlwaysAborts,
    ..KNOWN
};

const TRANSFORM: FunctionSemantics = FunctionSemantics {
    behavior: Transform,
    ..KNOWN
};

const STRING_PREDICATE: FunctionSemantics = FunctionSemantics {
    predicate: StringPredicate,
    ..KNOWN
};

const PRESERVING_STRICT: FunctionSemantics = FunctionSemantics {
    nil_behavior: AlwaysAborts,
    provenance: Preserve,
    ..KNOWN
};

const NUMERIC_CAST: FunctionSemantics = FunctionSemantics {
    behavior: NumericCast,
    ..KNOWN
};

const PRESERVING: FunctionSemantics = FunctionSemantics {
    provenance: Preserve,
    ..KNOWN
};

const IP_CERTIFICATE: FunctionSemantics = FunctionSemantics {
    #[cfg(test)]
    collection_items: CollectionItemSemantics::CertificateIpList,
    ..KNOWN
};

const MAP_LOOKUP: FunctionSemantics = FunctionSemantics {
    nil_behavior: DirectAccessAborts,
    ..KNOWN
};

const INDENT: FunctionSemantics = FunctionSemantics {
    behavior: Transform,
    provenance: Preserve,
    ..KNOWN
};

const PRESERVING_CAST: FunctionSemantics = FunctionSemantics {
    behavior: NumericCast,
    provenance: Preserve,
    ..KNOWN
};

const DESCRIPTOR: FunctionSemantics = FunctionSemantics {
    predicate: TypeDescriptor,
    ..KNOWN
};

const MAP_MERGE: FunctionSemantics = FunctionSemantics {
    input_read: InputRead::Contents,
    heap_effect: HeapEffect::Operands,
    nil_behavior: DirectAccessAborts,
    collection: Merge,
    ..KNOWN
};

const DURATION_PARSER: FunctionSemantics = FunctionSemantics {
    predicate: StringPredicate,
    #[cfg(test)]
    parser: ParserSemantics::Duration,
    ..KNOWN
};

const FORMATTER: FunctionSemantics = FunctionSemantics {
    input_read: InputRead::Contents,
    provenance: Preserve,
    predicate: TypeDescriptor,
    ..KNOWN
};

const STRINGIFIER: FunctionSemantics = FunctionSemantics {
    input_read: InputRead::Contents,
    behavior: Stringify,
    ..KNOWN
};

const REGEX_SPLIT: FunctionSemantics = FunctionSemantics {
    collection: StringSplit,
    ..KNOWN
};

const SEMVER_PARSER: FunctionSemantics = FunctionSemantics {
    predicate: StringPredicate,
    #[cfg(test)]
    parser: ParserSemantics::Semver,
    ..KNOWN
};

const SPLIT: FunctionSemantics = FunctionSemantics {
    collection: StringSplit,
    ..KNOWN
};

const LIMITED_SPLIT: FunctionSemantics = FunctionSemantics {
    collection: StringSplit,
    ..KNOWN
};

const URL_PARSER: FunctionSemantics = FunctionSemantics {
    predicate: StringPredicate,
    #[cfg(test)]
    parser: ParserSemantics::Url,
    ..KNOWN
};

const NONEMPTY: FunctionSemantics = FunctionSemantics {
    behavior: NonEmpty,
    ..KNOWN
};

impl FunctionSemantics {
    const fn recognized(self) -> Self {
        Self {
            heap_effect: match self.heap_effect {
                HeapEffect::Unknown => HeapEffect::ReadOnly,
                HeapEffect::ReadOnly | HeapEffect::Operands => self.heap_effect,
            },
            ..self
        }
    }

    const fn reading(self, input_read: InputRead) -> Self {
        Self { input_read, ..self }
    }

    const fn writing(self, heap_effect: HeapEffect) -> Self {
        Self {
            heap_effect,
            ..self
        }
    }

    const fn fixed(self, parameters: &'static [GoType], result: GoType) -> Self {
        Self {
            success: SuccessType::Fixed(result),
            parameters,
            variadic: None,
            ..self.recognized()
        }
    }

    const fn variadic(self, parameters: &'static [GoType], tail: GoType, result: GoType) -> Self {
        Self {
            success: SuccessType::Fixed(result),
            parameters,
            variadic: Some(tail),
            ..self.recognized()
        }
    }

    const fn selected(self, parameters: &'static [GoType], variadic: Option<GoType>) -> Self {
        Self {
            success: SuccessType::Selected,
            parameters,
            variadic,
            ..self.recognized()
        }
    }

    /// Checks only the Go call signature; builtin transfer preconditions remain separate.
    pub(crate) fn accepts_arity(self, count: usize) -> Option<bool> {
        if !self.known
            && self.parameters.is_empty()
            && self.variadic.is_none()
            && self.success == SuccessType::Selected
        {
            return None;
        }
        Some(match self.variadic {
            Some(_) => count >= self.parameters.len(),
            None => count == self.parameters.len(),
        })
    }

    pub(crate) fn parameter(self, index: usize) -> Option<GoType> {
        self.parameters.get(index).copied().or(self.variadic)
    }
}

const STRING_MATCH: FunctionSemantics = STRING_PREDICATE.fixed(&[Text, Text], Bool);

/// Returns the analyzer-owned semantics for one Helm or Sprig function.
///
/// Each recognized name has one row combining its signature and transfer facets.
#[must_use]
pub(crate) fn function_semantics(function: &str) -> FunctionSemantics {
    match function {
        "abbrev" | "wrap" => UNKNOWN.fixed(&[Int, Text], Text),
        "abbrevboth" => UNKNOWN.fixed(&[Int, Int, Text], Text),
        "add" => ARITHMETIC.variadic(&[], Unknown, Int64),
        "add1" => ARITHMETIC.fixed(&[Unknown], Int64),
        "add1f" | "ceil" | "floor" => ARITHMETIC.fixed(&[Unknown], Float64),
        "addf" => ARITHMETIC.variadic(&[], Unknown, Float64),
        "adler32sum" => encoded(OutputAlphabet::Decimal),
        "sha1sum" | "sha256sum" | "sha512sum" => encoded(OutputAlphabet::Hexadecimal),
        "and" | "coalesce" | "or" => UNKNOWN.selected(&[], Some(Unknown)),
        "b64dec" | "lower" | "trim" | "upper" => TRANSFORM.fixed(&[Text], Text),
        "b64enc" => encoded(OutputAlphabet::Base64),
        "chunk" | "mustChunk" => UNKNOWN.selected(&[Int, Unknown], None),
        "concat" => STRICT.selected(&[], Some(Unknown)),
        "contains" | "hasPrefix" | "hasSuffix" | "mustRegexMatch" | "regexMatch" => STRING_MATCH,
        "default" => UNKNOWN.selected(&[Unknown], Some(Unknown)),
        "empty" | "not" => UNKNOWN.fixed(&[Unknown], Bool),
        "eq" => UNKNOWN.variadic(&[Unknown], Unknown, Bool),
        "fail" => UNKNOWN.fixed(&[Text], Text),
        "first" | "last" => STRICT.selected(&[Unknown], None),
        "float64" => NUMERIC_CAST.fixed(&[Unknown], Float64),
        "fromJson" | "fromToml" => UNKNOWN.fixed(&[Text], Map),
        "fromJsonArray" | "fromYamlArray" => UNKNOWN.fixed(&[Text], Slice),
        "fromYaml" => PRESERVING.fixed(&[Text], Map),
        "ge" | "gt" | "le" | "lt" | "ne" => UNKNOWN.fixed(&[Unknown, Unknown], Bool),
        "genCA" => UNKNOWN.selected(&[Text, Int], None),
        "genCAWithKey" => UNKNOWN.selected(&[Text, Int, Text], None),
        "genSelfSignedCert" => IP_CERTIFICATE.selected(&[Text, Slice, Slice, Int], None),
        "genSelfSignedCertWithKey" => UNKNOWN.selected(&[Text, Slice, Slice, Int, Text], None),
        "genSignedCert" => IP_CERTIFICATE.selected(&[Text, Slice, Slice, Int, Unknown], None),
        "genSignedCertWithKey" => UNKNOWN.selected(&[Text, Slice, Slice, Int, Unknown, Text], None),
        "htpasswd" | "trimAll" | "trimPrefix" | "trimSuffix" => {
            TRANSFORM.fixed(&[Text, Text], Text)
        }
        "include" | "template" => UNKNOWN
            .fixed(&[Text, Unknown], Text)
            .writing(HeapEffect::Unknown),
        "indent" | "nindent" => INDENT.fixed(&[Int, Text], Text),
        "mustSlice" | "slice" => STRICT.selected(&[Unknown], Some(Unknown)),
        "int" => PRESERVING_CAST.fixed(&[Unknown], Int),
        "int64" => NUMERIC_CAST.fixed(&[Unknown], Int64),
        "kindOf" | "typeOf" => DESCRIPTOR.fixed(&[Unknown], Text),
        "kindIs" | "typeIs" | "typeIsLike" => DESCRIPTOR.fixed(&[Text, Unknown], Bool),
        "lookup" => KNOWN.fixed(&[Text, Text, Text, Text], Map),
        "max" | "min" | "mul" => ARITHMETIC.variadic(&[Unknown], Unknown, Int64),
        "maxf" | "minf" | "mulf" | "subf" => ARITHMETIC.variadic(&[Unknown], Unknown, Float64),
        "mustDateModify" => DURATION_PARSER.selected(&[Text, Unknown], None),
        "mustFromJson" | "semver" => UNKNOWN.selected(&[Text], None),
        "mustRegexFindAll" | "mustRegexSplit" | "regexFindAll" => {
            UNKNOWN.fixed(&[Text, Text, Int], Slice)
        }
        "mustRegexReplaceAll"
        | "mustRegexReplaceAllLiteral"
        | "regexReplaceAll"
        | "regexReplaceAllLiteral"
        | "replace" => TRANSFORM.fixed(&[Text, Text, Text], Text),
        "mustToJson" | "mustToRawJson" | "mustToPrettyJson" | "toJson" | "toRawJson"
        | "toPrettyJson" => UNKNOWN.fixed(&[Unknown], Text).reading(InputRead::Contents),
        "mustUniq" | "uniq" => PRESERVING_STRICT.fixed(&[Unknown], Slice),
        "now" => UNKNOWN.fixed(&[], Unknown),
        "plural" => UNKNOWN.fixed(&[Text, Text, Int], Text),
        "print" | "println" => UNKNOWN
            .variadic(&[], Unknown, Text)
            .reading(InputRead::Contents),
        "printf" => FORMATTER.variadic(&[Text], Unknown, Text),
        "quote" | "squote" | "urlquery" => STRINGIFIER.variadic(&[], Unknown, Text),
        "randAlpha" | "randAlphaNum" | "randAscii" | "randBytes" | "randNumeric" => {
            UNKNOWN.fixed(&[Int], Text)
        }
        "randInt" => UNKNOWN.fixed(&[Int, Int], Int),
        "regexSplit" => REGEX_SPLIT.fixed(&[Text, Text, Int], Slice),
        "repeat" | "trunc" => TRANSFORM.fixed(&[Int, Text], Text),
        "required" => UNKNOWN.selected(&[Text, Unknown], None),
        "round" => ARITHMETIC.variadic(&[Unknown, Int], Float64, Float64),
        "semverCompare" => SEMVER_PARSER.fixed(&[Text, Text], Bool),
        "seq" => UNKNOWN.variadic(&[], Int, Text),
        "split" => SPLIT.fixed(&[Text, Text], Map),
        "splitList" => SPLIT.fixed(&[Text, Text], Slice),
        "splitn" => LIMITED_SPLIT.fixed(&[Text, Int, Text], Map),
        "sub" => ARITHMETIC.fixed(&[Unknown, Unknown], Int64),
        "substr" => TRANSFORM.fixed(&[Int, Int, Text], Text),
        "ternary" => STRICT.selected(&[Unknown, Unknown, Bool], None),
        "toString" => STRINGIFIER.fixed(&[Unknown], Text),
        "toYaml" | "toYamlPretty" => PRESERVING
            .fixed(&[Unknown], Text)
            .reading(InputRead::Contents),
        "tpl" => PRESERVING
            .fixed(&[Text, Unknown], Text)
            .writing(HeapEffect::Unknown),
        "until" => UNKNOWN.selected(&[Int], None),
        "untilStep" => UNKNOWN.selected(&[Int, Int, Int], None),
        "urlParse" => URL_PARSER.fixed(&[Text], Map),
        "uuidv4" => NONEMPTY.fixed(&[], Text),
        "wrapWith" => UNKNOWN.fixed(&[Int, Text, Text], Text),
        _ => collection_semantics(function),
    }
}

fn collection_semantics(function: &str) -> FunctionSemantics {
    match function {
        "dig" => UNKNOWN
            .selected(&[], Some(Unknown))
            .reading(InputRead::Contents),
        "append" | "mustAppend" | "prepend" | "mustPrepend" | "push" => {
            STRICT.fixed(&[Unknown, Unknown], Slice)
        }
        "compact" | "initial" | "rest" | "reverse" => STRICT.fixed(&[Unknown], Slice),
        "len" => STRICT.fixed(&[Unknown], Int).reading(InputRead::Contents),
        "list" | "tuple" => UNKNOWN.variadic(&[], Unknown, Slice),
        "deepCopy" | "mustDeepCopy" => PRESERVING_STRICT
            .selected(&[Unknown], None)
            .reading(InputRead::Contents),
        "dict" => UNKNOWN.variadic(&[], Unknown, Map),
        "get" => MAP_LOOKUP
            .selected(&[Map, Text], None)
            .reading(InputRead::Keyed),
        "hasKey" => MAP_LOOKUP
            .fixed(&[Map, Text], Bool)
            .reading(InputRead::Keyed),
        "index" => STRICT
            .selected(&[Unknown], Some(Unknown))
            .reading(InputRead::Keyed),
        "keys" => MAP_LOOKUP
            .variadic(&[], Map, Slice)
            .reading(InputRead::Contents),
        "set" => MAP_LOOKUP
            .fixed(&[Map, Text, Unknown], Map)
            .writing(HeapEffect::Operands),
        "merge" | "mergeOverwrite" | "mustMerge" | "mustMergeOverwrite" => {
            MAP_MERGE.selected(&[Map], Some(Map))
        }
        "omit" | "pick" => MAP_LOOKUP
            .variadic(&[Map], Text, Map)
            .reading(InputRead::Contents),
        "pluck" => MAP_LOOKUP.variadic(&[Text], Map, Slice),
        "unset" => MAP_LOOKUP
            .fixed(&[Map, Text], Map)
            .writing(HeapEffect::Operands),
        "values" => MAP_LOOKUP.fixed(&[Map], Slice).reading(InputRead::Contents),
        "sortAlpha" => UNKNOWN
            .fixed(&[Unknown], Slice)
            .reading(InputRead::Contents)
            .writing(HeapEffect::Operands),
        "toStrings" => UNKNOWN
            .fixed(&[Unknown], Slice)
            .reading(InputRead::Contents),
        "join" => UNKNOWN
            .fixed(&[Text, Unknown], Text)
            .reading(InputRead::Contents),
        _ => UNKNOWN,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MethodReceiver {
    Values,
    Files,
    ApiVersions,
    KubeVersion,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    ValuesAsMap,
    ValuesTable,
    ValuesYaml,
    FilesGet,
    FilesGetBytes,
    FilesGlob,
    FilesLines,
    FilesAsConfig,
    FilesAsSecrets,
    ApiVersionsHas,
    KubeVersionGitVersion,
    KubeVersionString,
    Foreign(std::string::String),
}

impl Method {
    pub(crate) fn resolve(receiver: MethodReceiver, name: &str) -> Self {
        match (receiver, name) {
            (MethodReceiver::Values, "AsMap") => Self::ValuesAsMap,
            (MethodReceiver::Values, "Table") => Self::ValuesTable,
            (MethodReceiver::Values, "YAML") => Self::ValuesYaml,
            (MethodReceiver::Files, "Get") => Self::FilesGet,
            (MethodReceiver::Files, "GetBytes") => Self::FilesGetBytes,
            (MethodReceiver::Files, "Glob") => Self::FilesGlob,
            (MethodReceiver::Files, "Lines") => Self::FilesLines,
            (MethodReceiver::Files, "AsConfig") => Self::FilesAsConfig,
            (MethodReceiver::Files, "AsSecrets") => Self::FilesAsSecrets,
            (MethodReceiver::ApiVersions, "Has") => Self::ApiVersionsHas,
            (MethodReceiver::KubeVersion, "GitVersion") => Self::KubeVersionGitVersion,
            (MethodReceiver::KubeVersion, "String") => Self::KubeVersionString,
            (
                MethodReceiver::Values
                | MethodReceiver::Files
                | MethodReceiver::ApiVersions
                | MethodReceiver::KubeVersion
                | MethodReceiver::Other,
                _,
            ) => Self::Foreign(name.to_owned()),
        }
    }

    pub(crate) fn name(&self) -> &str {
        match self {
            Self::ValuesAsMap => "AsMap",
            Self::ValuesTable => "Table",
            Self::ValuesYaml => "YAML",
            Self::FilesGet => "Get",
            Self::FilesGetBytes => "GetBytes",
            Self::FilesGlob => "Glob",
            Self::FilesLines => "Lines",
            Self::FilesAsConfig => "AsConfig",
            Self::FilesAsSecrets => "AsSecrets",
            Self::ApiVersionsHas => "Has",
            Self::KubeVersionGitVersion => "GitVersion",
            Self::KubeVersionString => "String",
            Self::Foreign(name) => name,
        }
    }

    pub(crate) fn semantics(&self) -> FunctionSemantics {
        match self {
            Self::ValuesAsMap => KNOWN.selected(&[], None).reading(InputRead::Contents),
            Self::ValuesTable => STRICT.selected(&[Text], None),
            Self::ValuesYaml => KNOWN.fixed(&[], Text).reading(InputRead::Contents),
            Self::FilesGet => STRICT.fixed(&[Text], Text),
            Self::FilesGetBytes | Self::FilesLines => STRICT.fixed(&[Text], Slice),
            Self::FilesGlob => STRICT.fixed(&[Text], Map),
            Self::FilesAsConfig
            | Self::FilesAsSecrets
            | Self::KubeVersionGitVersion
            | Self::KubeVersionString => KNOWN.fixed(&[], Text),
            Self::ApiVersionsHas => STRICT.fixed(&[Text], Bool),
            Self::Foreign(_) => UNKNOWN.reading(InputRead::Contents),
        }
    }

    pub(crate) fn is_foreign(&self) -> bool {
        match self {
            Self::ValuesAsMap
            | Self::ValuesTable
            | Self::ValuesYaml
            | Self::FilesGet
            | Self::FilesGetBytes
            | Self::FilesGlob
            | Self::FilesLines
            | Self::FilesAsConfig
            | Self::FilesAsSecrets
            | Self::ApiVersionsHas
            | Self::KubeVersionGitVersion
            | Self::KubeVersionString => false,
            Self::Foreign(_) => true,
        }
    }
}

#[cfg(test)]
#[path = "tests/host_patterns.rs"]
mod test_patterns;
#[cfg(test)]
pub(crate) use test_patterns::{strict_collection_item_pattern, strict_parser_operand_pattern};
