//! A concrete guard evaluator for semantic predicate comparisons.
//!
//! Predicates are compared by what they decide on values documents, never by syntax: two
//! normal forms of one condition must agree on every document of a representative product,
//! and the expected predicate must reproduce Helm's recorded verdicts.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Guard, GuardValue, Predicate, PredicateKind, ValuesPath};

/// The value at a literal values path, or `None` when a segment is missing.
pub(crate) fn value_at<'a>(
    document: &'a serde_json::Value,
    path: &ValuesPath,
) -> eyre::Result<Option<&'a serde_json::Value>> {
    let mut value = Some(document);
    for segment in path.segments() {
        let key = segment
            .literal()
            .ok_or_eyre("the guard oracle evaluates literal paths only")?;
        value = value.and_then(|value| value.get(key));
    }
    Ok(value)
}

/// Helm truthiness: nil, false, zero, and empty strings and collections are falsy.
pub(crate) fn truthy(value: Option<&serde_json::Value>) -> bool {
    match value {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(value)) => *value,
        Some(serde_json::Value::Number(value)) => value.as_f64().is_some_and(|value| value != 0.0),
        Some(serde_json::Value::String(value)) => !value.is_empty(),
        Some(serde_json::Value::Array(value)) => !value.is_empty(),
        Some(serde_json::Value::Object(value)) => !value.is_empty(),
    }
}

fn equals(actual: Option<&serde_json::Value>, literal: &GuardValue) -> bool {
    match literal {
        GuardValue::Null => actual.is_none_or(serde_json::Value::is_null),
        GuardValue::String(literal) => actual.and_then(serde_json::Value::as_str) == Some(literal),
        GuardValue::Bool(literal) => actual.and_then(serde_json::Value::as_bool) == Some(*literal),
        GuardValue::Int(literal) => actual.and_then(serde_json::Value::as_i64) == Some(*literal),
        GuardValue::Float(literal) => {
            let literal = literal.parse::<f64>().ok();
            actual
                .and_then(serde_json::Value::as_f64)
                .is_some_and(|actual| Some(actual) == literal)
        }
    }
}

fn has_type(value: Option<&serde_json::Value>, schema_type: &str) -> eyre::Result<bool> {
    Ok(match schema_type {
        "string" => value.is_some_and(serde_json::Value::is_string),
        "boolean" => value.is_some_and(serde_json::Value::is_boolean),
        "number" => value.is_some_and(serde_json::Value::is_number),
        "integer" => value
            .and_then(serde_json::Value::as_f64)
            .is_some_and(|value| value.fract() == 0.0),
        "object" => value.is_some_and(serde_json::Value::is_object),
        "array" => value.is_some_and(serde_json::Value::is_array),
        "null" => value.is_some_and(serde_json::Value::is_null),
        other => eyre::bail!("unsupported schema type: {other}"),
    })
}

fn matches(value: Option<&serde_json::Value>, pattern: &str) -> eyre::Result<Option<bool>> {
    // Guard patterns use JSON Schema's ECMAScript dialect, look-arounds included, so a Draft 7
    // `pattern` validator decides them.
    let Some(text) = value.filter(|value| value.is_string()) else {
        return Ok(None);
    };
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .build(&serde_json::json!({ "pattern": pattern }))
        .map_err(|error| eyre::eyre!("invalid guard pattern {pattern}: {error}"))?;
    Ok(Some(validator.is_valid(text)))
}

/// Whether one guard holds on a values document.
pub(crate) fn guard_holds(guard: &Guard, document: &serde_json::Value) -> eyre::Result<bool> {
    match guard {
        Guard::Truthy { path } => Ok(truthy(value_at(document, path)?)),
        Guard::Not { path } => Ok(!truthy(value_at(document, path)?)),
        Guard::Eq { path, value } => Ok(equals(value_at(document, path)?, value)),
        Guard::NotEq { path, value } => Ok(!equals(value_at(document, path)?, value)),
        Guard::Absent { path } => Ok(value_at(document, path)?.is_none()),
        Guard::TypeIs { path, schema_type } => has_type(value_at(document, path)?, schema_type),
        Guard::NotTypeIs { path, schema_type } => {
            Ok(!has_type(value_at(document, path)?, schema_type)?)
        }
        Guard::MatchesPattern {
            path,
            pattern,
            templated,
        } => {
            let value = value_at(document, path)?;
            let action = *templated
                && value
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|text| text.contains("{{"));
            Ok(action || matches(value, pattern)?.unwrap_or(false))
        }
        Guard::NotMatchesPattern { path, pattern } => {
            Ok(matches(value_at(document, path)?, pattern)? == Some(false))
        }
        Guard::Or { paths } => {
            for path in paths {
                if truthy(value_at(document, path)?) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Guard::AnyOf { alternatives } => {
            for alternative in alternatives {
                let mut holds = true;
                for guard in alternative {
                    holds &= guard_holds(guard, document)?;
                }
                if holds {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Guard::MatchesValue { path, pattern } => match value_at(document, path)? {
            None => Ok(false),
            Some(value) => pattern
                .matches(value)
                .ok_or_eyre("an invalid value pattern has no verdict"),
        },
        Guard::CollectionPattern {
            path: _,
            domain: _,
            quantifier: _,
            pattern: _,
        }
        | Guard::RangeKeyPrefix { path: _, prefix: _ }
        | Guard::RangeKeyEquals { path: _, key: _ }
        | Guard::RangeKeyMatches {
            path: _,
            pattern: _,
        }
        | Guard::Range { path: _ }
        | Guard::With { path: _ }
        | Guard::Default { path: _ }
        | Guard::IntGt { path: _, bound: _ }
        | Guard::IntLt { path: _, bound: _ }
        | Guard::AtMostOneMember { path: _ }
        | Guard::MinMembers { path: _, bound: _ }
        | Guard::HasKey { path: _, key: _ }
        | Guard::NotHasKey { path: _, key: _ }
        | Guard::ContainsEquals { path: _, value: _ }
        | Guard::ContainsMemberEquals {
            path: _,
            member: _,
            value: _,
        }
        | Guard::ContainsTruthyMember { path: _, member: _ } => {
            eyre::bail!("the guard oracle does not evaluate {guard:?}")
        }
    }
}

/// Whether a predicate holds on a values document; approximations are refused.
pub(crate) fn predicate_holds(
    predicate: &Predicate,
    document: &serde_json::Value,
) -> eyre::Result<bool> {
    match predicate.kind() {
        PredicateKind::True => Ok(true),
        PredicateKind::False => Ok(false),
        PredicateKind::Guard(guard) => guard_holds(guard, document),
        PredicateKind::Not(inner) => Ok(!predicate_holds(inner, document)?),
        PredicateKind::And(parts) => {
            for part in parts {
                if !predicate_holds(part, document)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        PredicateKind::Or(parts) => {
            for part in parts {
                if predicate_holds(part, document)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        PredicateKind::Approximate { .. } => {
            eyre::bail!("the guard oracle refuses approximations: {predicate:?}")
        }
    }
}

/// Representative leaf values: absence, null, every truthiness class of every kind, and the
/// given literals (the strings a predicate compares against).
pub(crate) fn representatives(literals: &[&str]) -> Vec<Option<serde_json::Value>> {
    let mut values = vec![
        None,
        Some(serde_json::Value::Null),
        Some(serde_json::json!(false)),
        Some(serde_json::json!(true)),
        Some(serde_json::json!(0)),
        Some(serde_json::json!(1)),
        Some(serde_json::json!("")),
        Some(serde_json::json!("other")),
        Some(serde_json::json!([])),
        Some(serde_json::json!([1])),
        Some(serde_json::json!({})),
        Some(serde_json::json!({"key": 1})),
    ];
    values.extend(
        literals
            .iter()
            .map(|literal| Some(serde_json::json!(literal))),
    );
    values
}

fn insert(
    document: &mut serde_json::Value,
    path: &ValuesPath,
    value: &serde_json::Value,
) -> eyre::Result<()> {
    let mut current = document;
    let segments = path.segments().collect::<Vec<_>>();
    let (last, parents) = segments
        .split_last()
        .ok_or_eyre("the guard oracle cannot set the values root")?;
    for segment in parents {
        let key = segment.literal().ok_or_eyre("literal parent segment")?;
        let object = current
            .as_object_mut()
            .ok_or_eyre("parent segment is not an object")?;
        current = object
            .entry(key.to_owned())
            .or_insert_with(|| serde_json::json!({}));
    }
    let key = last.literal().ok_or_eyre("literal leaf segment")?;
    current
        .as_object_mut()
        .ok_or_eyre("leaf parent is not an object")?
        .insert(key.to_owned(), value.clone());
    Ok(())
}

/// Every document assigning one representative to each leaf path.
pub(crate) fn product_documents(
    paths: &[ValuesPath],
    leaves: &[Option<serde_json::Value>],
) -> eyre::Result<Vec<serde_json::Value>> {
    let mut documents = vec![serde_json::json!({})];
    for path in paths {
        let mut next = Vec::with_capacity(documents.len() * leaves.len());
        for document in &documents {
            for leaf in leaves {
                let mut document = document.clone();
                if let Some(leaf) = leaf {
                    insert(&mut document, path, leaf)?;
                }
                next.push(document);
            }
        }
        documents = next;
    }
    Ok(documents)
}

/// Two predicates decide every document of the product of `leaves` over their paths alike.
pub(crate) fn assert_equivalent(
    have: &Predicate,
    want: &Predicate,
    leaves: &[Option<serde_json::Value>],
) -> eyre::Result<()> {
    let paths = have
        .value_paths()
        .union(&want.value_paths())
        .cloned()
        .collect::<Vec<_>>();
    for document in product_documents(&paths, leaves)? {
        let actual = predicate_holds(have, &document)?;
        let expected = predicate_holds(want, &document)?;
        if actual != expected {
            eyre::bail!(
                "predicates disagree on {document}: have {actual}, want {expected}\nhave: {have:?}\nwant: {want:?}"
            );
        }
    }
    Ok(())
}

/// The predicate reproduces Helm's recorded verdict on each document.
pub(crate) fn assert_helm_verdicts(
    predicate: &Predicate,
    verdicts: &[(serde_json::Value, bool)],
) -> eyre::Result<()> {
    for (document, helm) in verdicts {
        let holds = predicate_holds(predicate, document)?;
        if holds != *helm {
            eyre::bail!("Helm decides {helm} on {document}, the predicate {holds}: {predicate:?}");
        }
    }
    Ok(())
}

/// A guard disjunctive normal form as one predicate.
pub(crate) fn dnf_predicate(dnf: &helm_schema_core::GuardDnf) -> Predicate {
    Predicate::Or(
        dnf.disjuncts()
            .iter()
            .map(|conjunction| Predicate::And(conjunction.iter().cloned().collect()))
            .collect(),
    )
}

/// A guard conjunction as one predicate.
pub(crate) fn conjunction(guards: &[Guard]) -> Predicate {
    Predicate::all(guards.iter().cloned().map(Predicate::from).collect())
}
