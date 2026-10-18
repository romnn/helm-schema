//! Schema-directed constraint and instance identity, separate from diagnostic paths.

use std::collections::{BTreeMap, BTreeSet};
use std::iter::Peekable;
use std::str::Split;

use color_eyre::eyre::{self, OptionExt as _};
use jsonschema::paths::Location;
use serde_json::Value;

pub(crate) fn verify_registry_integrity(
    schema: &Value,
    draft: jsonschema::Draft,
    root_uri: &str,
) -> eyre::Result<()> {
    struct Pending<'a> {
        schema: &'a Value,
        draft: jsonschema::Draft,
        base: jsonschema::Uri<String>,
    }
    let retrieval = jsonschema::uri::from_str(root_uri)?;
    let mut identities = BTreeSet::from([retrieval.to_string()]);
    let mut pending = vec![Pending {
        schema,
        draft,
        base: retrieval.clone(),
    }];
    while let Some(Pending {
        schema: node,
        draft,
        mut base,
    }) = pending.pop()
    {
        eyre::ensure!(
            draft != jsonschema::Draft::Unknown,
            "unproved schema dialect"
        );
        if let Some(id) = draft.create_resource_ref(node).id() {
            base = jsonschema::uri::resolve_against(&base.borrow(), id)?;
            eyre::ensure!(
                base.fragment()
                    .map(jsonschema::uri::EncodedString::as_str)
                    .is_none_or(str::is_empty),
                "unproved fragment-bearing schema resource ID: {base}"
            );
            base = base.strip_fragment().to_owned();
            eyre::ensure!(
                identities.insert(base.to_string())
                    || (std::ptr::eq(node, schema) && base == retrieval),
                "registry identity collision: {base}"
            );
        }
        let anchors: Vec<&str> = match draft {
            jsonschema::Draft::Draft4 => node
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| id.strip_prefix('#'))
                .into_iter()
                .collect(),
            jsonschema::Draft::Draft6 | jsonschema::Draft::Draft7 => node
                .get("$id")
                .and_then(Value::as_str)
                .and_then(|id| id.strip_prefix('#'))
                .into_iter()
                .collect(),
            jsonschema::Draft::Draft201909 => node
                .get("$anchor")
                .and_then(Value::as_str)
                .into_iter()
                .collect(),
            _ => ["$anchor", "$dynamicAnchor"]
                .into_iter()
                .filter_map(|keyword| node.get(keyword).and_then(Value::as_str))
                .collect(),
        };
        for anchor in anchors {
            let identity = jsonschema::uri::resolve_against(&base.borrow(), &format!("#{anchor}"))?;
            let identity = if anchor.is_empty() {
                identity.strip_fragment().to_string()
            } else {
                identity.to_string()
            };
            eyre::ensure!(
                identities.insert(identity.clone()),
                "registry identity collision: {identity}"
            );
        }
        for child in draft.subresources_of(node) {
            pending.push(Pending {
                schema: child,
                draft: draft.detect(child),
                base: base.clone(),
            });
        }
    }
    Ok(())
}

struct CompilerPosition<'a> {
    node: &'a Value,
    draft: jsonschema::Draft,
    base: jsonschema::Uri<String>,
    pointer: String,
    resource_base: String,
}
struct CompilerSource<'a> {
    node: &'a Value,
    draft: jsonschema::Draft,
    resource_base: String,
}

pub(super) fn verify_compiler_locations(
    registry: &jsonschema::Registry<'_>,
    root_uri: &str,
) -> eyre::Result<()> {
    let resolver = registry.resolver(jsonschema::uri::from_str(root_uri)?);
    let (root, resolver, draft) = resolver.lookup(root_uri)?.into_inner();
    let resolver = resolver.in_subresource(draft.create_resource_ref(root))?;
    let mut pending = vec![CompilerPosition {
        node: root,
        draft,
        base: resolver.base_uri().as_ref().clone(),
        pointer: String::new(),
        resource_base: String::new(),
    }];
    // The compiler cache omits source identity and the reference base used to store nested suffixes.
    // Ignoring dynamic scope is conservative: a collision never becomes permission to disambiguate.
    let mut locations = BTreeMap::<(String, String), CompilerSource<'_>>::new();
    while let Some(CompilerPosition {
        node,
        draft,
        base,
        pointer,
        resource_base,
    }) = pending.pop()
    {
        if let Some(previous) = locations.insert(
            (base.to_string(), pointer.clone()),
            CompilerSource {
                node,
                draft,
                resource_base: resource_base.clone(),
            },
        ) {
            eyre::ensure!(
                std::ptr::eq(previous.node, node)
                    && previous.draft == draft
                    && previous.resource_base == resource_base,
                "compiler location collision: {base}#{pointer}"
            );
            continue;
        }
        eyre::ensure!(
            locations.len() <= 100_000,
            "compiler provenance proof exceeds its position budget"
        );
        eyre::ensure!(
            node.get("$dynamicRef").is_none() && node.get("$recursiveRef").is_none(),
            "unproved dynamic reference correspondence"
        );
        if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
            let alias = jsonschema::uri::resolve_against(&base.borrow(), reference)?;
            let target_pointer = match alias.fragment() {
                Some(fragment) if fragment.as_str().starts_with('/') => fragment
                    .decode()
                    .to_string()
                    .map_err(|_| eyre::eyre!("reference fragment is not UTF-8"))?
                    .into_owned(),
                _ => String::new(),
            };
            let (target, target_resolver, target_draft) = registry
                .resolver(base.clone())
                .lookup(reference)?
                .into_inner();
            pending.push(CompilerPosition {
                node: target,
                draft: target_draft,
                base: target_resolver.base_uri().as_ref().clone(),
                resource_base: target_pointer.clone(),
                pointer: target_pointer,
            });
        }
        for child in draft.subresources_of(node) {
            let suffix = schema_node_pointer(node, child, Location::new())
                .ok_or_eyre("schema child has no source position")?;
            // Definitions compile only when referenced; their declaration path is not a compiler address.
            if matches!(
                suffix.as_str().split('/').nth(1),
                Some("definitions" | "$defs")
            ) {
                continue;
            }
            let child_draft = draft.detect(child);
            let child_base = match child_draft.create_resource_ref(child).id() {
                Some(id) => jsonschema::uri::resolve_against(&base.borrow(), id)?,
                None => base.clone(),
            };
            pending.push(CompilerPosition {
                node: child,
                draft: child_draft,
                base: child_base,
                pointer: format!("{pointer}{suffix}"),
                resource_base: resource_base.clone(),
            });
        }
    }
    Ok(())
}

pub(super) fn assertion_provenance<'a, 'i>(
    error: &'a jsonschema::ValidationError<'i>,
) -> &'a jsonschema::ValidationError<'i> {
    // propertyNames rebuilds the outer route from a child-relative path; the child retains its tracker.
    match error.kind() {
        jsonschema::error::ValidationErrorKind::PropertyNames { error } => {
            assertion_provenance(error)
        }
        _ => error,
    }
}

pub(crate) fn canonical_schema_location(
    registry: &jsonschema::Registry<'_>,
    root_uri: &str,
    error: &jsonschema::ValidationError<'_>,
) -> eyre::Result<String> {
    use jsonschema::error::ValidationErrorKind;

    eyre::ensure!(
        !matches!(
            error.kind(),
            ValidationErrorKind::FalseSchema
                | ValidationErrorKind::Referencing(_)
                | ValidationErrorKind::BacktrackLimitExceeded { .. }
                | ValidationErrorKind::RegexEngineFailure { .. }
        ) && error.evaluation_path().as_str().rsplit('/').next() == Some(error.kind().keyword()),
        "{} does not prove a complete constraint route",
        error.kind().keyword()
    );
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
    if document.pointer(error.instance_path().as_str()) != Some(error.instance().as_ref()) {
        return Err("reported instance does not match its path".to_string());
    }
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
    let mut route = assertion_provenance(error)
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
