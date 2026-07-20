use serde_json::Value;

use helm_schema_core::{
    ConditionalGuard, ConditionalPathOverlay, ContractValuePathFacts, GuardValue,
    ProviderSchemaUse, ValueKind,
};
use serde_yaml::Value as YamlValue;

use crate::condition_encoding::{
    HELM_TRUTHY_DEFINITION_NAME, helm_truthy_definition_schema, value_references_helm_truthy,
};
use crate::foreign_schema::ForeignSchemaRestriction;
use crate::merge::{merge_schema_list, merge_two_schemas, union_schema_list};
use crate::path_schema::{
    generalize_fixed_object_schema_to_open_map, merge_explicit_empty_placeholder,
    open_fragment_values_schema,
};
use crate::schema_model::{
    add_null_schema, empty_schema, empty_string_schema, guard_value_to_json,
    is_declared_object_schema, is_empty_schema, is_object_or_array_schema,
    is_open_string_map_schema, is_scalar_like_schema, is_scalar_schema, scalar_union_schema,
    schema_allows_type, schema_permits_empty_string, schema_type, type_schema,
};
use crate::schema_node::SchemaNode;
use crate::schema_node::is_placeholder_fragment_object_schema;
use crate::values_yaml::ValuesYamlPathFacts;
use crate::values_yaml::yaml_value_at_path;

/// Strings spelling an implicit YAML NULL token (including the empty
/// string) in a bare plain-scalar position.
pub(crate) const PLAIN_SCALAR_NULL_TOKEN_PATTERN: &str = r"^(|~|null|Null|NULL)$";
/// Strings spelling an implicit YAML BOOLEAN token in a bare plain-scalar
/// position (the YAML 1.1 set Helm's renderer round-trips through).
pub(crate) const PLAIN_SCALAR_BOOL_TOKEN_PATTERN: &str =
    r"^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$";

// The numeric token grammars below are derived from go-yaml v2's `resolve()`,
// the YAML 1.1 resolver Helm's manifest consumers inherit through
// `sigs.k8s.io/yaml`. A plain scalar starting with a sign or digit has every
// underscore stripped, then reads as a base-detecting integer
// (`strconv.ParseInt`/`ParseUint` with base 0: decimal, `0x` hex, `0b`
// binary, `0o`/leading-zero octal) or a decimal float
// (`yamlStyleFloat` + `ParseFloat`); a scalar starting with `.` reads as a
// bare `ParseFloat` without underscore stripping. On any parse error —
// including float64 overflow — the token falls back to a plain string.
//
// Exclusion alternates must be PROVABLY numeric (over-excluding falsely
// rejects strings that stay strings), so digit counts are bounded far below
// the float64 overflow cliff and exotic residue (underscored hex, 100+-digit
// mantissas, three-digit exponents) is deliberately left unexcluded.

/// Sign/digit-led tokens the resolver provably reads as decimal numbers,
/// after global underscore stripping. Covers integers ("1_000"), leading-zero
/// float fallbacks ("09"), trailing-dot floats ("1."), exponent forms
/// ("1e99", bounded to two exponent digits so overflow never reaches the
/// claim), and sign-led leading-dot floats ("-.5"). An unsigned leading
/// underscore stays a string (the resolver never enters its numeric path).
const PLAIN_SCALAR_DECIMAL_NUMBER_TOKEN_PATTERN: &str = r"^([0-9][0-9_]{0,50}(\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*[0-9][0-9_]{0,50}(\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*\._*[0-9][0-9_]{0,50}([eE][+-]?[0-9]{1,2})?|\.[0-9]{1,50}([eE][+-]?[0-9]{1,2})?)$";
/// Radix-prefixed tokens the resolver provably reads as integers, with digit
/// counts bounded to stay within `int64` for signed spellings.
const PLAIN_SCALAR_PREFIXED_NUMBER_TOKEN_PATTERN: &str =
    r"^[+-]?(0[xX][0-9a-fA-F]{1,15}|0[bB][01]{1,62}|0[oO][0-7]{1,20})$";
/// The exact special-float table entries: signed infinities and UNSIGNED
/// NaNs only — "+.nan" is absent from the resolver table and stays a string.
pub(crate) const PLAIN_SCALAR_SPECIAL_FLOAT_TOKEN_PATTERN: &str =
    r"^([+-]?\.(inf|Inf|INF)|\.(nan|NaN|NAN))$";
/// Tokens the resolver provably reads as `!!int` within `int64`/`uint64`:
/// optionally signed decimal without a leading zero (underscores stripped),
/// or a bounded radix-prefixed/legacy-octal literal. Float-tagged spellings
/// that merely have an integral value ("09", "4e2", "1.") stay out — a
/// float64 in an integer manifest slot is not reliably accepted.
const PLAIN_SCALAR_INTEGER_TOKEN_PATTERN: &str = r"^([+-]_*)?(0|[1-9][0-9_]{0,17}|0[xX][0-9a-fA-F]{1,15}|0[bB][01]{1,62}|0[oO][0-7]{1,20}|0[0-7]{1,20})$";
/// Tokens the resolver provably reads as any numeric tag with a
/// JSON-representable value: the integer lane plus the decimal float lanes.
/// Infinities and NaNs are excluded — they have no JSON encoding, so a
/// number slot never accepts them downstream.
pub(crate) const PLAIN_SCALAR_NUMBER_TOKEN_PATTERN: &str = r"^(([+-]_*)?(0|[1-9][0-9_]{0,17}|0[xX][0-9a-fA-F]{1,15}|0[bB][01]{1,62}|0[oO][0-7]{1,20}|0[0-7]{1,20})|[0-9][0-9_]{0,50}(\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*[0-9][0-9_]{0,50}(\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*\._*[0-9][0-9_]{0,50}([eE][+-]?[0-9]{1,2})?|\.[0-9]{1,50}([eE][+-]?[0-9]{1,2})?)$";

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
        provider_schema: &Value,
        type_hint_schema: &Value,
        guard_predicate_schema: &Value,
    ) -> bool {
        self.values_yaml.is_empty_string
            && ((self.contract.has_render_use && self.contract.all_render_uses_self_guarded)
                || schema_allows_type(provider_schema, "string")
                || is_scalar_like_schema(type_hint_schema)
                || is_scalar_like_schema(guard_predicate_schema))
    }

    fn empty_map_placeholder_has_structural_object_use(self, provider_schema: &Value) -> bool {
        self.values_yaml.is_empty_map
            && !self.contract.used_as_serialized
            && (self.contract.is_ranged_source
                || self.contract.has_self_range_guard_render_use
                || self.contract.used_as_yaml_serialized
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
    /// Branch-scoped hints: they may only WIDEN an otherwise-typed base
    /// (add accepted alternatives), never stand alone as its typing —
    /// `allOf` branches can narrow but never re-widen a base.
    pub(crate) guarded_type_hint_schema: Value,
    /// Hints from literal `default`/`coalesce` fallbacks: they type only the
    /// truthy arm of the path, so when they are the path's only typing the
    /// base must keep the whole Helm-falsy set open beside them.
    pub(crate) fallback_type_hint_schema: Value,
}

impl ResolvePolicy {
    pub(crate) fn provider_schema_for_value_use(
        &self,
        schema: &Value,
        use_: &ProviderSchemaUse,
    ) -> Option<Value> {
        match use_.kind {
            // `toYaml` accepts every input kind but preserves that kind in
            // the rendered YAML value, so a typed provider sink projects
            // directly back to the input. Opaque fragment output does not
            // provide that identity guarantee; only a sequence placement is
            // structurally load-bearing there.
            ValueKind::YamlSerialized => Some(subtract_omitted_members(
                relax_template_supplied_required(
                    schema.clone(),
                    &use_.template_supplied_member_keys,
                ),
                &use_.omitted_members,
            )),
            // A self-ranged FRAGMENT use renders loop-body ITEMS derived
            // from the members, so the slot's item schema types the
            // rendered items, never the source's members: rangeability
            // (array or map) is the only sound projection (traefik's
            // resourceAttributes flag loops reassembled through the
            // pod-template roundtrip).
            ValueKind::Fragment if use_.is_self_range_collection => schema_allows_type(
                schema, "array",
            )
            .then(|| serde_json::json!({ "anyOf": [{ "type": "array" }, { "type": "object" }] })),
            ValueKind::Fragment => schema_allows_type(schema, "array").then(|| schema.clone()),
            ValueKind::PartialScalar | ValueKind::Serialized => None,
            ValueKind::Scalar if use_.is_self_range_collection => {
                ForeignSchemaRestriction::ScalarCollection.apply(schema.clone())
            }
            // The slot observes ONE separator-delimited segment of the raw
            // string, so the preimage constrains that segment instead of the
            // whole spelling.
            ValueKind::Scalar if use_.split_segment.is_some() => use_
                .split_segment
                .as_ref()
                .and_then(|segment| split_segment_provider_preimage(schema, segment)),
            ValueKind::Scalar => ForeignSchemaRestriction::Scalar
                .apply(schema.clone())
                .map(plain_scalar_provider_preimage),
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
            | ConditionalGuard::ContainsMemberEquals { .. }
            | ConditionalGuard::TypeIs { .. }
            | ConditionalGuard::MatchesPattern { .. }
            | ConditionalGuard::IntGt { .. }
            | ConditionalGuard::IntLt { .. }
            | ConditionalGuard::HasKey { .. }
            | ConditionalGuard::AtMostOneMember { .. }
            | ConditionalGuard::MinMembers { .. }
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
            guarded_type_hint_schema,
            fallback_type_hint_schema,
        } = input;
        // A literal fallback documents intent, not a contract: like the
        // declared default, its type must not narrow a path that some
        // serializing or totally-formatting render provably tolerates any
        // input at (flux2's `--log-level={{ .Values.logLevel |
        // default "info" }}` embeds every value kind as argument text).
        let fallback_type_hint_schema = if facts.contract.used_as_serialized {
            empty_schema()
        } else {
            fallback_type_hint_schema
        };
        // A fallback hint types only the truthy arm: every Helm-empty input
        // takes the literal fallback and renders, so when NO independent
        // channel types the path (a provider slot, a guard comparison, a
        // runtime string contract, or an ordinary hint), the merged base
        // must keep the whole Helm-falsy set open beside the hinted type
        // (cilium `default "1.8" .Values.upgradeCompatibility`).
        let fallback_hint_only_typing = !is_empty_schema(&fallback_type_hint_schema)
            && is_empty_schema(&type_hint_schema)
            && is_empty_schema(&provider_schema)
            && is_empty_schema(&guard_predicate_schema)
            && !facts.contract.has_string_contract;
        // Inside the merge pipeline the fallback hint behaves like an
        // ordinary hint; only the falsy escape below distinguishes it.
        let type_hint_schema = merge_two_schemas(type_hint_schema, fallback_type_hint_schema);
        // A serialized or totally-stringified render accepts any input
        // type, so the chart provably tolerates anything at this path in
        // the states where that use is live. The declared default then
        // documents intent without narrowing. Real contracts from OTHER
        // uses (provider sinks on their own rows, string-transform hints,
        // guard schemas) still apply below: one stringified occurrence must
        // not erase an independent stricter consumer.
        let values_yaml_schema = if facts.contract.used_as_serialized {
            empty_schema()
        } else if facts.contract.is_partial_scalar_value_path
            && is_scalar_schema(&values_yaml_schema)
        {
            // A scalar spliced into a partial string slot (`-v={{ x }}`)
            // prints ANY scalar; the declared default's type is intent,
            // not a constraint, so it widens to the scalar union. Real
            // contracts from other uses still apply below.
            scalar_union_schema()
        } else {
            values_yaml_schema
        };
        // The same argument defers guard-derived typing on serialized
        // paths: a `typeIs "string"` guard partitions branches, and a
        // serialized sibling branch proves the complement renders too, so
        // the guard's type may only WIDEN an otherwise-typed base below —
        // never stand alone as its typing.
        let mut guard_predicate_schema = guard_predicate_schema;
        let deferred_guard_schema = if facts.contract.used_as_serialized {
            std::mem::replace(&mut guard_predicate_schema, empty_schema())
        } else {
            empty_schema()
        };
        let preserve_explicit_null_default_by_contract =
            facts.preserve_explicit_null_default(&type_hint_schema, &guard_predicate_schema);
        let preserve_empty_string_fallback = facts.preserve_empty_string_fallback(
            &provider_schema,
            &type_hint_schema,
            &guard_predicate_schema,
        );
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
                guarded_type_hint_schema: empty_schema(),
                fallback_type_hint_schema: empty_schema(),
            },
            preserve_empty_string_fallback,
        );
        let widening_schema = merge_two_schemas(guarded_type_hint_schema, deferred_guard_schema);
        let merged = if !is_empty_schema(&merged) && !is_empty_schema(&widening_schema) {
            merge_two_schemas(merged, widening_schema)
        } else {
            merged
        };
        let merged = if !is_empty_schema(&merged)
            && !facts.contract.is_direct_ranged_source
            && !facts.contract.has_string_contract
            && ((facts.contract.has_render_use
                && (facts.contract.all_render_uses_self_guarded
                    || (facts.contract.all_render_uses_falsy_tolerant
                        && !facts.contract.has_referenced_descendants))
                && !facts.contract.has_unconditional_render_use)
                || fallback_hint_only_typing)
        {
            // Every Helm-falsy value skips a self-guarded consumer (or takes
            // a literal fallback before any consumer runs). Keeping only the
            // declared falsy default made schema validity depend on which
            // off-state the chart happened to ship. A path-wide runtime
            // string contract disables the escape: that consumer parses the
            // RAW value before any selection runs. Falsy-tolerant uses
            // (merge operands, digest rows) extend the escape only for LEAF
            // paths: a falsy parent would still abort its descendants' field
            // reads, so referenced descendants keep the strict base.
            union_schema_list(vec![merged, helm_falsy_schema()])
        } else {
            merged
        };
        let preserve_explicit_null_default = preserve_explicit_null_default_by_contract
            || (facts.values_yaml.is_explicit_null
                && facts.contract.used_as_fragment
                && !is_empty_schema(&merged));

        // A declared object/array whose every render use sits under its own
        // truthy guard accepts explicit `null`: helm null-deletion removes
        // the key and the falsy guard skips the branch, so null never
        // reaches a consumer (datadog `datadog.securityContext`).
        let self_guarded_structure_tolerates_null = facts.contract.is_nullable
            && facts.contract.has_render_use
            && facts.contract.all_render_uses_self_guarded
            && is_object_or_array_schema(&merged);
        let resolved = if (preserve_explicit_null_default
            || (is_scalar_like_schema(&merged) && facts.contract.is_nullable)
            || self_guarded_structure_tolerates_null)
            && !is_empty_schema(&merged)
        {
            add_null_schema(merged)
        } else if preserve_explicit_null_default {
            empty_schema()
        } else if facts.empty_map_placeholder_has_structural_object_use(&merged) {
            // A merge-layered render proves any user-supplied map renders
            // here — its member typing rides the synthesized layer arms —
            // so the declared `{}` keeps an open-map lane, and the layer's
            // SYNTHETIC self-truthiness guard must not pin the exact
            // off-state against the other (templated-string) evidence.
            let merged = if facts.contract.has_merge_layered_use {
                crate::merge::union_schema_list(vec![
                    merged,
                    serde_json::json!({ "additionalProperties": {}, "type": "object" }),
                ])
            } else {
                merged
            };
            merge_explicit_empty_placeholder(
                merged,
                facts.values_yaml.is_empty_map,
                // Bare `p.*` value rows also spell `*` (map-value flows),
                // so only STRUCTURED item rows prove a list shape here — and
                // a destructured `range $k, $v` iterates maps just as well,
                // so its member rows must keep the declared-`{}` map OPEN
                // for user-named entries instead of pinning the exact empty
                // off-state (cluster-autoscaler `extraEnvConfigMaps`).
                facts.contract.has_structured_item_descendants
                    && !facts.contract.has_destructured_range_use,
                facts.contract.has_render_use
                    && facts.contract.all_render_uses_self_guarded
                    && !facts.contract.has_merge_layered_use,
                facts.contract.used_as_fragment && !facts.contract.is_ranged_source,
            )
        } else if facts.values_yaml.has_no_schema_evidence && facts.contract.is_ranged_source {
            // An undeclared map the chart itself iterates is user-populated
            // (istiod's `range $key, $val := .Values.env` has no values.yaml
            // default at all); its keys are data, so member probes must not
            // close it. The stamp only applies to object-typed schemas.
            crate::path_schema::stamp_explicit_map_openness(merged)
        } else if facts.contract.used_as_serialized
            && facts.contract.has_referenced_descendants
            && is_empty_schema(&merged)
        {
            // Descendant rows insert under this unconstrained slot; the
            // carrier merge reads a bare `{}` as an empty placeholder and
            // closes it, while an explicit `additionalProperties: {}`
            // counts as openness evidence and survives.
            serde_json::json!({ "additionalProperties": {} })
        } else {
            merged
        };
        // A directly ranged path accepts the whole runtime iterable
        // domain: `range` renders collections, nil, and (without member
        // structure in the loop body) integer counts, regardless of the
        // declared default's shape. Guarded member implications below
        // still narrow the live states.
        if facts.contract.is_direct_ranged_source {
            // A serialized sibling use renders any input at its own site,
            // but cannot erase the runtime domain of an independently
            // executing direct range.
            let iterable = crate::runtime_iterable_schema(
                !facts.contract.has_destructured_range_use
                    && !facts.contract.has_json_decoded_range_use,
            );
            if is_empty_schema(&resolved) {
                // The direct range is the only evidence: its runtime
                // domain is the path's whole domain (a non-empty base
                // also keeps the carrier's item rows from re-typing the
                // slot as a bare array).
                iterable
            } else {
                union_schema_list(vec![resolved, iterable])
            }
        } else {
            resolved
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
            && !facts.contract.used_as_serialized
            && is_empty_schema(provider_schema)
            && is_empty_schema(type_hint_schema)
            && is_empty_schema(guard_predicate_schema)
            && facts.values_yaml.has_no_schema_evidence
        {
            scalar_union_schema()
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
                    && is_declared_object_schema(&input.values_yaml_schema)
                    && is_scalar_schema(&input.provider_schema)
                {
                    input.values_yaml_schema
                } else if input.facts.contract.used_as_fragment
                    && is_declared_object_schema(&input.values_yaml_schema)
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
        } else if input.facts.contract.used_as_fragment
            && !input.facts.contract.used_as_serialized
            && (is_empty_schema(&input.values_yaml_schema) || input.facts.values_yaml.is_empty_map)
        {
            // A fragment-only path with no shape evidence (undeclared, or a
            // declared-`{}` placeholder) splices whatever the user supplies
            // and `toYaml` is total: scalars, sequences, and maps all
            // render in a mapping-value slot. The splice claims no
            // shape; independent consumers narrow through their own
            // guarded lanes.
            empty_schema()
        } else if !is_empty_schema(&input.values_yaml_schema) {
            input.values_yaml_schema
        } else {
            empty_schema()
        };

        let base = merge_two_schemas(base, input.type_hint_schema);
        // Condition guards are MAY-BE dispatch evidence (`kindIs "map" x`
        // arms prove the chart handles maps), never a requirement: a
        // declared default shape must not erase a structurally handled
        // alternative, so the guard domain unions with the base instead of
        // intersecting it.
        if is_empty_schema(&base) || is_empty_schema(&input.guard_predicate_schema) {
            merge_two_schemas(base, input.guard_predicate_schema)
        } else {
            union_schema_list(vec![base, input.guard_predicate_schema])
        }
    }
}

/// Drop provider `required` entries the template already satisfies with
/// literal sibling keys in the splice's own mapping: the rendered object
/// carries them regardless of the user value (metrics-server's `- name:
/// tmp` beside `toYaml .Values.tmpVolume` at a Volume slot).
fn relax_template_supplied_required(
    mut schema: Value,
    supplied: &std::collections::BTreeSet<String>,
) -> Value {
    if supplied.is_empty() {
        return schema;
    }
    if let Some(object) = schema.as_object_mut() {
        if let Some(required) = object.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|key| key.as_str().is_none_or(|key| !supplied.contains(key)));
            if required.is_empty() {
                object.remove("required");
            }
        }
        for arms_key in ["allOf", "anyOf", "oneOf"] {
            if let Some(arms) = object.get_mut(arms_key).and_then(Value::as_array_mut) {
                for arm in arms {
                    *arm = relax_template_supplied_required(arm.take(), supplied);
                }
            }
        }
    }
    schema
}

/// Strip the typing of members a guard-scoped `omit` may remove before the
/// sink reads the map: the rendered object lacks them in the omitting
/// states, so their unconditional payload typing would reject documents
/// the chart renders fine (external-secrets' OpenShift-adapted
/// `runAsUser`). Keys with lowerable retain guards come back as dedicated
/// conditional arms.
fn subtract_omitted_members(
    mut schema: Value,
    omitted: &std::collections::BTreeMap<String, Vec<helm_schema_core::ConditionalGuard>>,
) -> Value {
    if omitted.is_empty() {
        return schema;
    }
    if let Some(object) = schema.as_object_mut() {
        if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
            properties.retain(|key, _| !omitted.contains_key(key));
        }
        if let Some(required) = object.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|key| key.as_str().is_none_or(|key| !omitted.contains_key(key)));
            if required.is_empty() {
                object.remove("required");
            }
        }
        for arms_key in ["allOf", "anyOf", "oneOf"] {
            if let Some(arms) = object.get_mut(arms_key).and_then(Value::as_array_mut) {
                for arm in arms {
                    *arm = subtract_omitted_members(arm.take(), omitted);
                }
            }
        }
    }
    schema
}

/// Preimage of a provider slot observed through ONE separator-delimited
/// segment of the raw string (tempo's `regexSplit ":" . -1 | last` port
/// suffix): an integer-typed slot admits exactly the strings whose named
/// segment spells an integer. Any other slot type abstains — a string
/// segment leaves the source effectively unconstrained.
fn split_segment_provider_preimage(
    schema: &Value,
    segment: &helm_schema_core::SplitSegmentUse,
) -> Option<Value> {
    let pattern = split_segment_pattern(schema, segment)?;
    Some(serde_json::json!({ "type": "string", "pattern": pattern }))
}

/// The accepted-source pattern for a slot observed through one separator
/// segment: only integer-typed slots have a composable segment grammar; any
/// other slot type abstains.
pub(crate) fn split_segment_pattern(
    schema: &Value,
    segment: &helm_schema_core::SplitSegmentUse,
) -> Option<String> {
    if schema_type(schema) != Some("integer") {
        return None;
    }
    let separator = regex::escape(&segment.separator);
    Some(if segment.last {
        format!("^([\\s\\S]*{separator})?[+-]?[0-9]+$")
    } else {
        format!("^[+-]?[0-9]+({separator}[\\s\\S]*)?$")
    })
}

fn plain_scalar_provider_preimage(schema: Value) -> Value {
    // A raw string spelling an implicit YAML token of ANOTHER kind the slot
    // ALSO allows still validates once the completed document reparses
    // (aws-load-balancer-controller's `nameOverride: "null"` renders a
    // null the null-widened provider slot accepts), so the token-class
    // exclusions below apply only for kinds the slot rejects.
    let allowed = ImplicitTokenAllowance {
        null: schema_allows_type(&schema, "null"),
        boolean: schema_allows_type(&schema, "boolean"),
        number: schema_allows_type(&schema, "number"),
        integer: schema_allows_type(&schema, "integer"),
    };
    plain_scalar_provider_preimage_with(schema, &allowed)
}

/// Which implicit-token kinds the WHOLE provider slot accepts; computed once
/// so nested variants keep seeing their siblings' kinds.
#[derive(Clone, Copy)]
struct ImplicitTokenAllowance {
    null: bool,
    boolean: bool,
    number: bool,
    integer: bool,
}

fn plain_scalar_provider_preimage_with(schema: Value, allowed: &ImplicitTokenAllowance) -> Value {
    let Some(object) = schema.as_object() else {
        return schema;
    };
    if let Some(types) = object.get("type").and_then(Value::as_array) {
        let variants = types
            .iter()
            .filter_map(Value::as_str)
            .map(|schema_type| {
                let mut variant = object.clone();
                variant.insert("type".to_string(), Value::String(schema_type.to_string()));
                plain_scalar_provider_preimage_with(Value::Object(variant), allowed)
            })
            .collect();
        return union_schema_list(variants);
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(variants) = object.get(keyword).and_then(Value::as_array) {
            let mut transformed = object.clone();
            transformed.insert(
                keyword.to_string(),
                Value::Array(
                    variants
                        .iter()
                        .cloned()
                        .map(|variant| plain_scalar_provider_preimage_with(variant, allowed))
                        .collect(),
                ),
            );
            return Value::Object(transformed);
        }
    }

    match schema_type(&schema) {
        Some("integer") => scalar_number_preimage(schema, true),
        Some("number") => scalar_number_preimage(schema, false),
        Some("boolean") => scalar_boolean_preimage(schema),
        Some("string") => scalar_plain_string_preimage(schema, allowed),
        _ => schema,
    }
}

fn scalar_plain_string_preimage(schema: Value, allowed: &ImplicitTokenAllowance) -> Value {
    let mut exclusions = vec![
        serde_json::json!({ "not": { "pattern": "^[!&*#{}\\[\\],|>@`%]" } }),
        serde_json::json!({ "not": { "pattern": "^[-?:]([ \\t]|$)" } }),
        serde_json::json!({ "not": { "pattern": ":[ \\t]|:$" } }),
        serde_json::json!({ "not": { "pattern": "[ \\t]#" } }),
        serde_json::json!({ "not": { "pattern": "[\\r\\n]" } }),
    ];
    if !allowed.null {
        exclusions
            .push(serde_json::json!({ "not": { "pattern": PLAIN_SCALAR_NULL_TOKEN_PATTERN } }));
    }
    if !allowed.boolean {
        exclusions
            .push(serde_json::json!({ "not": { "pattern": PLAIN_SCALAR_BOOL_TOKEN_PATTERN } }));
    }
    if !allowed.number && !allowed.integer {
        exclusions.push(serde_json::json!({
            "not": { "pattern": PLAIN_SCALAR_DECIMAL_NUMBER_TOKEN_PATTERN }
        }));
        // Helm's YAML 1.1 resolver also reads hex, explicit octal, and
        // binary spellings as integers, so a bare token in any of those
        // forms reparses away from the string the sink needs (velero's
        // unquoted BackupStorageLocation provider).
        exclusions.push(serde_json::json!({
            "not": { "pattern": PLAIN_SCALAR_PREFIXED_NUMBER_TOKEN_PATTERN }
        }));
    }
    if !allowed.number {
        exclusions.push(serde_json::json!({
            "not": { "pattern": PLAIN_SCALAR_SPECIAL_FLOAT_TOKEN_PATTERN }
        }));
    }
    let lexical_domain = serde_json::json!({
        "type": "string",
        "allOf": exclusions
    });
    merge_schema_list(vec![schema, lexical_domain])
}

fn scalar_number_preimage(schema: Value, integer: bool) -> Value {
    let object = schema.as_object().expect("typed schema is an object");
    if [
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
    ]
    .iter()
    .any(|keyword| object.contains_key(*keyword))
    {
        return schema;
    }
    let string_schema = scalar_string_preimage(
        object,
        if integer {
            PLAIN_SCALAR_INTEGER_TOKEN_PATTERN
        } else {
            PLAIN_SCALAR_NUMBER_TOKEN_PATTERN
        },
    );
    union_schema_list(vec![schema, string_schema])
}

fn scalar_boolean_preimage(schema: Value) -> Value {
    let object = schema.as_object().expect("typed schema is an object");
    let string_schema = scalar_string_preimage(object, PLAIN_SCALAR_BOOL_TOKEN_PATTERN);
    union_schema_list(vec![schema, string_schema])
}

fn scalar_string_preimage(object: &serde_json::Map<String, Value>, pattern: &str) -> Value {
    let mut schema = serde_json::Map::new();
    schema.insert("type".to_string(), Value::String("string".to_string()));
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        schema.insert(
            "enum".to_string(),
            Value::Array(
                values
                    .iter()
                    .map(Value::to_string)
                    .map(Value::String)
                    .collect(),
            ),
        );
    } else if let Some(value) = object.get("const") {
        schema.insert("const".to_string(), Value::String(value.to_string()));
    } else {
        schema.insert("pattern".to_string(), Value::String(pattern.to_string()));
    }
    Value::Object(schema)
}

fn helm_falsy_schema() -> Value {
    serde_json::json!({
        "not": {
            "$ref": format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}")
        }
    })
}

/// The branch schema is the strongest available evidence schema that is not a
/// vacuous placeholder when real content exists and accepts the chart's
/// shipped default whenever the branch tolerates its own absence.
pub(crate) fn conditional_target_schema(
    target_value_path: &str,
    overlay: &ConditionalPathOverlay,
    values_yaml_doc: &YamlValue,
    branch_schema: Value,
    values_yaml_schema: Value,
    resolved_fallback: Value,
    active_by_defaults: Option<bool>,
) -> Value {
    let schema = conditional_target_schema_inner(
        target_value_path,
        overlay,
        values_yaml_doc,
        branch_schema,
        values_yaml_schema,
        resolved_fallback,
        active_by_defaults,
    );
    // A branch whose renders all sit behind the path's OWN truthy selection
    // (`if .Values.x`, `with`, `default`) never consumes a Helm-falsy value:
    // the guard skips or the fallback substitutes, so the branch's typing
    // holds only for truthy inputs. The self-guard predicates are
    // deliberately absent from the overlay key (they double as nullability
    // evidence), so the falsy escape must ride the arm schema itself. The
    // declared-default and values.yaml merges above may narrow the escape
    // away again, which is why it is restored after them.
    let facts = overlay.evidence.facts;
    if facts.has_render_use
        && facts.all_render_uses_self_guarded
        && !facts.has_unconditional_render_use
        && !facts.is_direct_ranged_source
        && !crate::schema_model::is_empty_schema(&schema)
    {
        return union_schema_list(vec![schema, helm_falsy_schema()]);
    }
    schema
}

fn conditional_target_schema_inner(
    target_value_path: &str,
    overlay: &ConditionalPathOverlay,
    values_yaml_doc: &YamlValue,
    branch_schema: Value,
    values_yaml_schema: Value,
    resolved_fallback: Value,
    active_by_defaults: Option<bool>,
) -> Value {
    let declared_default = yaml_value_at_path(values_yaml_doc, target_value_path)
        .and_then(|value| serde_json::to_value(value).ok());
    // A branch that rejects the path's own declared default narrows values
    // the chart itself ships.
    let rejects_declared_default = |schema: &Value| {
        declared_default
            .as_ref()
            .is_some_and(|default_value| !schema_accepts_json_value(schema, default_value))
    };

    // A branch keyed on the path's own positive type partition must stay
    // satisfiable for that type — the arm executes for
    // it. A branch resolve that contradicts its partition (an object-guess
    // for a `kindIs "slice"` arm) merges WITH the partition instead.
    let branch_schema = {
        let mut branch_schema = branch_schema;
        let mut positive_self_types = std::collections::BTreeSet::new();
        for guard in &overlay.guards {
            collect_positive_self_types(guard, target_value_path, false, &mut positive_self_types);
        }
        for schema_type in positive_self_types {
            // A "number" partition over an integer-allowing branch is NOT a
            // contradiction: draft-07 `integer` is a value predicate that
            // integral floats satisfy, so the arm stays satisfiable while the
            // branch keeps rejecting fractional floats the render would place
            // into the provider slot (sealed-secrets' `typeOf`-dispatched
            // policy/v1 minAvailable).
            if schema_type == "number" && schema_allows_non_falsy_type(&branch_schema, "integer") {
                continue;
            }
            if !schema_allows_non_falsy_type(&branch_schema, &schema_type) {
                branch_schema = union_schema_list(vec![branch_schema, type_schema(&schema_type)]);
            }
        }
        branch_schema
    };

    // A serialized catch-all branch still needs the declared structural
    // shape: unlike a positive type arm, the complement executes for every
    // unhandled kind.
    let self_type_complement = overlay.guards.iter().any(|guard| {
        matches!(
            guard,
            ConditionalGuard::Not(inner)
                if matches!(
                    inner.as_ref(),
                    ConditionalGuard::TypeIs { path, .. } if path == target_value_path
                )
        )
    });
    let branch_schema = if active_by_defaults.is_some()
        && (!(overlay.evidence.facts.used_as_serialized
            || overlay.evidence.facts.used_as_yaml_serialized)
            || self_type_complement)
        // A declared-`{}` placeholder claims no input shape for a fragment
        // branch: `toYaml` is total there, so the placeholder's
        // object typing must not narrow the branch.
        && !(overlay.evidence.facts.used_as_fragment
            && is_placeholder_fragment_object_schema(&values_yaml_schema))
        && should_merge_values_yaml_into_conditional_branch(&branch_schema, &values_yaml_schema)
    {
        merge_schema_list(vec![branch_schema, values_yaml_schema.clone()])
    } else {
        branch_schema
    };
    let branch_schema = if rejects_declared_default(&branch_schema) {
        declared_default.as_ref().map_or_else(
            || branch_schema.clone(),
            |default_value| {
                // An explicitly DECLARED null default must stay accepted
                // when every use in the branch tolerates null (self-guarded
                // rows: a null is falsy, or deleted by helm, so it never
                // reaches the consumer). A branch that places the raw value
                // keeps its strict typing.
                if default_value.is_null() && overlay.evidence.facts.is_nullable {
                    return union_schema_list(vec![branch_schema.clone(), type_schema("null")]);
                }
                let declared_type = if default_value.is_object() {
                    Some("object")
                } else if default_value.is_array() {
                    Some("array")
                } else {
                    None
                };
                if declared_type
                    .is_some_and(|schema_type| !schema_allows_type(&branch_schema, schema_type))
                {
                    union_schema_list(vec![
                        branch_schema.clone(),
                        open_objects_rejecting_declared_members(
                            values_yaml_schema.clone(),
                            default_value,
                        ),
                    ])
                } else {
                    open_objects_rejecting_declared_members(branch_schema.clone(), default_value)
                }
            },
        )
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
        declared_default
            .as_ref()
            .map_or(resolved_fallback.clone(), |default_value| {
                open_objects_rejecting_declared_members(resolved_fallback, default_value)
            })
    } else {
        branch_schema
    }
}

fn collect_positive_self_types(
    guard: &helm_schema_core::ConditionalGuard,
    target_value_path: &str,
    negated: bool,
    out: &mut std::collections::BTreeSet<String>,
) {
    match guard {
        helm_schema_core::ConditionalGuard::TypeIs { path, schema_type }
            if !negated && path == target_value_path =>
        {
            out.insert(schema_type.clone());
        }
        helm_schema_core::ConditionalGuard::Not(inner) => {
            collect_positive_self_types(inner, target_value_path, !negated, out);
        }
        helm_schema_core::ConditionalGuard::AllOf(guards)
        | helm_schema_core::ConditionalGuard::AnyOf(guards) => {
            for guard in guards {
                collect_positive_self_types(guard, target_value_path, negated, out);
            }
        }
        helm_schema_core::ConditionalGuard::Truthy { .. }
        | helm_schema_core::ConditionalGuard::With { .. }
        | helm_schema_core::ConditionalGuard::IntGt { .. }
        | helm_schema_core::ConditionalGuard::IntLt { .. }
        | helm_schema_core::ConditionalGuard::HasKey { .. }
        | helm_schema_core::ConditionalGuard::ContainsMemberEquals { .. }
        | helm_schema_core::ConditionalGuard::Eq { .. }
        | helm_schema_core::ConditionalGuard::NotEq { .. }
        | helm_schema_core::ConditionalGuard::Absent { .. }
        | helm_schema_core::ConditionalGuard::TypeIs { .. }
        | helm_schema_core::ConditionalGuard::MatchesPattern { .. }
        | helm_schema_core::ConditionalGuard::AtMostOneMember { .. }
        | helm_schema_core::ConditionalGuard::MinMembers { .. } => {}
    }
}

fn schema_allows_non_falsy_type(schema: &Value, schema_type: &str) -> bool {
    if schema
        .get("not")
        .and_then(|not| not.get("$ref"))
        .and_then(Value::as_str)
        == Some(&format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}"))
    {
        return false;
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(arms) = schema.get(keyword).and_then(Value::as_array) {
            return arms
                .iter()
                .any(|arm| schema_allows_non_falsy_type(arm, schema_type));
        }
    }
    schema_allows_type(schema, schema_type)
}

pub(crate) fn open_objects_rejecting_declared_members(schema: Value, declared: &Value) -> Value {
    preserve_declared_default(schema, declared, false)
}

pub(crate) fn preserve_declared_default_in_schema(schema: Value, declared: &Value) -> Value {
    let schema = preserve_declared_default(schema, declared, true);
    preserve_declared_plain_scalar_empty_defaults(schema, declared)
}

fn preserve_declared_default(mut schema: Value, declared: &Value, preserve_scalar: bool) -> Value {
    let (Some(schema_object), Some(declared_object)) =
        (schema.as_object_mut(), declared.as_object())
    else {
        if let (Some(schema_object), Some(declared_items)) =
            (schema.as_object_mut(), declared.as_array())
            && let Some(items_schema) = schema_object.get_mut("items")
        {
            for declared_item in declared_items {
                *items_schema = preserve_declared_default(
                    std::mem::take(items_schema),
                    declared_item,
                    preserve_scalar,
                );
            }
        }
        return if !preserve_scalar || schema_accepts_json_value(&schema, declared) {
            schema
        } else {
            union_schema_list(vec![
                schema,
                SchemaNode::const_value(declared.clone()).into_value(),
            ])
        };
    };

    for keyword in ["allOf", "anyOf", "oneOf"] {
        let Some(branches) = schema_object.get_mut(keyword).and_then(Value::as_array_mut) else {
            continue;
        };
        for branch in branches {
            *branch = preserve_declared_default(std::mem::take(branch), declared, false);
        }
    }
    for keyword in ["then", "else"] {
        let Some(branch) = schema_object.get_mut(keyword) else {
            continue;
        };
        *branch = preserve_declared_default(std::mem::take(branch), declared, false);
    }

    let known_properties = schema_object
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    if schema_object.get("additionalProperties") == Some(&Value::Bool(false))
        && declared_object
            .keys()
            .any(|key| !known_properties.contains(key))
    {
        schema_object.remove("additionalProperties");
    }

    let Some(properties) = schema_object
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    else {
        return schema;
    };
    for (key, child_schema) in properties {
        let Some(child_default) = declared_object.get(key) else {
            continue;
        };
        *child_schema =
            preserve_declared_default(std::mem::take(child_schema), child_default, preserve_scalar);
    }
    schema
}

fn preserve_declared_plain_scalar_empty_defaults(mut schema: Value, declared: &Value) -> Value {
    if declared.as_str() == Some("") {
        return if has_plain_scalar_implicit_token_exclusion(&schema)
            && !schema_accepts_json_value(&schema, declared)
        {
            union_schema_list(vec![
                schema,
                SchemaNode::const_value(declared.clone()).into_value(),
            ])
        } else {
            schema
        };
    }

    let Some(schema_object) = schema.as_object_mut() else {
        return schema;
    };
    if let Some(declared_items) = declared.as_array() {
        if let Some(items_schema) = schema_object.get_mut("items") {
            for declared_item in declared_items {
                *items_schema = preserve_declared_plain_scalar_empty_defaults(
                    std::mem::take(items_schema),
                    declared_item,
                );
            }
        }
        return schema;
    }
    let Some(declared_object) = declared.as_object() else {
        return schema;
    };

    // Structural wrappers (`allOf` narrowing branches, `if`/`then`/`else`
    // dispatch, and the `anyOf`/`oneOf` member-projection arms that model a
    // ranged source as array | object | null) carry the same declared
    // default into each branch.
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema_object.get_mut(keyword).and_then(Value::as_array_mut) {
            for branch in branches {
                *branch =
                    preserve_declared_plain_scalar_empty_defaults(std::mem::take(branch), declared);
            }
        }
    }
    for keyword in ["then", "else"] {
        if let Some(branch) = schema_object.get_mut(keyword) {
            *branch =
                preserve_declared_plain_scalar_empty_defaults(std::mem::take(branch), declared);
        }
    }
    if let Some(properties) = schema_object
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    {
        for (key, child_schema) in properties {
            let Some(child_default) = declared_object.get(key) else {
                continue;
            };
            *child_schema = preserve_declared_plain_scalar_empty_defaults(
                std::mem::take(child_schema),
                child_default,
            );
        }
    }
    // A map default whose members are validated by a shared member schema
    // (`range`d over `additionalProperties`/`items`) preserves each declared
    // member's empty scalar defaults through that one member schema.
    for keyword in ["additionalProperties", "items"] {
        if let Some(member_schema) = schema_object.get_mut(keyword)
            && member_schema.is_object()
        {
            for declared_value in declared_object.values() {
                *member_schema = preserve_declared_plain_scalar_empty_defaults(
                    std::mem::take(member_schema),
                    declared_value,
                );
            }
        }
    }
    schema
}

fn has_plain_scalar_implicit_token_exclusion(schema: &Value) -> bool {
    if schema
        .get("not")
        .and_then(|not| not.get("pattern"))
        .and_then(Value::as_str)
        == Some(PLAIN_SCALAR_NULL_TOKEN_PATTERN)
    {
        return true;
    }
    // The preimage rides an `allOf` of `not` patterns, and a nullable sink
    // wraps that in an `anyOf`/`oneOf` alongside the `null` arm, so the
    // exclusion must be detected through every combinator wrapper.
    ["allOf", "anyOf", "oneOf"].iter().any(|keyword| {
        schema
            .get(keyword)
            .and_then(Value::as_array)
            .is_some_and(|branches| {
                branches
                    .iter()
                    .any(has_plain_scalar_implicit_token_exclusion)
            })
    })
}

fn should_merge_values_yaml_into_conditional_branch(
    branch_schema: &Value,
    values_yaml_schema: &Value,
) -> bool {
    crate::schema_model::is_empty_schema(branch_schema)
        || (is_scalar_like_schema(branch_schema) && is_scalar_like_schema(values_yaml_schema))
}

fn schema_accepts_json_value(schema: &Value, instance: &Value) -> bool {
    let document = value_references_helm_truthy(schema).then(|| {
        serde_json::json!({
            "$defs": {
                HELM_TRUTHY_DEFINITION_NAME: helm_truthy_definition_schema()
            },
            "allOf": [schema]
        })
    });
    jsonschema::validator_for(document.as_ref().unwrap_or(schema))
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
