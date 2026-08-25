use std::collections::BTreeSet;

use helm_schema_core::{FailValueRequirement, GuardValue};

/// One disjoint JSON value kind admitted by a runtime requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum JsonValueKind {
    Null,
    Boolean,
    Integer,
    NonIntegerNumber,
    String,
    Array,
    Object,
}

/// Where a value requirement is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequirementPosition {
    WholeValue,
    Member,
}

/// Constraint on the integer count whose generated members all satisfy a requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntegerRangeConstraint {
    Any,
    Maximum(i64),
}

pub(crate) fn admitted_json_value_kinds(
    requirements: &[FailValueRequirement],
    position: RequirementPosition,
) -> BTreeSet<JsonValueKind> {
    all_json_value_kinds()
        .into_iter()
        .filter(|kind| {
            requirements
                .iter()
                .all(|requirement| requirement_admits_kind(requirement, *kind, position))
        })
        .collect()
}

pub(crate) fn json_value_kinds_for_schema_type(schema_type: &str) -> BTreeSet<JsonValueKind> {
    match schema_type {
        "null" => BTreeSet::from([JsonValueKind::Null]),
        "boolean" => BTreeSet::from([JsonValueKind::Boolean]),
        "integer" => BTreeSet::from([JsonValueKind::Integer]),
        "number" => BTreeSet::from([JsonValueKind::Integer, JsonValueKind::NonIntegerNumber]),
        "string" => BTreeSet::from([JsonValueKind::String]),
        "array" => BTreeSet::from([JsonValueKind::Array]),
        "object" => BTreeSet::from([JsonValueKind::Object]),
        _ => BTreeSet::new(),
    }
}

pub(crate) fn integer_range_constraint(
    requirements: &[FailValueRequirement],
) -> Option<IntegerRangeConstraint> {
    let mut constraint = IntegerRangeConstraint::Any;
    for requirement in requirements {
        constraint = intersect_integer_constraints(
            constraint,
            requirement_integer_range_constraint(requirement)?,
        );
    }
    Some(constraint)
}

fn requirement_admits_kind(
    requirement: &FailValueRequirement,
    kind: JsonValueKind,
    position: RequirementPosition,
) -> bool {
    match requirement {
        FailValueRequirement::SchemaType(required) => {
            json_value_kinds_for_schema_type(required).contains(&kind)
                || position == RequirementPosition::WholeValue && kind == JsonValueKind::Null
        }
        FailValueRequirement::SchemaTypeEvenNull(required) => {
            json_value_kinds_for_schema_type(required).contains(&kind)
        }
        FailValueRequirement::ComparableKind(required) => {
            kind == JsonValueKind::Null
                || json_value_kinds_for_schema_type(required).contains(&kind)
        }
        FailValueRequirement::TruthyImpliesSchemaType(_)
        | FailValueRequirement::HelmFalsy
        | FailValueRequirement::FieldHelmFalsy { .. }
        | FailValueRequirement::FieldNotEquals { .. }
        | FailValueRequirement::NotEquals(_)
        | FailValueRequirement::QuotedSerializationSafe { .. }
        | FailValueRequirement::PlainScalarSafe { .. } => true,
        FailValueRequirement::HelmTruthy => kind != JsonValueKind::Null,
        FailValueRequirement::PrintfStringOperand => {
            matches!(kind, JsonValueKind::Object | JsonValueKind::String)
        }
        FailValueRequirement::FieldEquals { .. }
        | FailValueRequirement::FieldPresentNotNull { .. }
        | FailValueRequirement::FieldHelmTruthy { .. }
        | FailValueRequirement::HasMember(_)
        | FailValueRequirement::HasMemberEvenDefaulted(_) => kind == JsonValueKind::Object,
        FailValueRequirement::NotSchemaType(rejected) => {
            !json_value_kinds_for_schema_type(rejected).contains(&kind)
        }
        FailValueRequirement::MatchesPattern { .. }
        | FailValueRequirement::NotMatchesPattern { .. }
        | FailValueRequirement::StringLengthBounds { .. } => kind == JsonValueKind::String,
        FailValueRequirement::MemberHost { handled_kinds, .. } => {
            kind == JsonValueKind::Object
                || handled_kinds
                    .iter()
                    .any(|handled| json_value_kinds_for_schema_type(handled).contains(&kind))
        }
        FailValueRequirement::Iterable { allow_integer } => {
            matches!(
                kind,
                JsonValueKind::Array | JsonValueKind::Null | JsonValueKind::Object
            ) || *allow_integer && kind == JsonValueKind::Integer
        }
        FailValueRequirement::IndexableAt(_) => {
            matches!(kind, JsonValueKind::Array | JsonValueKind::String)
        }
        FailValueRequirement::SplitSegmentsAtLeast {
            allow_non_string, ..
        } => kind == JsonValueKind::String || *allow_non_string,
        FailValueRequirement::AnyOf(alternatives) => alternatives.iter().any(|alternative| {
            alternative
                .iter()
                .all(|requirement| requirement_admits_kind(requirement, kind, position))
        }),
    }
}

fn requirement_integer_range_constraint(
    requirement: &FailValueRequirement,
) -> Option<IntegerRangeConstraint> {
    match requirement {
        FailValueRequirement::SchemaType(required)
        | FailValueRequirement::SchemaTypeEvenNull(required)
        | FailValueRequirement::ComparableKind(required) => Some(
            if json_value_kinds_for_schema_type(required).contains(&JsonValueKind::Integer) {
                IntegerRangeConstraint::Any
            } else {
                IntegerRangeConstraint::Maximum(0)
            },
        ),
        FailValueRequirement::TruthyImpliesSchemaType(required) => Some(
            if json_value_kinds_for_schema_type(required).contains(&JsonValueKind::Integer) {
                IntegerRangeConstraint::Any
            } else {
                IntegerRangeConstraint::Maximum(1)
            },
        ),
        FailValueRequirement::HelmTruthy
        | FailValueRequirement::FieldEquals { .. }
        | FailValueRequirement::FieldPresentNotNull { .. }
        | FailValueRequirement::FieldHelmTruthy { .. }
        | FailValueRequirement::HasMember(_)
        | FailValueRequirement::HasMemberEvenDefaulted(_)
        | FailValueRequirement::MatchesPattern { .. }
        | FailValueRequirement::NotMatchesPattern { .. }
        | FailValueRequirement::StringLengthBounds { .. }
        | FailValueRequirement::PrintfStringOperand
        | FailValueRequirement::IndexableAt(_) => Some(IntegerRangeConstraint::Maximum(0)),
        FailValueRequirement::HelmFalsy => Some(IntegerRangeConstraint::Maximum(1)),
        FailValueRequirement::NotEquals(GuardValue::Int(value)) => Some(if *value < 0 {
            IntegerRangeConstraint::Any
        } else {
            IntegerRangeConstraint::Maximum(*value)
        }),
        FailValueRequirement::NotEquals(GuardValue::Float(_)) => None,
        FailValueRequirement::NotEquals(
            GuardValue::String(_) | GuardValue::Bool(_) | GuardValue::Null,
        )
        | FailValueRequirement::FieldHelmFalsy { .. }
        | FailValueRequirement::FieldNotEquals { .. }
        | FailValueRequirement::QuotedSerializationSafe { .. }
        | FailValueRequirement::PlainScalarSafe { .. } => Some(IntegerRangeConstraint::Any),
        FailValueRequirement::NotSchemaType(rejected) => Some(
            if json_value_kinds_for_schema_type(rejected).contains(&JsonValueKind::Integer) {
                IntegerRangeConstraint::Maximum(0)
            } else {
                IntegerRangeConstraint::Any
            },
        ),
        FailValueRequirement::MemberHost { handled_kinds, .. } => Some(
            if handled_kinds.iter().any(|handled| {
                json_value_kinds_for_schema_type(handled).contains(&JsonValueKind::Integer)
            }) {
                IntegerRangeConstraint::Any
            } else {
                IntegerRangeConstraint::Maximum(0)
            },
        ),
        FailValueRequirement::Iterable { allow_integer } => Some(if *allow_integer {
            IntegerRangeConstraint::Any
        } else {
            IntegerRangeConstraint::Maximum(0)
        }),
        FailValueRequirement::SplitSegmentsAtLeast {
            allow_non_string, ..
        } => Some(if *allow_non_string {
            IntegerRangeConstraint::Any
        } else {
            IntegerRangeConstraint::Maximum(0)
        }),
        FailValueRequirement::AnyOf(alternatives) => {
            let mut constraint = None;
            for alternative in alternatives {
                let alternative = integer_range_constraint(alternative)?;
                constraint = Some(match constraint {
                    None => alternative,
                    Some(current) => union_integer_constraints(current, alternative),
                });
            }
            Some(constraint.unwrap_or(IntegerRangeConstraint::Maximum(0)))
        }
    }
}

fn all_json_value_kinds() -> BTreeSet<JsonValueKind> {
    BTreeSet::from([
        JsonValueKind::Null,
        JsonValueKind::Boolean,
        JsonValueKind::Integer,
        JsonValueKind::NonIntegerNumber,
        JsonValueKind::String,
        JsonValueKind::Array,
        JsonValueKind::Object,
    ])
}

fn intersect_integer_constraints(
    left: IntegerRangeConstraint,
    right: IntegerRangeConstraint,
) -> IntegerRangeConstraint {
    match (left, right) {
        (IntegerRangeConstraint::Any, constraint) | (constraint, IntegerRangeConstraint::Any) => {
            constraint
        }
        (IntegerRangeConstraint::Maximum(left), IntegerRangeConstraint::Maximum(right)) => {
            IntegerRangeConstraint::Maximum(left.min(right))
        }
    }
}

fn union_integer_constraints(
    left: IntegerRangeConstraint,
    right: IntegerRangeConstraint,
) -> IntegerRangeConstraint {
    match (left, right) {
        (IntegerRangeConstraint::Any, _) | (_, IntegerRangeConstraint::Any) => {
            IntegerRangeConstraint::Any
        }
        (IntegerRangeConstraint::Maximum(left), IntegerRangeConstraint::Maximum(right)) => {
            IntegerRangeConstraint::Maximum(left.max(right))
        }
    }
}
