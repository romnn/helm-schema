use serde_json::Value;

#[derive(Clone, Copy)]
struct TypeDomain {
    possible: bool,
    universal: bool,
}

impl TypeDomain {
    const UNKNOWN: Self = Self {
        possible: true,
        universal: false,
    };
    const ALL: Self = Self {
        possible: true,
        universal: true,
    };
    const NONE: Self = Self {
        possible: false,
        universal: false,
    };

    fn intersect(self, other: Self) -> Self {
        Self {
            possible: self.possible && other.possible,
            universal: self.universal && other.universal,
        }
    }
}

pub(super) fn top_type_preimage(
    schema: &Value,
    kind: helm_schema_core::ValueKind,
    placement: helm_schema_core::PlainScalarPlacement,
) -> Value {
    let (output_type, source) = match kind {
        helm_schema_core::ValueKind::GoFormattedMap => ("string", safe_mapping(2)),
        helm_schema_core::ValueKind::GoFormattedSequence => ("array", safe_sequence(2)),
        helm_schema_core::ValueKind::Scalar
        | helm_schema_core::ValueKind::PartialScalar
        | helm_schema_core::ValueKind::Fragment
        | helm_schema_core::ValueKind::Serialized
        | helm_schema_core::ValueKind::YamlSerialized
        | helm_schema_core::ValueKind::TemplatedYamlSerialized
        | helm_schema_core::ValueKind::WidenedDependency
        | helm_schema_core::ValueKind::WidenedRegion => return super::empty_schema(),
    };
    let mut constraints = Vec::new();
    if !may_allow_type(schema, output_type) {
        constraints.push(serde_json::json!({ "not": source }));
    }
    if kind == helm_schema_core::ValueKind::GoFormattedMap
        && placement == helm_schema_core::PlainScalarPlacement::BlockValue
    {
        constraints.push(serde_json::json!({ "not": invalid_singleton_mapping() }));
    }
    match constraints.as_slice() {
        [] => super::empty_schema(),
        [constraint] => constraint.clone(),
        _ => serde_json::json!({ "allOf": constraints }),
    }
}

fn invalid_singleton_mapping() -> Value {
    // One benign key/value word surrounds a colon separator; no sibling or escaping can shield it.
    serde_json::json!({
        "type": "object",
        "minProperties": 1,
        "maxProperties": 1,
        "propertyNames": {
            "type": "string",
            "minLength": 1,
            "not": { "pattern": "[^A-Za-z0-9_.$/+-]" },
        },
        "additionalProperties": {
            "type": "string",
            "pattern": "^[A-Za-z0-9_.$/+-]+:[ \t]+[A-Za-z0-9_.$/+-]+$",
            "not": { "pattern": "[^A-Za-z0-9_.$/+: \t-]" },
        },
    })
}

fn safe_string() -> Value {
    serde_json::json!({ "type": "string", "not": { "pattern": "[^A-Za-z0-9_./+-]" } })
}

fn safe_mapping(depth: usize) -> Value {
    serde_json::json!({
        "type": "object",
        "propertyNames": safe_string(),
        "additionalProperties": safe_value(depth.saturating_sub(1)),
    })
}

fn safe_sequence(depth: usize) -> Value {
    serde_json::json!({ "type": "array", "items": safe_value(depth.saturating_sub(1)) })
}

fn safe_value(depth: usize) -> Value {
    let mut alternatives = vec![
        serde_json::json!({ "type": ["null", "boolean", "number"] }),
        safe_string(),
    ];
    if depth != 0 {
        alternatives.push(safe_mapping(depth));
        alternatives.push(safe_sequence(depth));
    }
    serde_json::json!({ "anyOf": alternatives })
}

pub(super) fn may_allow_type(schema: &Value, output_type: &str) -> bool {
    type_domain(schema, output_type, &mut 256).possible
}

fn type_domain(schema: &Value, output_type: &str, remaining: &mut usize) -> TypeDomain {
    if *remaining == 0 {
        return TypeDomain::UNKNOWN;
    }
    *remaining -= 1;
    match schema {
        Value::Bool(true) => TypeDomain::ALL,
        Value::Bool(false) => TypeDomain::NONE,
        Value::Object(object) => {
            let mut domain = TypeDomain::ALL;
            for (keyword, value) in object {
                domain = domain.intersect(keyword_domain(keyword, value, output_type, remaining));
                if !domain.possible {
                    return domain;
                }
            }
            domain
        }
        Value::Null | Value::Number(_) | Value::String(_) | Value::Array(_) => TypeDomain::UNKNOWN,
    }
}

fn keyword_domain(
    keyword: &str,
    value: &Value,
    output_type: &str,
    remaining: &mut usize,
) -> TypeDomain {
    match keyword {
        "type" => match value {
            Value::String(kind) => allowed_type(kind == output_type),
            Value::Array(kinds) if kinds.iter().all(Value::is_string) => {
                allowed_type(kinds.iter().any(|kind| kind.as_str() == Some(output_type)))
            }
            _ => TypeDomain::UNKNOWN,
        },
        "const" => finite_domain(std::iter::once(value), output_type),
        "enum" => value.as_array().map_or(TypeDomain::UNKNOWN, |values| {
            finite_domain(values.iter(), output_type)
        }),
        "not" => {
            let inner = type_domain(value, output_type, remaining);
            TypeDomain {
                possible: !inner.universal,
                universal: !inner.possible,
            }
        }
        "allOf" | "anyOf" | "oneOf" => composite_domain(keyword, value, output_type, remaining),
        name if super::is_annotation_keyword(name)
            || matches!(name, "$schema" | "$id" | "$defs" | "definitions") =>
        {
            TypeDomain::ALL
        }
        _ => TypeDomain::UNKNOWN,
    }
}

fn allowed_type(allowed: bool) -> TypeDomain {
    if allowed {
        TypeDomain::ALL
    } else {
        TypeDomain::NONE
    }
}

fn finite_domain<'value>(
    mut values: impl Iterator<Item = &'value Value>,
    output_type: &str,
) -> TypeDomain {
    TypeDomain {
        possible: values.any(|value| match output_type {
            "string" => value.is_string(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => true,
        }),
        universal: false,
    }
}

fn composite_domain(
    keyword: &str,
    value: &Value,
    output_type: &str,
    remaining: &mut usize,
) -> TypeDomain {
    let Some(schemas) = value.as_array() else {
        return TypeDomain::UNKNOWN;
    };
    let domains: Vec<_> = schemas
        .iter()
        .map(|schema| type_domain(schema, output_type, remaining))
        .collect();
    match keyword {
        "allOf" => domains
            .into_iter()
            .fold(TypeDomain::ALL, TypeDomain::intersect),
        "anyOf" => TypeDomain {
            possible: domains.iter().any(|domain| domain.possible),
            universal: domains.iter().any(|domain| domain.universal),
        },
        "oneOf" => TypeDomain {
            possible: domains.iter().any(|domain| domain.possible),
            universal: domains.iter().filter(|domain| domain.universal).count() == 1
                && domains.iter().filter(|domain| domain.possible).count() == 1,
        },
        _ => TypeDomain::UNKNOWN,
    }
}

#[cfg(test)]
#[path = "../tests/go_format_singleton.rs"]
mod singleton_tests;
