//! Canonical contextual assertion identity, separate from concrete diagnostic paths.

use std::iter::Peekable;
use std::str::Split;

use serde_json::Value;

use super::adjudication::ViolationKey;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum InstanceSegment {
    Fixed(String),
    Wildcard,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CanonicalViolationKey {
    assertion: ViolationKey,
    instance: Vec<InstanceSegment>,
}

impl CanonicalViolationKey {
    pub(super) fn new(key: &ViolationKey, path: Option<&[InstanceSegment]>) -> Self {
        let instance = path.map_or_else(
            || {
                key.instance_path
                    .split('/')
                    .skip(1)
                    .map(|token| InstanceSegment::Fixed(token.to_string()))
                    .collect()
            },
            <[InstanceSegment]>::to_vec,
        );
        let mut assertion = key.clone();
        assertion.instance_path.clear();
        if !matches!(
            assertion.keyword.as_str(),
            "required" | "additionalProperties" | "unevaluatedProperties" | "propertyNames"
        ) {
            assertion.detail.clear();
        }
        Self {
            assertion,
            instance,
        }
    }
}

#[derive(Clone, Copy)]
enum InstanceStep<'a> {
    Property(&'a str),
    MapValue,
    ArrayItem,
}

struct InstanceCursor<'a> {
    document: &'a Value,
    tokens: Peekable<std::iter::Skip<Split<'a, char>>>,
    concrete: String,
    canonical: Vec<InstanceSegment>,
}

impl InstanceCursor<'_> {
    fn consume(&mut self, step: InstanceStep<'_>) -> Result<(), String> {
        let token = self
            .tokens
            .next()
            .ok_or("schema route exceeds instance path")?;
        let container = self
            .document
            .pointer(&self.concrete)
            .ok_or("instance container missing")?;
        let wildcard = match step {
            InstanceStep::Property(expected) if container.is_object() && token == expected => false,
            InstanceStep::MapValue if container.is_object() => true,
            InstanceStep::ArrayItem if container.is_array() => true,
            _ => return Err("schema route does not identify the instance position".to_string()),
        };
        self.concrete.push('/');
        self.concrete.push_str(token);
        if self.document.pointer(&self.concrete).is_none() {
            return Err("schema route identifies no instance value".to_string());
        }
        self.canonical.push(if wildcard {
            InstanceSegment::Wildcard
        } else {
            InstanceSegment::Fixed(token.to_string())
        });
        Ok(())
    }
}

pub(super) fn canonical_instance_path(
    document: &Value,
    error: &jsonschema::ValidationError<'_>,
    diagnostic_path: &str,
) -> Result<Vec<InstanceSegment>, String> {
    // This grouped error conflates forbidden names with failures of permitted member values.
    if matches!(
        error.kind(),
        jsonschema::error::ValidationErrorKind::UnevaluatedProperties { .. }
    ) {
        return Err("unevaluatedProperties does not prove member correspondence".to_string());
    }
    // Validator paths are escaped JSON pointers.
    // Keep empty and numeric property tokens intact;
    // only the evaluated keyword route and actual JSON container establish their roles.
    let mut route = error
        .evaluation_path()
        .as_str()
        .split('/')
        .skip(1)
        .peekable();
    let mut instance = InstanceCursor {
        document,
        tokens: error.instance_path().as_str().split('/').skip(1).peekable(),
        concrete: String::new(),
        canonical: Vec::new(),
    };
    while let Some(keyword) = route.next() {
        match keyword {
            "properties" => {
                let property = route.next().ok_or("missing schema property name")?;
                instance.consume(InstanceStep::Property(property))?;
            }
            "patternProperties" => {
                route.next().ok_or("missing schema property pattern")?;
                instance.consume(InstanceStep::MapValue)?;
            }
            "additionalProperties" => {
                if instance.tokens.peek().is_some() {
                    instance.consume(InstanceStep::MapValue)?;
                }
            }
            "items" | "prefixItems" | "additionalItems" | "unevaluatedItems" | "contains" => {
                if route
                    .peek()
                    .is_some_and(|token| token.parse::<usize>().is_ok())
                {
                    route.next();
                }
                if instance.tokens.peek().is_some() {
                    instance.consume(InstanceStep::ArrayItem)?;
                }
            }
            "allOf" | "anyOf" | "oneOf" => {
                if let Some(index) = route.next()
                    && index.parse::<usize>().is_err()
                {
                    return Err("invalid schema applicator index".to_string());
                }
            }
            "dependencies" | "dependentSchemas" => {
                route.next().ok_or("missing schema dependency name")?;
            }
            "$ref" | "$dynamicRef" | "$recursiveRef" | "if" | "then" | "else" | "not"
            | "propertyNames" => {}
            _ if route.peek().is_none() => {}
            _ => return Err(format!("unsupported schema route keyword {keyword:?}")),
        }
    }
    if instance.tokens.next().is_some() {
        return Err("schema route leaves an ambiguous instance position".to_string());
    }
    let suffix = diagnostic_path
        .strip_prefix(error.instance_path().as_str())
        .ok_or("structural violation does not extend the assertion instance")?;
    if !suffix.is_empty() && !suffix.starts_with('/') {
        return Err("structural violation has an ambiguous member path".to_string());
    }
    // Forbidden members and invalid property names are structural identities, never wildcards.
    instance.canonical.extend(
        suffix
            .split('/')
            .skip(1)
            .map(|token| InstanceSegment::Fixed(token.to_string())),
    );
    Ok(instance.canonical)
}
