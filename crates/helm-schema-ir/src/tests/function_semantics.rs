use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;
use test_util::prelude::sim_assert_eq;

use crate::expr_call_eval::{INTENTIONAL_DISPATCH_EXCEPTIONS, has_catalog_or_dispatch_exception};
use crate::function_semantics::{
    ArgumentEvaluationMode, CollectionShape, NilBehavior, OutputSemantics, PredicateSemantics,
    ProvenanceBehavior, StringOperands, argument_evaluation_mode, function_semantics,
    strict_collection_item_pattern, strict_parser_operand_pattern,
};

const KNOWN_FUNCTIONS: &[&str] = &[
    "add",
    "add1",
    "add1f",
    "addf",
    "adler32sum",
    "append",
    "b64dec",
    "b64enc",
    "ceil",
    "compact",
    "concat",
    "contains",
    "deepCopy",
    "first",
    "float64",
    "floor",
    "fromYaml",
    "genSelfSignedCert",
    "genSignedCert",
    "get",
    "hasKey",
    "hasPrefix",
    "hasSuffix",
    "htpasswd",
    "indent",
    "index",
    "initial",
    "int",
    "int64",
    "keys",
    "kindOf",
    "last",
    "len",
    "lookup",
    "lower",
    "max",
    "maxf",
    "merge",
    "mergeOverwrite",
    "min",
    "minf",
    "mul",
    "mulf",
    "mustDateModify",
    "mustDeepCopy",
    "mustMerge",
    "mustMergeOverwrite",
    "mustRegexMatch",
    "mustRegexReplaceAll",
    "mustRegexReplaceAllLiteral",
    "mustSlice",
    "mustUniq",
    "nindent",
    "omit",
    "pick",
    "pluck",
    "prepend",
    "printf",
    "push",
    "quote",
    "regexMatch",
    "regexReplaceAll",
    "regexReplaceAllLiteral",
    "regexSplit",
    "repeat",
    "replace",
    "rest",
    "reverse",
    "round",
    "semverCompare",
    "sha1sum",
    "sha256sum",
    "sha512sum",
    "slice",
    "split",
    "splitList",
    "splitn",
    "squote",
    "sub",
    "subf",
    "substr",
    "ternary",
    "toString",
    "toYaml",
    "tpl",
    "trim",
    "trimAll",
    "trimPrefix",
    "trimSuffix",
    "trunc",
    "typeOf",
    "uniq",
    "unset",
    "upper",
    "urlParse",
    "urlquery",
    "values",
];

#[test]
fn every_known_function_has_one_catalog_row() {
    let mut seen = BTreeSet::new();
    for function in KNOWN_FUNCTIONS {
        sim_assert_eq!(have: seen.insert(*function), want: true);
        sim_assert_eq!(have: function_semantics(function).is_known(), want: true);
    }
    sim_assert_eq!(have: function_semantics("notCatalogued").is_known(), want: false);
}

#[test]
fn dispatcher_special_forms_are_catalogued_or_intentional_exceptions() {
    let mut seen = BTreeSet::new();
    for function in KNOWN_FUNCTIONS {
        sim_assert_eq!(
            have: has_catalog_or_dispatch_exception(function),
            want: true
        );
    }
    for function in INTENTIONAL_DISPATCH_EXCEPTIONS {
        sim_assert_eq!(have: seen.insert(*function), want: true);
        sim_assert_eq!(have: function_semantics(function).is_known(), want: false);
        sim_assert_eq!(
            have: has_catalog_or_dispatch_exception(function),
            want: true
        );
    }
    sim_assert_eq!(
        have: has_catalog_or_dispatch_exception("notCatalogued"),
        want: false
    );
}

#[test]
fn argument_evaluation_mode_preserves_grouping() {
    let field = TemplateExpr::Field(vec!["Values".to_string(), "subject".to_string()]);
    sim_assert_eq!(
        have: argument_evaluation_mode(&field),
        want: ArgumentEvaluationMode::DirectLookup
    );
    let selector = TemplateExpr::Selector {
        operand: Box::new(TemplateExpr::Parenthesized(Box::new(field.clone()))),
        path: vec!["child".to_string()],
    };
    sim_assert_eq!(
        have: argument_evaluation_mode(&selector),
        want: ArgumentEvaluationMode::GroupedReceiverLookup {
            selected_segments: 1
        }
    );
    sim_assert_eq!(
        have: argument_evaluation_mode(&TemplateExpr::Parenthesized(Box::new(field))),
        want: ArgumentEvaluationMode::Evaluated
    );
    sim_assert_eq!(
        have: argument_evaluation_mode(&TemplateExpr::Parenthesized(Box::new(selector))),
        want: ArgumentEvaluationMode::Evaluated
    );
}

#[test]
fn overlapping_function_facets_are_intentional() {
    let quote = function_semantics("quote");
    sim_assert_eq!(have: quote.string_operands, want: StringOperands::All);
    sim_assert_eq!(have: quote.output, want: OutputSemantics::TotalStringification);

    let indent = function_semantics("indent");
    sim_assert_eq!(have: indent.output, want: OutputSemantics::StringTransform);
    sim_assert_eq!(have: indent.provenance, want: ProvenanceBehavior::Preserve);

    let integer = function_semantics("int");
    sim_assert_eq!(have: integer.output, want: OutputSemantics::TotalNumericCast);
    sim_assert_eq!(have: integer.provenance, want: ProvenanceBehavior::Preserve);

    let semver = function_semantics("semverCompare");
    sim_assert_eq!(have: semver.predicate, want: PredicateSemantics::String);
    sim_assert_eq!(have: semver.string_operands, want: StringOperands::All);
    sim_assert_eq!(
        have: strict_parser_operand_pattern(semver, 2).map(|(index, _)| index),
        want: Some(1)
    );

    let split = function_semantics("split");
    sim_assert_eq!(have: split.collection, want: CollectionShape::StringSplit);
    sim_assert_eq!(have: split.string_operands, want: StringOperands::All);

    let merge = function_semantics("merge");
    sim_assert_eq!(have: merge.collection, want: CollectionShape::Merge);
    sim_assert_eq!(have: merge.nil_behavior, want: NilBehavior::DirectAccessAborts);

    sim_assert_eq!(
        have: strict_collection_item_pattern(function_semantics("genSignedCert"), 1).is_some(),
        want: true
    );
}
