//! Schema-directed constraint and instance identity, separate from diagnostic paths.

use std::iter::Peekable;
use std::str::Split;

use color_eyre::eyre::{self, OptionExt as _};
use jsonschema::paths::Location;
use serde_json::Value;

pub(super) fn canonical_schema_location(
    registry: &jsonschema::Registry<'_>,
    root_uri: &str,
    error: &jsonschema::ValidationError<'_>,
) -> eyre::Result<String> {
    let reported = error
        .absolute_keyword_location()
        .ok_or_eyre("missing absolute keyword location")?;
    let root_resolver = registry.resolver(jsonschema::uri::from_str(root_uri)?);
    let (root, root_resolver, root_draft) = root_resolver.lookup(root_uri)?.into_inner();
    let root_resolver = root_resolver.in_subresource(root_draft.create_resource_ref(root))?;
    let (mut node, mut resolver, mut draft) = (root, root_resolver.clone(), root_draft);
    let mut schema = node;
    for token in error.evaluation_path().as_str().split('/').skip(1) {
        let child = node
            .pointer(&format!("/{token}"))
            .ok_or_eyre("evaluation route does not identify a schema node")?;
        if std::ptr::eq(node, schema) && matches!(token, "$ref" | "$dynamicRef" | "$recursiveRef") {
            eyre::ensure!(
                token == "$ref",
                "dynamic reference correspondence is unproved"
            );
            let reference = child.as_str().ok_or_eyre("reference is not a string")?;
            (node, resolver, draft) = resolver.lookup(reference)?.into_inner();
            schema = node;
        } else {
            node = child;
            if draft
                .subresources_of(schema)
                .any(|child| std::ptr::eq(child, node))
            {
                draft = draft.detect(node);
                resolver = resolver.in_subresource(draft.create_resource_ref(node))?;
                schema = node;
            }
        }
    }
    eyre::ensure!(
        reported.strip_fragment() == resolver.base_uri().strip_fragment(),
        "reported keyword resource disagrees with its resolved evaluation route"
    );
    // Reference lookup can retain either a retrieval alias or the declared resource ID.
    // Derive one location from the node's source origin, independent of that choice.
    let origin = schema_node_pointer(root, node, Location::new())
        .ok_or_eyre("constraint has no proven origin in the pinned schema")?;
    (node, resolver, draft) = (root, root_resolver, root_draft);
    schema = root;
    let mut pointer = Location::new();
    for token in origin.as_str().split('/').skip(1) {
        node = node
            .pointer(&format!("/{token}"))
            .ok_or_eyre("schema origin is missing")?;
        pointer = pointer.join(token.replace("~1", "/").replace("~0", "~").as_str());
        if draft
            .subresources_of(schema)
            .any(|child| std::ptr::eq(child, node))
        {
            draft = draft.detect(node);
            let resource = draft.create_resource_ref(node);
            if resource.id().is_some() {
                resolver = resolver.in_subresource(resource)?;
                pointer = Location::new();
            }
            schema = node;
        }
    }
    let resource_uri = resolver.base_uri();
    let mut fragment = jsonschema::uri::EncodedBuffer::new();
    fragment.encode_str::<jsonschema::uri::Path>(pointer.as_str());
    let location = resource_uri.with_fragment(Some(fragment.as_estr()));
    eyre::ensure!(
        std::ptr::eq(resolver.lookup(location.as_str())?.contents(), node),
        "canonical keyword location does not identify the resolved schema node"
    );
    Ok(location.to_string())
}

fn schema_node_pointer(root: &Value, target: &Value, pointer: Location) -> Option<Location> {
    if std::ptr::eq(root, target) {
        return Some(pointer);
    }
    match root {
        Value::Object(properties) => {
            for (name, child) in properties {
                if let Some(found) = schema_node_pointer(child, target, pointer.join(name.as_str()))
                {
                    return Some(found);
                }
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                if let Some(found) = schema_node_pointer(child, target, pointer.join(index)) {
                    return Some(found);
                }
            }
        }
        _ => {}
    }
    None
}

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
