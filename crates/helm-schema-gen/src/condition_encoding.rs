use helm_schema_core::{ConditionalGuard, GuardValue};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::schema_model::guard_value_to_json;
use crate::schema_node::{JsonSchemaType, SchemaNode};
use crate::split_value_path;
use crate::values_yaml::yaml_value_at_path;

pub(crate) fn build_condition_clauses(
    guards: &[ConditionalGuard],
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
) -> Vec<SchemaNode> {
    guards
        .iter()
        .filter_map(|guard| {
            build_single_condition_fragment(guard, ancestor_segments, values_yaml_doc)
        })
        .collect()
}

/// Pass-level memo for guard-condition fragments: big charts repeat the same
/// activation and branch guards across hundreds of arms, and rebuilding each
/// structural encoding dominates arm emission.
pub(crate) type ConditionFragmentCache =
    std::collections::BTreeMap<(Vec<String>, ConditionalGuard), Option<SchemaNode>>;

pub(crate) fn build_condition_clauses_cached(
    guards: &[ConditionalGuard],
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
    cache: &mut ConditionFragmentCache,
) -> Vec<SchemaNode> {
    guards
        .iter()
        .filter_map(|guard| {
            cache
                .entry((ancestor_segments.to_vec(), guard.clone()))
                .or_insert_with(|| {
                    build_single_condition_fragment(guard, ancestor_segments, values_yaml_doc)
                })
                .clone()
        })
        .collect()
}

fn build_single_condition_fragment(
    guard: &ConditionalGuard,
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
) -> Option<SchemaNode> {
    match guard {
        ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
            let declared = yaml_value_at_path(values_yaml_doc, path);
            // Helm COALESCES mapping values with a declared mapping default
            // instead of replacing it: under a non-empty mapping default,
            // any object the user writes (even `{}`) merges into that
            // default and renders truthy. The document-level test must
            // accept every object then, or its negation would reject
            // documents the chart renders fine. (Explicitly nulling out
            // every default key still merges to an empty — falsy — mapping;
            // that residue stays unmodeled, in the permissive direction.)
            let default_is_nonempty_mapping =
                matches!(declared, Some(YamlValue::Mapping(mapping)) if !mapping.is_empty());
            let truthy = if default_is_nonempty_mapping {
                SchemaNode::any_of(vec![SchemaNode::object(), helm_truthy_condition_schema()])
            } else {
                helm_truthy_condition_schema()
            };
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                truthy,
                declared.is_some_and(yaml_value_is_truthy),
            )
        }
        ConditionalGuard::Eq { path, value } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            guard_value_enum_schema(value)?,
            guard_value_matches_optional_yaml(value, yaml_value_at_path(values_yaml_doc, path)),
        ),
        ConditionalGuard::NotEq { path, value } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            guard_value_enum_schema(value).map(SchemaNode::not)?,
            !guard_value_matches_optional_yaml(value, yaml_value_at_path(values_yaml_doc, path)),
        ),
        // The member key is an OPAQUE property name (it may contain dots),
        // so it becomes a literal `required` entry below the segmented
        // collection path. An absent collection satisfies the test exactly
        // when the declared default already ships that key.
        ConditionalGuard::HasKey { path, key } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            SchemaNode::foreign(serde_json::json!({
                "type": "object",
                "required": [key],
            })),
            matches!(
                yaml_value_at_path(values_yaml_doc, path),
                Some(YamlValue::Mapping(mapping))
                    if mapping.contains_key(YamlValue::String(key.clone()))
            ),
        ),
        // The guard is a sound SUBSET of its Sprig coercion by definition:
        // the explicit test admits raw integers, plus the string spellings
        // the coercion provably parses into the claimed region (jenkins'
        // `"5"` replicas abort like `5`; a mixed-sign region also collects
        // every unparseable spelling through the complement lane). An
        // absent key satisfies it exactly when the declared default lands
        // in the region (the merged render is then statically decided).
        ConditionalGuard::IntGt { path, bound } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            int_bound_leaf_schema(
                "exclusiveMinimum",
                *bound,
                int_region_string_preimage(true, *bound),
            ),
            decimal_default_int_value(yaml_value_at_path(values_yaml_doc, path))
                .is_some_and(|value| value > *bound),
        ),
        ConditionalGuard::IntLt { path, bound } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            int_bound_leaf_schema(
                "exclusiveMaximum",
                *bound,
                int_region_string_preimage(false, *bound),
            ),
            decimal_default_int_value(yaml_value_at_path(values_yaml_doc, path))
                .is_some_and(|value| value < *bound),
        ),
        ConditionalGuard::Absent { path } => {
            let segments = split_value_path(path);
            let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
            if relative_segments.is_empty() {
                return None;
            }
            // Render-time absence after coalescing with declared defaults:
            // an explicit `null` deletes the key (helm null-deletion), and
            // a missing key stays absent only when the chart declares no
            // (non-null) default to fill it. `Absent` deliberately counts
            // explicit null as absent — the nil-safe selector lanes
            // (`(.Values.x).leaf`) render at null exactly like at a
            // missing key. Strict key-presence (Sprig `hasKey`/`dig`
            // observability, where a present nil is PRESENT) is the
            // separate `HasKey` guard.
            let explicit_null = build_required_condition_fragment(
                &relative_segments,
                SchemaNode::enum_values(vec![Value::Null]),
            )?;
            let declared_default_fills = yaml_value_at_path(values_yaml_doc, path)
                .is_some_and(|value| !matches!(value, YamlValue::Null));
            if declared_default_fills {
                Some(explicit_null)
            } else {
                let missing = SchemaNode::not(build_required_condition_fragment(
                    &relative_segments,
                    SchemaNode::empty(),
                )?);
                Some(SchemaNode::any_of(vec![missing, explicit_null]))
            }
        }
        ConditionalGuard::TypeIs { path, schema_type } => {
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::type_named(schema_type),
                yaml_value_at_path(values_yaml_doc, path)
                    .is_some_and(|value| matches_yaml_schema_type(value, schema_type)),
            )
        }
        ConditionalGuard::MatchesPattern { path, pattern } => {
            let default_matches = yaml_value_at_path(values_yaml_doc, path)
                .and_then(YamlValue::as_str)
                .is_some_and(|value| {
                    regex::Regex::new(pattern).is_ok_and(|regex| regex.is_match(value))
                });
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::foreign(serde_json::json!({
                    "pattern": pattern,
                    "type": "string",
                })),
                default_matches,
            )
        }
        // The existential over iterated items: `contains` covers the array
        // lane; the object lane quantifies member values through the
        // double negation (∃ member P = ¬∀ member ¬P).
        ConditionalGuard::ContainsMemberEquals {
            path,
            member,
            value,
        } => {
            let item = SchemaNode::foreign(serde_json::json!({
                "type": "object",
                "properties": { member: guard_value_enum_schema(value)?.into_value() },
                "required": [member],
            }))
            .into_value();
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::foreign(serde_json::json!({
                    "anyOf": [
                        { "type": "array", "contains": item },
                        { "type": "object", "not": { "additionalProperties": { "not": item } } },
                    ]
                })),
                declared_collection_contains_member_equals(
                    yaml_value_at_path(values_yaml_doc, path),
                    member,
                    value,
                ),
            )
        }
        // A non-collection value never iterates, so the "at most one
        // member" bound constrains only maps and lists; both size keywords
        // are no-ops on other instance types.
        ConditionalGuard::AtMostOneMember { path } => {
            let declared_size_at_most_one = match yaml_value_at_path(values_yaml_doc, path) {
                Some(YamlValue::Mapping(mapping)) => mapping.len() <= 1,
                Some(YamlValue::Sequence(items)) => items.len() <= 1,
                _ => true,
            };
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::foreign(serde_json::json!({
                    "maxProperties": 1,
                    "maxItems": 1,
                })),
                declared_size_at_most_one,
            )
        }
        // Exact: `keys X` aborts rendering for non-maps, so the body runs
        // only for mappings with enough members.
        ConditionalGuard::MinMembers { path, bound } => {
            let declared_matches = matches!(
                yaml_value_at_path(values_yaml_doc, path),
                Some(YamlValue::Mapping(mapping)) if mapping.len() as i64 >= *bound
            );
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::foreign(serde_json::json!({
                    "type": "object",
                    "minProperties": bound.max(&0),
                })),
                declared_matches,
            )
        }
        ConditionalGuard::Not(inner) => Some(SchemaNode::not(build_single_condition_fragment(
            inner,
            ancestor_segments,
            values_yaml_doc,
        )?)),
        ConditionalGuard::AllOf(guards) => {
            let clauses = build_condition_clauses(guards, ancestor_segments, values_yaml_doc);
            (!clauses.is_empty()).then(|| SchemaNode::all_of(clauses))
        }
        ConditionalGuard::AnyOf(guards) => {
            let clauses = build_condition_clauses(guards, ancestor_segments, values_yaml_doc);
            (!clauses.is_empty()).then(|| SchemaNode::any_of(clauses))
        }
    }
}

/// Whether a guard (and every nested guard) produces a condition fragment.
/// `build_condition_clauses` silently FILTERS unencodable guards, which is
/// safe for `if/then` arms (a narrower `if` applies the arm less often) but
/// unsound for terminal `then: false` clauses, where a dropped conjunct
/// widens the rejected set. Mirrors `build_single_condition_fragment`.
pub(crate) fn guard_encodes_fully(
    guard: &ConditionalGuard,
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
) -> bool {
    match guard {
        ConditionalGuard::Not(inner) => {
            guard_encodes_fully(inner, ancestor_segments, values_yaml_doc)
        }
        ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
            !guards.is_empty()
                && guards
                    .iter()
                    .all(|guard| guard_encodes_fully(guard, ancestor_segments, values_yaml_doc))
        }
        other => {
            build_single_condition_fragment(other, ancestor_segments, values_yaml_doc).is_some()
        }
    }
}

fn guard_value_enum_schema(value: &GuardValue) -> Option<SchemaNode> {
    guard_value_to_json(value).map(|value| SchemaNode::enum_values(vec![value]))
}

/// String spellings provably landing in (or out of) an int-cast region.
enum IntStringPreimage {
    /// Spellings certainly COERCING INTO the region (single-sign regions:
    /// the value must parse, so only clean bounded spellings qualify).
    Within(String),
    /// Spellings certainly coercing into the region are everything OUTSIDE
    /// this overapproximated escape language (mixed-sign regions contain
    /// 0, where every unparseable or overflowing spelling lands).
    Excluding(String),
}

/// The typed-integer bound leaf, widened with the string preimage when one
/// is representable.
fn int_bound_leaf_schema(
    keyword: &str,
    bound: i64,
    preimage: Option<IntStringPreimage>,
) -> SchemaNode {
    let integer = serde_json::json!({ "type": "integer", (keyword): bound });
    match preimage {
        Some(IntStringPreimage::Within(pattern)) => SchemaNode::foreign(serde_json::json!({
            "anyOf": [integer, { "type": "string", "pattern": pattern }]
        })),
        Some(IntStringPreimage::Excluding(pattern)) => SchemaNode::foreign(serde_json::json!({
            "anyOf": [integer, { "type": "string", "not": { "pattern": pattern } }]
        })),
        None => SchemaNode::foreign(integer),
    }
}

/// The string preimage of the `IntGt`/`IntLt` regions under Sprig's
/// `int`/`int64` coercion (`strconv.ParseInt` base 0 after trimming a
/// zero decimal tail; every parse failure and overflow coerces to 0).
///
/// Single-sign regions (0 outside the region) claim only spellings that
/// certainly PARSE into the region — clean decimal and radix forms with
/// enough significant digits. Mixed-sign regions (a positive `IntLt`
/// bound, a negative `IntGt` bound) contain 0, so every spelling except
/// a successful parse beyond the bound lands inside: they claim the
/// COMPLEMENT of an overapproximated escape language instead.
fn int_region_string_preimage(greater: bool, bound: i64) -> Option<IntStringPreimage> {
    if greater {
        if let Some(pattern) = decimal_strings_above(bound) {
            return Some(IntStringPreimage::Within(pattern));
        }
        // bound < 0: only a successful parse of a magnitude STRICTLY
        // beyond |bound| escapes the region.
        let escape = parse_magnitude_reaching(bound.unsigned_abs());
        Some(IntStringPreimage::Excluding(format!(
            "^-({escape})(\\.0*)?$"
        )))
    } else {
        if let Some(pattern) = decimal_strings_below(bound) {
            return Some(IntStringPreimage::Within(pattern));
        }
        // bound > 0: only a successful unsigned parse reaching the bound
        // escapes the region.
        let escape = parse_magnitude_reaching(bound.unsigned_abs());
        Some(IntStringPreimage::Excluding(format!(
            "^\\+?({escape})(\\.0*)?$"
        )))
    }
}

/// Overapproximation of every spelling whose successful parse reaches a
/// magnitude of at least `bound`: per radix family, any spelling long
/// enough that its maximal value `radix^chars − 1` reaches the bound.
/// Underscores and leading zeros only lower the parsed value (a leading
/// zero flips decimals to octal), so counting raw characters is safe for
/// an OVERapproximation — the complement lane claims only spellings that
/// certainly stay below.
fn parse_magnitude_reaching(bound: u64) -> String {
    // (prefix, first-char class, tail-char class, radix)
    let families: [(&str, &str, &str, u128); 4] = [
        ("", "[0-9]", "[0-9_]", 10),
        ("0[xX]", "[0-9a-fA-F_]", "[0-9a-fA-F_]", 16),
        ("0[bB]", "[01_]", "[01_]", 2),
        ("0[oO]", "[0-7_]", "[0-7_]", 8),
    ];
    let mut alternates = Vec::new();
    for (prefix, first, tail, radix) in families {
        // Smallest char count whose maximal value reaches the bound.
        let mut chars = 1usize;
        let mut max_value: u128 = radix - 1;
        while max_value < u128::from(bound) {
            max_value = max_value * radix + (radix - 1);
            chars += 1;
        }
        alternates.push(match chars - 1 {
            0 => format!("{prefix}{first}{tail}*"),
            more => format!("{prefix}{first}{tail}{{{more},}}"),
        });
    }
    alternates.join("|")
}

/// Radix spellings (hex, binary, explicit and legacy octal) certainly
/// parsing to a magnitude strictly above `bound`: a nonzero leading digit
/// with enough digits that the minimal value `radix^(digits−1)` already
/// exceeds the bound, capped where every spelling still parses without
/// overflow. Underscored and zero-padded spellings abstain.
fn radix_magnitudes_above(bound: u64) -> Vec<String> {
    // (prefix, lead class, digit class, radix, max significant digits)
    let families: [(&str, &str, &str, u128, usize); 3] = [
        ("0[xX]", "[1-9a-fA-F]", "[0-9a-fA-F]", 16, 15),
        ("0[bB]", "1", "[01]", 2, 62),
        ("0[oO]?", "[1-7]", "[0-7]", 8, 20),
    ];
    let mut alternates = Vec::new();
    for (prefix, lead, digit, radix, max_digits) in families {
        // Smallest digit count whose minimal value exceeds the bound.
        let mut digits = 1usize;
        let mut min_value: u128 = 1;
        while min_value <= u128::from(bound) {
            min_value *= radix;
            digits += 1;
        }
        if digits > max_digits {
            continue;
        }
        alternates.push(match (digits - 1, max_digits - 1) {
            (0, 0) => format!("{prefix}{lead}"),
            (0, high) => format!("{prefix}{lead}{digit}{{0,{high}}}"),
            (low, high) if low == high => format!("{prefix}{lead}{digit}{{{low}}}"),
            (low, high) => format!("{prefix}{lead}{digit}{{{low},{high}}}"),
        });
    }
    alternates
}

// Sprig's `int`/`int64` coercion parses strings through `strconv.ParseInt`
// with base detection, so a leading zero flips the base to octal and a
// non-decimal spelling coerces to 0. The single-sign preimage patterns
// below therefore claim clean spellings only — optional sign, no leading
// zero, no underscores — whose parsed value provably lands in the claimed
// region; ambiguous spellings abstain. Mixed-sign regions ride the
// complement lane in `int_region_string_preimage` instead.

/// Alternates matching unsigned decimal spellings with value strictly
/// greater than `bound` (`bound >= 0`), without leading zeros.
fn decimal_magnitude_above(bound: u64) -> String {
    let digits: Vec<u8> = bound.to_string().bytes().map(|byte| byte - b'0').collect();
    let mut alternates = vec![format!("[1-9][0-9]{{{},}}", digits.len())];
    for (index, &digit) in digits.iter().enumerate() {
        // The first digit needs no extra floor: a bound's leading digit is
        // nonzero (or the bound is 0 itself, where digit+1 = 1).
        let low = digit + 1;
        if low > 9 {
            continue;
        }
        let prefix: String = digits[..index]
            .iter()
            .map(|digit| char::from(b'0' + digit))
            .collect();
        let range = if low == 9 {
            "9".to_string()
        } else {
            format!("[{low}-9]")
        };
        let suffix = match digits.len() - index - 1 {
            0 => String::new(),
            1 => "[0-9]".to_string(),
            count => format!("[0-9]{{{count}}}"),
        };
        alternates.push(format!("{prefix}{range}{suffix}"));
    }
    alternates.join("|")
}

/// String spellings certainly parsing strictly above `bound`; only
/// non-negative bounds have a single-sign region.
fn decimal_strings_above(bound: i64) -> Option<String> {
    let bound = u64::try_from(bound).ok()?;
    let mut alternates = vec![decimal_magnitude_above(bound)];
    alternates.extend(radix_magnitudes_above(bound));
    Some(format!("^\\+?({})$", alternates.join("|")))
}

/// String spellings certainly parsing strictly below `bound`; only
/// non-positive bounds have a single-sign region.
fn decimal_strings_below(bound: i64) -> Option<String> {
    if bound > 0 {
        return None;
    }
    let magnitude = bound.unsigned_abs();
    let mut alternates = vec![decimal_magnitude_above(magnitude)];
    alternates.extend(radix_magnitudes_above(magnitude));
    if magnitude == 0 {
        // Below zero the magnitude is value-free, so zero-padded VALID
        // octal is admissible too. It must stay out of the general arm:
        // a zero-led spelling parses as octal, where an 8 or 9 digit is
        // a parse ERROR coercing to 0 ("-018" renders 0, not −18).
        alternates.push("0+[1-7][0-7]{0,19}".to_string());
    }
    Some(format!("^-({})$", alternates.join("|")))
}

/// The statically decided integer value of a declared default: a YAML
/// integer, or a clean signed decimal string spelling (no leading zero).
fn decimal_default_int_value(value: Option<&YamlValue>) -> Option<i64> {
    match value? {
        YamlValue::Number(number) => number.as_i64(),
        YamlValue::String(text) => {
            let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
            if unsigned.is_empty()
                || (unsigned.len() > 1 && unsigned.starts_with('0'))
                || !unsigned.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            text.trim_start_matches('+').parse::<i64>().ok()
        }
        _ => None,
    }
}

fn build_default_aware_leaf_condition_fragment(
    value_path: &str,
    ancestor_segments: &[String],
    leaf_schema: SchemaNode,
    default_matches: bool,
) -> Option<SchemaNode> {
    let segments = split_value_path(value_path);
    let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
    if relative_segments.is_empty() {
        return Some(leaf_schema);
    }
    let explicit = build_required_condition_fragment(&relative_segments, leaf_schema)?;
    if !default_matches {
        return Some(explicit);
    }
    let absent = build_required_condition_fragment(&relative_segments, SchemaNode::empty())
        .map(SchemaNode::not)?;
    Some(SchemaNode::any_of(vec![absent, explicit]))
}

pub(crate) fn value_references_helm_truthy(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, child)| {
            (key == "$ref"
                && child.as_str()
                    == Some(&format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}") as &str))
                || value_references_helm_truthy(child)
        }),
        Value::Array(items) => items.iter().any(value_references_helm_truthy),
        _ => false,
    }
}

/// Helm truthiness as one shared definition: every truthy/with condition
/// references it, which keeps the emitted `if` blocks small on charts with
/// many guarded renders.
pub(crate) const HELM_TRUTHY_DEFINITION_NAME: &str = "helm-truthy";

fn helm_truthy_condition_schema() -> SchemaNode {
    SchemaNode::foreign(serde_json::json!({
        "$ref": format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}")
    }))
}

pub(crate) fn helm_truthy_definition_schema() -> Value {
    SchemaNode::any_of(vec![
        SchemaNode::const_value(Value::Bool(true)),
        SchemaNode::typed(JsonSchemaType::Number).typed_keyword(
            "not",
            SchemaNode::const_value(Value::Number(0.into())).into_value(),
        ),
        SchemaNode::typed(JsonSchemaType::String)
            .typed_keyword("minLength", Value::Number(1.into())),
        SchemaNode::array().min_items(1),
        SchemaNode::object().min_properties(1),
    ])
    .into_value()
}

fn build_required_condition_fragment(
    path_segments: &[String],
    leaf_schema: SchemaNode,
) -> Option<SchemaNode> {
    let (head, tail) = path_segments.split_first()?;
    let child = if tail.is_empty() {
        leaf_schema
    } else {
        build_required_condition_fragment(tail, leaf_schema)?
    };
    // A member wildcard quantifies over EVERY iterated member: the
    // document-level if-side cannot address "the current member" of a
    // per-member dispatch, and an arm firing on fewer states than the real
    // condition is safe here (under a sibling `AtMostOneMember` bound the
    // universal test is exact — the signoz singleton env lane).
    if head == "*" {
        let child = child.into_value();
        return Some(SchemaNode::foreign(serde_json::json!({
            "additionalProperties": child,
            "items": child,
        })));
    }
    Some(
        SchemaNode::object()
            .require(head.clone())
            .property(head.clone(), child),
    )
}

pub(crate) fn evaluate_guard_set_on_values(
    guards: &[ConditionalGuard],
    values_yaml_doc: &YamlValue,
) -> Option<bool> {
    guards
        .iter()
        .map(|guard| evaluate_guard_on_values(guard, values_yaml_doc))
        .collect::<Option<Vec<_>>>()
        .map(|results| results.into_iter().all(|result| result))
}

fn evaluate_guard_on_values(guard: &ConditionalGuard, values_yaml_doc: &YamlValue) -> Option<bool> {
    match guard {
        ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
            Some(yaml_value_at_path(values_yaml_doc, path).is_some_and(yaml_value_is_truthy))
        }
        ConditionalGuard::Eq { path, value } => Some(guard_value_matches_optional_yaml(
            value,
            yaml_value_at_path(values_yaml_doc, path),
        )),
        ConditionalGuard::NotEq { path, value } => Some(!guard_value_matches_optional_yaml(
            value,
            yaml_value_at_path(values_yaml_doc, path),
        )),
        ConditionalGuard::Absent { path } => {
            Some(yaml_value_at_path(values_yaml_doc, path).is_none())
        }
        ConditionalGuard::IntGt { path, bound } => Some(
            decimal_default_int_value(yaml_value_at_path(values_yaml_doc, path))
                .is_some_and(|value| value > *bound),
        ),
        ConditionalGuard::IntLt { path, bound } => Some(
            decimal_default_int_value(yaml_value_at_path(values_yaml_doc, path))
                .is_some_and(|value| value < *bound),
        ),
        ConditionalGuard::HasKey { path, key } => Some(matches!(
            yaml_value_at_path(values_yaml_doc, path),
            Some(YamlValue::Mapping(mapping))
                if mapping.contains_key(YamlValue::String(key.clone()))
        )),
        ConditionalGuard::TypeIs { path, schema_type } => {
            let Some(value) = yaml_value_at_path(values_yaml_doc, path) else {
                return Some(false);
            };
            Some(matches_yaml_schema_type(value, schema_type))
        }
        ConditionalGuard::MatchesPattern { path, pattern } => {
            let value = yaml_value_at_path(values_yaml_doc, path)?.as_str()?;
            regex::Regex::new(pattern)
                .ok()
                .map(|regex| regex.is_match(value))
        }
        ConditionalGuard::ContainsMemberEquals {
            path,
            member,
            value,
        } => Some(declared_collection_contains_member_equals(
            yaml_value_at_path(values_yaml_doc, path),
            member,
            value,
        )),
        ConditionalGuard::AtMostOneMember { path } => {
            Some(match yaml_value_at_path(values_yaml_doc, path) {
                Some(YamlValue::Mapping(mapping)) => mapping.len() <= 1,
                Some(YamlValue::Sequence(items)) => items.len() <= 1,
                _ => true,
            })
        }
        ConditionalGuard::MinMembers { path, bound } => Some(matches!(
            yaml_value_at_path(values_yaml_doc, path),
            Some(YamlValue::Mapping(mapping)) if mapping.len() as i64 >= *bound
        )),
        ConditionalGuard::Not(inner) => {
            evaluate_guard_on_values(inner, values_yaml_doc).map(|v| !v)
        }
        ConditionalGuard::AllOf(guards) => evaluate_guard_set_on_values(guards, values_yaml_doc),
        ConditionalGuard::AnyOf(guards) => guards
            .iter()
            .map(|guard| evaluate_guard_on_values(guard, values_yaml_doc))
            .collect::<Option<Vec<_>>>()
            .map(|results| results.into_iter().any(|result| result)),
    }
}

/// Whether the declared collection already carries an iterated item whose
/// `member` equals `value` (list items or mapping member values).
fn declared_collection_contains_member_equals(
    yaml: Option<&YamlValue>,
    member: &str,
    value: &GuardValue,
) -> bool {
    let items: Vec<&YamlValue> = match yaml {
        Some(YamlValue::Sequence(items)) => items.iter().collect(),
        Some(YamlValue::Mapping(mapping)) => mapping.values().collect(),
        _ => return false,
    };
    items.iter().any(|item| {
        matches!(item, YamlValue::Mapping(mapping)
        if guard_value_matches_optional_yaml(
            value,
            mapping.get(YamlValue::String(member.to_string())),
        ))
    })
}

fn guard_value_matches_optional_yaml(value: &GuardValue, yaml: Option<&YamlValue>) -> bool {
    let Some(yaml) = yaml else {
        return matches!(value, GuardValue::Null);
    };
    match value {
        GuardValue::String(expected) => yaml.as_str() == Some(expected.as_str()),
        GuardValue::Bool(expected) => yaml.as_bool() == Some(*expected),
        GuardValue::Int(expected) => {
            yaml.as_i64() == Some(*expected)
                || (*expected >= 0 && yaml.as_u64() == Some(*expected as u64))
        }
        GuardValue::Float(expected) => {
            let Some(expected) = expected.parse::<f64>().ok() else {
                return false;
            };
            yaml.as_f64() == Some(expected)
        }
        GuardValue::Null => matches!(yaml, YamlValue::Null),
    }
}

fn yaml_value_is_truthy(value: &YamlValue) -> bool {
    match value {
        YamlValue::Null => false,
        YamlValue::Bool(value) => *value,
        YamlValue::Number(value) => {
            value.as_i64().is_some_and(|value| value != 0)
                || value.as_u64().is_some_and(|value| value != 0)
                || value.as_f64().is_some_and(|value| value != 0.0)
        }
        YamlValue::String(value) => !value.is_empty(),
        YamlValue::Sequence(value) => !value.is_empty(),
        YamlValue::Mapping(value) => !value.is_empty(),
        YamlValue::Tagged(_) => false,
    }
}

fn matches_yaml_schema_type(value: &YamlValue, schema_type: &str) -> bool {
    match schema_type {
        "array" => matches!(value, YamlValue::Sequence(_)),
        "boolean" => value.as_bool().is_some(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.as_f64().is_some(),
        "object" => matches!(value, YamlValue::Mapping(_)),
        "string" => value.as_str().is_some(),
        _ => false,
    }
}

fn strip_ancestor_prefix(
    path_segments: &[String],
    ancestor_segments: &[String],
) -> Option<Vec<String>> {
    path_segments
        .starts_with(ancestor_segments)
        .then(|| path_segments[ancestor_segments.len()..].to_vec())
}
