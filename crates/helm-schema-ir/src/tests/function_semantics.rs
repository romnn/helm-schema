use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;
use test_util::prelude::sim_assert_eq;

use crate::eval_env::{BindingEvaluationMode, EvalEnv};
use crate::expr_call_eval::{INTENTIONAL_DISPATCH_EXCEPTIONS, has_catalog_or_dispatch_exception};
use crate::function_semantics::{
    ArgumentEvaluationMode, CollectionShape, LegacySuppliedOperands, NilBehavior, OutputBehavior,
    PredicateSemantics, ProvenanceBehavior, function_semantics, strict_collection_item_pattern,
    strict_parser_operand_pattern,
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
    "uuidv4",
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
    let mut env = EvalEnv::default();
    let field = TemplateExpr::Field(vec!["Values".to_string(), "subject".to_string()]);
    sim_assert_eq!(
        have: env.argument_evaluation_mode(&field),
        want: ArgumentEvaluationMode::DirectLookup
    );
    let selector = TemplateExpr::Selector {
        operand: Box::new(TemplateExpr::Parenthesized(Box::new(field.clone()))),
        path: vec!["child".to_string()],
    };
    sim_assert_eq!(
        have: env.argument_evaluation_mode(&selector),
        want: ArgumentEvaluationMode::GroupedReceiverLookup {
            selected_segments: 1
        }
    );
    sim_assert_eq!(
        have: env.argument_evaluation_mode(&TemplateExpr::Parenthesized(Box::new(field))),
        want: ArgumentEvaluationMode::Evaluated
    );
    sim_assert_eq!(
        have: env.argument_evaluation_mode(&TemplateExpr::Parenthesized(Box::new(selector))),
        want: ArgumentEvaluationMode::Evaluated
    );

    let root_selector = TemplateExpr::Selector {
        operand: Box::new(TemplateExpr::Variable(String::new())),
        path: vec!["child".to_string()],
    };
    sim_assert_eq!(
        have: env.argument_evaluation_mode(&root_selector),
        want: ArgumentEvaluationMode::DirectLookup
    );

    let local = TemplateExpr::Variable("member".to_string());
    sim_assert_eq!(
        have: env.argument_evaluation_mode(&local),
        want: ArgumentEvaluationMode::DirectLookup
    );
    let local_selector = TemplateExpr::Selector {
        operand: Box::new(local),
        path: vec!["db".to_string()],
    };
    sim_assert_eq!(
        have: env.argument_evaluation_mode(&local_selector),
        want: ArgumentEvaluationMode::DirectLookup
    );

    let dot = TemplateExpr::Field(Vec::new());
    env.dot_binding_mode = BindingEvaluationMode::Evaluated;
    sim_assert_eq!(
        have: env.argument_evaluation_mode(&dot),
        want: ArgumentEvaluationMode::Evaluated
    );
}

#[test]
fn overlapping_function_facets_are_intentional() {
    let quote = function_semantics("quote");
    sim_assert_eq!(have: quote.legacy_operands, want: LegacySuppliedOperands::All);
    sim_assert_eq!(have: quote.behavior, want: OutputBehavior::Stringify);

    let indent = function_semantics("indent");
    sim_assert_eq!(have: indent.behavior, want: OutputBehavior::Transform);
    sim_assert_eq!(have: indent.provenance, want: ProvenanceBehavior::Preserve);

    let integer = function_semantics("int");
    sim_assert_eq!(have: integer.behavior, want: OutputBehavior::NumericCast);
    sim_assert_eq!(have: integer.provenance, want: ProvenanceBehavior::Preserve);

    sim_assert_eq!(
        have: function_semantics("uuidv4").behavior,
        want: OutputBehavior::NonEmpty
    );

    let semver = function_semantics("semverCompare");
    sim_assert_eq!(have: semver.predicate, want: PredicateSemantics::String);
    sim_assert_eq!(have: semver.legacy_operands, want: LegacySuppliedOperands::All);
    sim_assert_eq!(
        have: strict_parser_operand_pattern(semver, 2).map(|(index, _)| index),
        want: Some(1)
    );

    let split = function_semantics("split");
    sim_assert_eq!(have: split.collection, want: CollectionShape::StringSplit);
    sim_assert_eq!(have: split.legacy_operands, want: LegacySuppliedOperands::All);

    let merge = function_semantics("merge");
    sim_assert_eq!(have: merge.collection, want: CollectionShape::Merge);
    sim_assert_eq!(have: merge.nil_behavior, want: NilBehavior::DirectAccessAborts);

    sim_assert_eq!(
        have: strict_collection_item_pattern(function_semantics("genSignedCert"), 1).is_some(),
        want: true
    );
}

#[test]
fn signatures_distinguish_parameters_from_legacy_supplied_operands() {
    use crate::function_semantics::SuccessType;
    use crate::value::kinds::GoType;
    let round = function_semantics("round");
    sim_assert_eq!(have: (round.parameter(0), round.parameter(1), round.parameter(2), round.parameter(3)), want: (Some(GoType::Unknown), Some(GoType::Int), Some(GoType::Float64), Some(GoType::Float64)));
    let lower = function_semantics("lower");
    sim_assert_eq!(have: lower.parameter(0), want: Some(GoType::String));
    sim_assert_eq!(have: lower.parameter(1), want: None);
    sim_assert_eq!(have: lower.legacy_operand_indices(2), want: vec![1]);
    for name in ["quote", "squote", "toString", "urlquery"] {
        sim_assert_eq!(have: function_semantics(name).parameter(0), want: Some(GoType::Unknown));
    }
    for name in ["fromJson", "fromYaml", "fromToml", "urlParse"] {
        sim_assert_eq!(have: function_semantics(name).success, want: SuccessType::Fixed(GoType::Map));
    }
    sim_assert_eq!(have: function_semantics("mustFromJson").success, want: SuccessType::Selected);
    sim_assert_eq!(have: function_semantics("mustDateModify").success, want: SuccessType::Selected);
}
