//! Schema-directed instance identity, separate from concrete diagnostic paths.

use std::iter::Peekable;
use std::str::Split;

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum InstanceSegment {
    Fixed(String),
    Wildcard,
}

#[derive(Clone, Copy)]
enum InstanceStep<'a> {
    Property(&'a str),
    MapValue,
    ArrayItem,
    TupleItem(&'a str),
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
            InstanceStep::TupleItem(expected) if container.is_array() && token == expected => false,
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
    // These grouped errors do not identify which concrete members failed.
    if matches!(
        error.kind(),
        jsonschema::error::ValidationErrorKind::UnevaluatedProperties { .. }
            | jsonschema::error::ValidationErrorKind::UnevaluatedItems { .. }
    ) {
        return Err(format!(
            "{} does not prove member correspondence",
            error.kind().keyword()
        ));
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
                let step = if matches!(keyword, "items" | "prefixItems")
                    && route
                        .peek()
                        .is_some_and(|token| token.parse::<usize>().is_ok())
                {
                    InstanceStep::TupleItem(route.next().ok_or("missing tuple position")?)
                } else {
                    InstanceStep::ArrayItem
                };
                if instance.tokens.peek().is_some() {
                    instance.consume(step)?;
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
    // AdditionalItems identifies every forbidden tail member; property names stay structural.
    for token in suffix.split('/').skip(1) {
        instance.canonical.push(
            if matches!(
                error.kind(),
                jsonschema::error::ValidationErrorKind::AdditionalItems { .. }
            ) {
                InstanceSegment::Wildcard
            } else {
                InstanceSegment::Fixed(token.to_string())
            },
        );
    }
    Ok(instance.canonical)
}
