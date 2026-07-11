use serde_json::Value;

use helm_schema_core::{
    ConditionalGuard, ConditionalPathOverlay, ContractValuePathFacts, GuardValue,
    ProviderSchemaUse, ResourceSchemaOracle, ValueKind,
};
use serde_yaml::Value as YamlValue;

use crate::foreign_schema::ForeignSchemaRestriction;
use crate::merge::{merge_schema_list, merge_two_schemas, union_schema_list};
use crate::overlay_lowering::resolve_overlay_target_schema;
use crate::path_schema::{
    generalize_fixed_object_schema_to_open_map, merge_explicit_empty_placeholder,
    open_fragment_values_schema,
};
use crate::schema_model::{
    add_null_schema, empty_schema, empty_string_schema, guard_value_to_json, is_empty_schema,
    is_fixed_object_schema, is_object_or_array_schema, is_open_string_map_schema,
    is_scalar_like_schema, is_scalar_schema, schema_allows_type, schema_permits_empty_string,
    schema_type, type_schema,
};
use crate::schema_node::SchemaNode;
use crate::schema_node::is_placeholder_fragment_object_schema;
use crate::values_yaml::ValuesYamlPathFacts;
use crate::values_yaml::yaml_value_at_path;

/// Generator-side policy for lowering semantic value uses into schema evidence.
///
/// Decisions about provider-schema domains and guard-derived constraints live
/// here rather than being spread across root-schema construction.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ResolvePolicy;

/// Structural facts for one `.Values.*` path.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ValuePathSchemaFacts {
    pub(crate) contract: ContractValuePathFacts,
    pub(crate) values_yaml: ValuesYamlPathFacts,
}

impl ValuePathSchemaFacts {
    pub(crate) fn new(contract: ContractValuePathFacts, values_yaml: ValuesYamlPathFacts) -> Self {
        Self {
            contract,
            values_yaml,
        }
    }

    fn has_explicit_null_scalar_default(
        self,
        type_hint_schema: &Value,
        guard_predicate_schema: &Value,
    ) -> bool {
        self.values_yaml.is_explicit_null
            && (is_scalar_like_schema(guard_predicate_schema)
                || (!self.contract.has_render_use && is_scalar_like_schema(type_hint_schema)))
    }

    fn accepts_null_default(
        self,
        type_hint_schema: &Value,
        guard_predicate_schema: &Value,
    ) -> bool {
        self.contract.is_nullable
            || self.has_explicit_null_scalar_default(type_hint_schema, guard_predicate_schema)
    }

    fn preserve_explicit_null_default(
        self,
        type_hint_schema: &Value,
        guard_predicate_schema: &Value,
    ) -> bool {
        self.values_yaml.is_explicit_null
            && self.accepts_null_default(type_hint_schema, guard_predicate_schema)
    }

    fn preserve_empty_string_fallback(
        self,
        type_hint_schema: &Value,
        guard_predicate_schema: &Value,
    ) -> bool {
        self.values_yaml.is_empty_string
            && ((self.contract.has_render_use && self.contract.all_render_uses_self_guarded)
                || is_scalar_like_schema(type_hint_schema)
                || is_scalar_like_schema(guard_predicate_schema))
    }

    fn empty_map_placeholder_has_structural_object_use(self, provider_schema: &Value) -> bool {
        self.values_yaml.is_empty_map
            && (self.contract.is_ranged_source
                || self.contract.has_self_range_guard_render_use
                || (schema_allows_type(provider_schema, "object")
                    && (self.contract.used_as_fragment
                        || (self.contract.has_render_use
                            && self.contract.all_render_uses_self_guarded))))
    }
}

/// Inputs for one value-path schema decision.
///
/// These are the evidence streams collected for a single `.Values.*` path
/// before the policy decides which schemas to prefer, merge, or preserve.
pub(crate) struct ValuePathSchemaInputs {
    pub(crate) facts: ValuePathSchemaFacts,
    pub(crate) provider_schema: Value,
    pub(crate) values_yaml_schema: Value,
    pub(crate) guard_predicate_schema: Value,
    pub(crate) type_hint_schema: Value,
}

impl ResolvePolicy {
    pub(crate) fn provider_schema_for_value_use(
        &self,
        schema: &Value,
        use_: &ProviderSchemaUse,
    ) -> Option<Value> {
        match use_.kind {
            ValueKind::Fragment => Some(schema.clone()),
            ValueKind::PartialScalar => None,
            ValueKind::Scalar if use_.is_self_range_collection => {
                ForeignSchemaRestriction::ScalarCollection.apply(schema.clone())
            }
            ValueKind::Scalar => ForeignSchemaRestriction::Scalar.apply(schema.clone()),
        }
    }

    pub(crate) fn guard_predicate_schema(
        &self,
        value_path: &str,
        predicate: &ConditionalGuard,
    ) -> Option<Value> {
        match predicate {
            ConditionalGuard::Eq { path, value } if path == value_path => {
                if matches!(value, GuardValue::Null) {
                    return Some(empty_schema());
                }
                let value = guard_value_to_json(value)?;
                let value_type = schema_type_for_guard_value(&value)?;
                Some(
                    SchemaNode::any_of(vec![
                        SchemaNode::enum_values(vec![value]),
                        SchemaNode::type_named(value_type),
                    ])
                    .into_value(),
                )
            }
            ConditionalGuard::TypeIs { path, schema_type } if path == value_path => {
                match schema_type.as_str() {
                    "array" | "boolean" | "integer" | "number" | "object" | "string" => {
                        Some(type_schema(schema_type))
                    }
                    _ => None,
                }
            }
            ConditionalGuard::Truthy { .. }
            | ConditionalGuard::With { .. }
            | ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. }
            | ConditionalGuard::Not(_)
            | ConditionalGuard::AllOf(_)
            | ConditionalGuard::AnyOf(_) => None,
        }
    }

    pub(crate) fn resolve_schema_for_value_path(&self, input: ValuePathSchemaInputs) -> Value {
        let ValuePathSchemaInputs {
            facts,
            provider_schema,
            values_yaml_schema,
            guard_predicate_schema,
            type_hint_schema,
        } = input;
        let preserve_explicit_null_default_by_contract =
            facts.preserve_explicit_null_default(&type_hint_schema, &guard_predicate_schema);
        let preserve_empty_string_fallback =
            facts.preserve_empty_string_fallback(&type_hint_schema, &guard_predicate_schema);
        let values_yaml_schema = self.adjust_values_yaml_schema_for_value_path(
            values_yaml_schema,
            facts,
            &provider_schema,
        );
        let provider_schema = self.adjust_provider_schema_for_value_path(
            facts,
            provider_schema,
            &values_yaml_schema,
            &type_hint_schema,
            &guard_predicate_schema,
        );
        let partial_scalar_schema = self.partial_scalar_schema_for_value_path(
            facts,
            &provider_schema,
            &type_hint_schema,
            &guard_predicate_schema,
        );
        let guard_predicate_schema =
            merge_schema_list(vec![guard_predicate_schema, partial_scalar_schema]);
        let merged = self.resolve_merged_schema_for_value_path(
            ValuePathSchemaInputs {
                facts,
                provider_schema,
                values_yaml_schema,
                guard_predicate_schema,
                type_hint_schema,
            },
            preserve_empty_string_fallback,
        );
        let preserve_explicit_null_default = preserve_explicit_null_default_by_contract
            || (facts.values_yaml.is_explicit_null
                && facts.contract.used_as_fragment
                && !is_empty_schema(&merged));

        if (preserve_explicit_null_default
            || (is_scalar_like_schema(&merged) && facts.contract.is_nullable))
            && !is_empty_schema(&merged)
        {
            add_null_schema(merged)
        } else if preserve_explicit_null_default {
            type_schema("null")
        } else if facts.empty_map_placeholder_has_structural_object_use(&merged) {
            merge_explicit_empty_placeholder(
                merged,
                facts.values_yaml.is_empty_map,
                facts.contract.has_referenced_descendants,
            )
        } else {
            merged
        }
    }

    fn adjust_values_yaml_schema_for_value_path(
        &self,
        values_yaml_schema: Value,
        facts: ValuePathSchemaFacts,
        provider_schema: &Value,
    ) -> Value {
        let values_yaml_schema =
            if facts.empty_map_placeholder_has_structural_object_use(provider_schema) {
                empty_schema()
            } else {
                values_yaml_schema
            };
        let values_yaml_schema =
            if facts.contract.accepted_values_root_fragment && facts.values_yaml.is_mapping {
                values_yaml_schema
            } else if facts.contract.used_as_fragment
                && is_empty_schema(provider_schema)
                && should_open_fragment_values_schema(&values_yaml_schema, facts)
            {
                open_fragment_values_schema(values_yaml_schema)
            } else {
                values_yaml_schema
            };

        if facts.contract.is_ranged_source && facts.values_yaml.is_mapping {
            generalize_fixed_object_schema_to_open_map(values_yaml_schema)
        } else {
            values_yaml_schema
        }
    }

    fn adjust_provider_schema_for_value_path(
        &self,
        facts: ValuePathSchemaFacts,
        provider_schema: Value,
        values_yaml_schema: &Value,
        type_hint_schema: &Value,
        guard_predicate_schema: &Value,
    ) -> Value {
        if facts.contract.used_as_fragment
            && is_scalar_schema(values_yaml_schema)
            && (is_scalar_like_schema(type_hint_schema)
                || is_scalar_like_schema(guard_predicate_schema))
        {
            ForeignSchemaRestriction::Scalar
                .apply(provider_schema.clone())
                .unwrap_or(provider_schema)
        } else {
            provider_schema
        }
    }

    fn partial_scalar_schema_for_value_path(
        &self,
        facts: ValuePathSchemaFacts,
        provider_schema: &Value,
        type_hint_schema: &Value,
        guard_predicate_schema: &Value,
    ) -> Value {
        if facts.contract.is_partial_scalar_value_path
            && is_empty_schema(provider_schema)
            && is_empty_schema(type_hint_schema)
            && is_empty_schema(guard_predicate_schema)
            && facts.values_yaml.has_no_schema_evidence
        {
            type_schema("string")
        } else {
            empty_schema()
        }
    }

    fn resolve_merged_schema_for_value_path(
        &self,
        input: ValuePathSchemaInputs,
        preserve_empty_string_fallback: bool,
    ) -> Value {
        let base = if !is_empty_schema(&input.provider_schema) {
            if is_empty_schema(&input.values_yaml_schema) {
                input.provider_schema
            } else {
                // Some charts use scalar "preset" values that are fed into helpers which
                // expand into full K8s objects in the rendered manifest (e.g. affinity presets).
                // In these cases the *input* type in values.yaml is the scalar, not the output
                // object type, so prefer the values.yaml scalar schema.
                if input.facts.contract.has_referenced_descendants
                    && is_fixed_object_schema(&input.values_yaml_schema)
                    && is_scalar_schema(&input.provider_schema)
                {
                    input.values_yaml_schema
                } else if input.facts.contract.used_as_fragment
                    && is_fixed_object_schema(&input.values_yaml_schema)
                    && is_open_string_map_schema(&input.provider_schema)
                {
                    input.provider_schema
                } else if input.facts.contract.used_as_fragment
                    && is_scalar_schema(&input.values_yaml_schema)
                    && is_object_or_array_schema(&input.provider_schema)
                {
                    input.values_yaml_schema
                } else if let Some(values_yaml_ty) = schema_type(&input.values_yaml_schema)
                    && is_scalar_schema(&input.values_yaml_schema)
                    && schema_allows_type(&input.provider_schema, values_yaml_ty)
                {
                    if preserve_empty_string_fallback
                        && values_yaml_ty == "string"
                        && !schema_permits_empty_string(&input.provider_schema)
                    {
                        union_schema_list(vec![input.provider_schema, empty_string_schema()])
                    } else {
                        input.provider_schema
                    }
                } else {
                    merge_two_schemas(input.provider_schema, input.values_yaml_schema)
                }
            }
        } else if !is_empty_schema(&input.values_yaml_schema) {
            input.values_yaml_schema
        } else if input.facts.contract.used_as_fragment {
            SchemaNode::unknown_object().into_value()
        } else {
            empty_schema()
        };

        let base = merge_two_schemas(base, input.type_hint_schema);
        merge_two_schemas(base, input.guard_predicate_schema)
    }
}

/// The branch schema is the strongest available evidence schema that is not a
/// vacuous placeholder when real content exists and accepts the chart's
/// shipped default whenever the branch tolerates its own absence.
pub(crate) fn conditional_target_schema(
    target_value_path: &str,
    overlay: &ConditionalPathOverlay,
    values_yaml_doc: &YamlValue,
    provider: &dyn ResourceSchemaOracle,
    values_yaml_schema: Value,
    resolved_fallback: Value,
    active_by_defaults: Option<bool>,
) -> Value {
    let branch_schema = resolve_overlay_target_schema(target_value_path, overlay, provider);

    let declared_default = yaml_value_at_path(values_yaml_doc, target_value_path)
        .and_then(|value| serde_json::to_value(value).ok());
    // A branch that rejects the path's own declared default narrows values
    // the chart itself ships.
    let rejects_declared_default = |schema: &Value| {
        declared_default
            .as_ref()
            .is_some_and(|default_value| !schema_accepts_json_value(schema, default_value))
    };

    let branch_schema = if active_by_defaults.is_some()
        && should_merge_values_yaml_into_conditional_branch(&branch_schema, &values_yaml_schema)
    {
        merge_schema_list(vec![branch_schema, values_yaml_schema])
    } else {
        branch_schema
    };
    // Guards inactive by defaults or undecidable on the values doc can still
    // be activated by a user who keeps the chart's other defaults.
    if active_by_defaults != Some(true) {
        if is_placeholder_fragment_object_schema(&branch_schema)
            && !is_placeholder_fragment_object_schema(&resolved_fallback)
        {
            // The swap gives a vacuous placeholder branch the resolved
            // content, but never a shape that rejects the shipped default.
            return if rejects_declared_default(&resolved_fallback) {
                branch_schema
            } else {
                resolved_fallback
            };
        }
        // A branch whose renders all sit behind their own truthiness only
        // fires for truthy values, so it must keep accepting the shipped
        // (possibly falsy) default. A branch read unconditionally under its
        // guard may legitimately narrow the default away.
        if !overlay.evidence.facts.is_nullable {
            return branch_schema;
        }
    }

    if rejects_declared_default(&branch_schema) {
        resolved_fallback
    } else {
        branch_schema
    }
}

fn should_merge_values_yaml_into_conditional_branch(
    branch_schema: &Value,
    values_yaml_schema: &Value,
) -> bool {
    crate::schema_model::is_empty_schema(branch_schema)
        || (is_scalar_like_schema(branch_schema) && is_scalar_like_schema(values_yaml_schema))
}

fn schema_accepts_json_value(schema: &Value, instance: &Value) -> bool {
    jsonschema::validator_for(schema)
        .map(|validator| validator.is_valid(instance))
        .unwrap_or(false)
}

fn should_open_fragment_values_schema(schema: &Value, facts: ValuePathSchemaFacts) -> bool {
    !facts.values_yaml.is_mapping
        || facts.values_yaml.is_empty_map
        || fixed_object_schema_has_object_or_array_child(schema)
}

fn fixed_object_schema_has_object_or_array_child(schema: &Value) -> bool {
    schema
        .as_object()
        .and_then(|object| object.get("properties"))
        .and_then(Value::as_object)
        .is_some_and(|properties| properties.values().any(is_object_or_array_schema))
}

fn schema_type_for_guard_value(value: &Value) -> Option<&'static str> {
    match value {
        Value::String(_) => Some("string"),
        Value::Bool(_) => Some("boolean"),
        Value::Number(number) if number.is_i64() || number.is_u64() => Some("integer"),
        Value::Number(_) => Some("number"),
        Value::Null => Some("null"),
        _ => None,
    }
}
