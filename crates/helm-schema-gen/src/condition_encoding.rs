use std::collections::BTreeSet;

use helm_schema_core::{ConditionalGuard, GuardValue};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::schema_model::guard_value_to_json;
use crate::schema_node::{JsonSchemaType, SchemaNode};
use crate::split_value_path;
use crate::values_yaml::yaml_value_at_path;

/// Which way a condition fragment may err where the encoding cannot be
/// exact.
///
/// A conditional ARM (`if C then S`) applies a claim, so C must hold
/// wherever the claim does and its leaves WIDEN. A TERMINAL clause
/// (`if C then false`) asserts that rendering aborts, so C must be
/// SUFFICIENT: a widened leaf would terminate documents that render.
/// `not` swaps the direction for everything below it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConditionPolarity {
    Widen,
    Narrow,
}

impl ConditionPolarity {
    fn flipped(self) -> Self {
        match self {
            Self::Widen => Self::Narrow,
            Self::Narrow => Self::Widen,
        }
    }
}

pub(crate) fn build_condition_clauses(
    guards: &[ConditionalGuard],
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
    absence: AbsenceDefaults<'_>,
    polarity: ConditionPolarity,
) -> Vec<SchemaNode> {
    guards
        .iter()
        .filter_map(|guard| {
            build_single_condition_fragment(
                guard,
                ancestor_segments,
                values_yaml_doc,
                absence,
                polarity,
            )
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
    absence: AbsenceDefaults<'_>,
    cache: &mut ConditionFragmentCache,
) -> Vec<SchemaNode> {
    guards
        .iter()
        .filter_map(|guard| {
            cache
                .entry((ancestor_segments.to_vec(), guard.clone()))
                .or_insert_with(|| {
                    build_single_condition_fragment(
                        guard,
                        ancestor_segments,
                        values_yaml_doc,
                        absence,
                        ConditionPolarity::Widen,
                    )
                })
                .clone()
        })
        .collect()
}

/// What a values document reads where it supplies nothing itself.
#[derive(Clone, Copy)]
pub(crate) struct AbsenceDefaults<'a> {
    /// The composed defaults MINUS everything the parent chart's own
    /// values.yaml declares: exactly the paths whose value a parent-level
    /// document supplies from a DEEPER coalesce stage when the key is
    /// missing. A missing parent-owned key was null-deleted and reads as
    /// nil; a missing dependency-owned key reads as the subchart's
    /// declared default.
    pub deeper_stage: &'a YamlValue,
    /// The dependency charts' own composed defaults, without that
    /// subtraction: what helm hands a DELETED dependency values root
    /// (`coalesceDeps` recreates the table and coalesces the subchart's
    /// values into it, while the parent's defaults for that root went with
    /// the deletion).
    pub dependency_refill: &'a YamlValue,
    /// The values roots those dependency charts own.
    pub dependency_roots: &'a BTreeSet<Vec<String>>,
}

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic lowering operation together makes its state transitions easier to audit"
)]
fn build_single_condition_fragment(
    guard: &ConditionalGuard,
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
    absence: AbsenceDefaults<'_>,
    polarity: ConditionPolarity,
) -> Option<SchemaNode> {
    let subchart_defaults_doc = absence.deeper_stage;
    let dependency_roots = absence.dependency_roots;
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
            // Only in the widening direction: the same residue the comment
            // above leaves unmodeled is exactly what a terminal clause must
            // not claim (velero declares `namespace: {labels: {}}`, and
            // deleting `labels` coalesces the parent back to a falsy empty
            // mapping, so its `len` guard never runs).
            let default_is_nonempty_mapping = polarity == ConditionPolarity::Widen
                && matches!(declared, Some(YamlValue::Mapping(mapping)) if !mapping.is_empty());
            let truthy = if default_is_nonempty_mapping {
                SchemaNode::any_of(vec![SchemaNode::object(), helm_truthy_condition_schema()])
            } else {
                helm_truthy_condition_schema()
            };
            // A missing parent-owned key was null-deleted and reads as
            // nil — Helm-falsy — while a missing dependency-owned key
            // reads as the subchart's own declared default.
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                truthy,
                yaml_value_at_path(subchart_defaults_doc, path).is_some_and(yaml_value_is_truthy),
            )
        }
        ConditionalGuard::Eq { path, value } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            guard_value_enum_schema(value)?,
            match yaml_value_at_path(subchart_defaults_doc, path) {
                Some(default) => guard_value_matches_optional_yaml(value, Some(default)),
                None => matches!(value, GuardValue::Null),
            },
        ),
        // A missing parent-owned key reads as nil, which differs from
        // every non-null literal; a dependency-owned key reads as its
        // subchart default (cilium's kvstoreMode membership rejects a
        // document whose null-deletion left the mode nil).
        ConditionalGuard::NotEq { path, value } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            guard_value_enum_schema(value).map(SchemaNode::not)?,
            match yaml_value_at_path(subchart_defaults_doc, path) {
                Some(default) => !guard_value_matches_optional_yaml(value, Some(default)),
                None => !matches!(value, GuardValue::Null),
            },
        ),
        // The member key is an OPAQUE property name (it may contain dots),
        // so it becomes a literal `required` entry below the segmented
        // collection path. A missing parent-owned collection reads as
        // nil, which holds no keys; a dependency-owned one holds exactly
        // the subchart default's keys.
        ConditionalGuard::HasKey { path, key } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            SchemaNode::foreign(serde_json::json!({
                "type": "object",
                "required": [key],
            })),
            matches!(
                yaml_value_at_path(subchart_defaults_doc, path),
                Some(YamlValue::Mapping(mapping))
                    if mapping.contains_key(YamlValue::String(key.clone()))
            ),
        ),
        // The guard is a sound SUBSET of its Sprig coercion by definition:
        // the explicit test admits raw integers, plus the string spellings
        // the coercion provably parses into the claimed region (jenkins'
        // `"5"` replicas abort like `5`; a mixed-sign region also collects
        // every unparsable spelling through the complement lane). A
        // missing parent-owned key reads as nil and coerces to 0, so it
        // satisfies the region exactly when 0 does; a dependency-owned
        // key coerces its subchart default.
        ConditionalGuard::IntGt { path, bound } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            int_bound_leaf_schema(
                "exclusiveMinimum",
                *bound,
                Some(int_region_string_preimage(true, *bound)),
            ),
            absent_coerced_int(subchart_defaults_doc, path) > *bound,
        ),
        ConditionalGuard::IntLt { path, bound } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            int_bound_leaf_schema(
                "exclusiveMaximum",
                *bound,
                Some(int_region_string_preimage(false, *bound)),
            ),
            absent_coerced_int(subchart_defaults_doc, path) < *bound,
        ),
        ConditionalGuard::Absent { path } => {
            let segments = split_value_path(path);
            let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
            if relative_segments.is_empty() {
                return None;
            }
            // Helm validates the COALESCED values — the same document the
            // templates render from — so a missing PARENT-owned key means
            // the render reads nil: coalescing already filled every
            // parent-declared default, and the only way such a key goes
            // missing is the user's explicit `null` deleting it (helm
            // null-deletion). A missing DEPENDENCY-owned key instead
            // reads as the subchart's declared default, which fills at
            // the subchart's own coalesce stage — only a non-null default
            // rescues it from absence. The explicit-null arm covers every
            // ownership: a surviving literal null reads as nil. `Absent`
            // deliberately counts null as absent — the nil-safe selector
            // lanes (`(.Values.x).leaf`) render at null exactly like at a
            // missing key. Strict key-presence (Sprig `hasKey`/`dig`
            // observability, where a present nil is PRESENT) is the
            // separate `HasKey` guard.
            // That deletion sticks below a dependency root too, but only
            // while the root SURVIVES: deleting the root hands the whole
            // subtree back to the subchart's defaults. The arms therefore
            // build inside the root's own schema, under its presence and
            // table type — a fragment already anchored below the root needs
            // neither, since the enclosing `properties` chain stops at a
            // null root before reaching it.
            let root_relative = enclosing_dependency_root(dependency_roots, &segments)
                .and_then(|root| strip_ancestor_prefix(root, ancestor_segments));
            let arm_segments =
                relative_segments.get(root_relative.as_ref().map_or(0, Vec::len)..)?;
            let explicit_null = build_required_condition_fragment(
                arm_segments,
                SchemaNode::enum_values(vec![Value::Null]),
            )?;
            let subchart_default_fills = matches!(yaml_value_at_path(subchart_defaults_doc, path), Some(value) if !matches!(value, YamlValue::Null));
            let deleted = if subchart_default_fills {
                explicit_null
            } else {
                let missing = SchemaNode::not(build_required_condition_fragment(
                    arm_segments,
                    SchemaNode::empty(),
                )?);
                SchemaNode::any_of(vec![missing, explicit_null])
            };
            let Some(root_relative) = root_relative else {
                return Some(deleted);
            };
            let live_root = SchemaNode::all_of(vec![SchemaNode::type_named("object"), deleted]);
            if root_relative.is_empty() {
                // The clause anchors AT the root, so the fragment states the
                // root's own table state and the refilled states stay out of
                // its reach.
                return Some(live_root);
            }
            let under_live_root = build_required_condition_fragment(&root_relative, live_root)?;
            let refill_fills = matches!(yaml_value_at_path(absence.dependency_refill, path), Some(value) if !matches!(value, YamlValue::Null));
            if refill_fills {
                return Some(under_live_root);
            }
            // A parent-only key the subchart never declares stays gone
            // through the refill too, so the deleted root reads nil as
            // well (kube-prometheus-stack's `grafana.operator.*`, which
            // the grafana chart knows nothing about).
            let root_gone = dependency_root_gone_condition(&root_relative)?;
            Some(SchemaNode::any_of(vec![under_live_root, root_gone]))
        }
        ConditionalGuard::TypeIs { path, schema_type } => {
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::type_named(schema_type),
                match yaml_value_at_path(subchart_defaults_doc, path) {
                    Some(default) => matches_yaml_schema_type(default, schema_type),
                    None => schema_type == "null",
                },
            )
        }
        ConditionalGuard::MatchesPattern { path, pattern } => {
            let ecma_pattern = crate::path_resolver::ecma_compatible_pattern(pattern)?;
            let absent_matches = yaml_value_at_path(subchart_defaults_doc, path)
                .and_then(YamlValue::as_str)
                .is_some_and(|value| {
                    regex::Regex::new(pattern).is_ok_and(|regex| regex.is_match(value))
                });
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::foreign(serde_json::json!({
                    "pattern": ecma_pattern,
                    "type": "string",
                })),
                absent_matches,
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
                    yaml_value_at_path(subchart_defaults_doc, path),
                    member,
                    value,
                ),
            )
        }
        ConditionalGuard::ContainsTruthyMember { path, member } => {
            let item = SchemaNode::foreign(serde_json::json!({
                "type": "object",
                "properties": { member: helm_truthy_condition_schema().into_value() },
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
                declared_collection_contains_truthy_member(
                    yaml_value_at_path(subchart_defaults_doc, path),
                    member,
                ),
            )
        }
        // Sprig `has` over a values list: false on nil, abort on
        // non-lists, so the guard holds exactly for arrays carrying the
        // literal. A missing parent-owned key reads as nil (never a
        // member); a dependency-owned key reads as its subchart default
        // list.
        ConditionalGuard::ContainsEquals { path, value } => {
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::foreign(serde_json::json!({
                    "type": "array",
                    "contains": guard_value_enum_schema(value)?.into_value(),
                })),
                declared_list_contains_value(
                    yaml_value_at_path(subchart_defaults_doc, path),
                    value,
                ),
            )
        }
        // A non-collection value never iterates, so the "at most one
        // member" bound constrains only maps and lists; both size keywords
        // are no-ops on other instance types.
        ConditionalGuard::AtMostOneMember { path } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            SchemaNode::foreign(serde_json::json!({
                "maxProperties": 1,
                "maxItems": 1,
            })),
            match yaml_value_at_path(subchart_defaults_doc, path) {
                Some(YamlValue::Mapping(mapping)) => mapping.len() <= 1,
                Some(YamlValue::Sequence(items)) => items.len() <= 1,
                _ => true,
            },
        ),
        // Exact: `keys X` aborts rendering for non-maps, so the body runs
        // only for mappings with enough members.
        ConditionalGuard::MinMembers { path, bound } => {
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::foreign(serde_json::json!({
                    "type": "object",
                    "minProperties": bound.max(&0),
                })),
                matches!(
                    yaml_value_at_path(subchart_defaults_doc, path),
                    Some(YamlValue::Mapping(mapping))
                        if *bound <= 0
                            || usize::try_from(*bound).is_ok_and(|bound| mapping.len() >= bound)
                ),
            )
        }
        ConditionalGuard::Not(inner) => {
            let negated = SchemaNode::not(build_single_condition_fragment(
                inner,
                ancestor_segments,
                values_yaml_doc,
                absence,
                polarity.flipped(),
            )?);
            // A negated MEMBER-quantified guard admits a second exact arm:
            // beside "not every member satisfies it" (which is vacuously
            // FALSE on an empty or absent collection), "every member
            // violates it" holds vacuously TRUE there. Both arms fire only
            // on states where some iteration escapes the guard, so the
            // disjunction stays inside the if-position discipline — an arm
            // firing on fewer states than the real per-member condition is
            // safe — while covering airflow's default empty
            // `workers.celery.sets`, whose per-set merge leaves every
            // deeper layer unshadowed.
            if ancestor_segments.is_empty()
                && let Some(quantified) = negated_member_guard_fragment(inner)
            {
                return Some(SchemaNode::any_of(vec![negated, quantified]));
            }
            Some(negated)
        }
        ConditionalGuard::AllOf(guards) => {
            let clauses = build_condition_clauses(
                guards,
                ancestor_segments,
                values_yaml_doc,
                absence,
                polarity,
            );
            (!clauses.is_empty()).then(|| SchemaNode::all_of(clauses))
        }
        ConditionalGuard::AnyOf(guards) => {
            let clauses = build_condition_clauses(
                guards,
                ancestor_segments,
                values_yaml_doc,
                absence,
                polarity,
            );
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
    absence: AbsenceDefaults<'_>,
) -> bool {
    match guard {
        ConditionalGuard::Not(inner) => {
            guard_encodes_fully(inner, ancestor_segments, values_yaml_doc, absence)
        }
        ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
            !guards.is_empty()
                && guards.iter().all(|guard| {
                    guard_encodes_fully(guard, ancestor_segments, values_yaml_doc, absence)
                })
        }
        other => build_single_condition_fragment(
            other,
            ancestor_segments,
            values_yaml_doc,
            absence,
            ConditionPolarity::Widen,
        )
        .is_some(),
    }
}

/// The document says a dependency values root is gone: absent, null, or a
/// non-table `coalesceDeps` refuses outright. Helm recreates the root in
/// each of those states and coalesces the subchart's own values into it.
pub(crate) fn dependency_root_gone_condition(root_segments: &[String]) -> Option<SchemaNode> {
    build_required_condition_fragment(root_segments, SchemaNode::type_named("object"))
        .map(SchemaNode::not)
}

/// The dependency values root whose DELETION this terminal clause proves
/// fatal. Everything under a deleted root reads the refill — the subchart's
/// own values, with the parent's defaults for that root gone — so a clause
/// whose guards ALL hold there terminates the render whatever the rest of
/// the document says. That is a document-level fact, and a clause anchored
/// under `properties.<root>` (its guards share an ancestor inside the root)
/// can never state it: no document missing the root enters that subschema.
///
/// A guard reaching outside the root abstains, since its truth is the
/// document's rather than the refill's. So does a re-rooted `global`
/// spelling — helm injects the PARENT's globals into the recreated table —
/// and a wildcard path, which names no single value to read.
pub(crate) fn deleted_dependency_root_terminates<'a>(
    guards: &[ConditionalGuard],
    absence: AbsenceDefaults<'a>,
) -> Option<&'a [String]> {
    let paths = guards
        .iter()
        .flat_map(ConditionalGuard::value_paths)
        .map(|path| split_value_path(&path))
        .collect::<Vec<_>>();
    if paths.is_empty()
        || paths.iter().any(|path| {
            path.iter()
                .any(|segment| segment == "*" || segment == "global")
        })
    {
        return None;
    }
    let root = absence
        .dependency_roots
        .iter()
        .filter(|root| paths.iter().all(|path| path.starts_with(root)))
        .max_by_key(|root| root.len())?;
    (evaluate_guard_set_on_values(guards, absence.dependency_refill) == Some(true))
        .then_some(root.as_slice())
}

/// The deepest dependency values root STRICTLY enclosing `segments`, whose
/// presence every absence claim below it hangs on. Nested subcharts own
/// nested roots (`clickhouse.zookeeper`), and each refills independently,
/// so the innermost one carries the claim — its own required chain already
/// keeps the outer roots present.
fn enclosing_dependency_root<'a>(
    dependency_roots: &'a BTreeSet<Vec<String>>,
    segments: &[String],
) -> Option<&'a [String]> {
    dependency_roots
        .iter()
        .filter(|root| root.len() < segments.len() && segments.starts_with(root))
        .max_by_key(|root| root.len())
        .map(Vec::as_slice)
}

fn guard_value_enum_schema(value: &GuardValue) -> Option<SchemaNode> {
    guard_value_to_json(value).map(|value| SchemaNode::enum_values(vec![value]))
}

/// String spellings provably landing in (or out of) an int-cast region.
pub(crate) enum IntStringPreimage {
    /// Spellings certainly COERCING INTO the region (single-sign regions:
    /// the value must parse, so only clean bounded spellings qualify).
    Within(String),
    /// Spellings certainly coercing into the region are everything OUTSIDE
    /// this overapproximated escape language (mixed-sign regions contain
    /// 0, where every unparsable or overflowing spelling lands).
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
pub(crate) fn int_region_string_preimage(greater: bool, bound: i64) -> IntStringPreimage {
    if greater {
        if let Some(pattern) = decimal_strings_above(bound) {
            return IntStringPreimage::Within(pattern);
        }
        // bound < 0: only a successful parse of a magnitude STRICTLY
        // beyond |bound| escapes the region.
        let escape = parse_magnitude_reaching(bound.unsigned_abs());
        IntStringPreimage::Excluding(format!("^-({escape})(\\.0*)?$"))
    } else {
        if let Some(pattern) = decimal_strings_below(bound) {
            return IntStringPreimage::Within(pattern);
        }
        // bound > 0: only a successful unsigned parse reaching the bound
        // escapes the region.
        let escape = parse_magnitude_reaching(bound.unsigned_abs());
        IntStringPreimage::Excluding(format!("^\\+?({escape})(\\.0*)?$"))
    }
}

/// Overapproximation of every spelling whose successful parse reaches a
/// magnitude of at least `bound` (`bound ≥ 1`): the EXACT clean-spelling
/// windows strictly above `bound − 1` per radix family, plus a bounded
/// residue for the spellings the windows deliberately abstain on —
/// underscored forms (Go permits `2_55`) and the one boundary length per
/// family where a maximal-length spelling may or may not overflow int64.
/// Anything longer certainly overflows and coerces to 0, so it stays OUT
/// of the escape and the complement lane claims it.
fn parse_magnitude_reaching(bound: u64) -> String {
    let mut alternates = magnitude_windows_above(bound.saturating_sub(1));
    // Underscored spellings parse to at most their digit reading, but the
    // windows cannot see through the insertions, so any underscored form
    // carrying a nonzero digit stays a possible escape.
    for (prefix, digit, nonzero) in [
        ("", "[0-9_]", "[1-9]"),
        ("0[xX]", "[0-9a-fA-F_]", "[1-9a-fA-F]"),
        ("0[bB]", "[01_]", "1"),
        ("0[oO]?", "[0-7_]", "[1-7]"),
    ] {
        alternates.push(format!("{prefix}{digit}*{nonzero}{digit}*_{digit}*"));
        alternates.push(format!("{prefix}{digit}*_{digit}*{nonzero}{digit}*"));
    }
    // Boundary lengths: one significant digit past each family's certain
    // cap, where the parse may still land within int64 (a 19-digit decimal
    // reaches 9.2e18 or overflows, depending on the digits). Anything
    // longer certainly overflows.
    alternates.push("[1-9][0-9]{18}".to_string());
    alternates.push("0[xX]0*[1-9a-fA-F][0-9a-fA-F]{15}".to_string());
    alternates.push("0[bB]0*1[01]{62}".to_string());
    alternates.push("0[oO]?0*[1-7][0-7]{20}".to_string());
    alternates.join("|")
}

/// Exact unsigned windows strictly above `bound` across every radix
/// family Go's base-0 parse accepts: clean decimal (no leading zero) plus
/// zero-pad-tolerant hex, binary, and octal (explicit `0o` and legacy
/// leading-zero) forms. Every claimed spelling certainly parses within
/// int64, so the windows are usable in claim position.
fn magnitude_windows_above(bound: u64) -> Vec<String> {
    let mut alternates = Vec::new();
    alternates.extend(radix_windows_above(bound, "", 10, 18, false));
    alternates.extend(radix_windows_above(bound, "0[xX]", 16, 15, true));
    alternates.extend(radix_windows_above(bound, "0[bB]", 2, 62, true));
    alternates.extend(radix_windows_above(bound, "0[oO]?", 8, 20, true));
    alternates
}

/// Per-position digit windows matching exactly the `prefix`-family
/// spellings whose value is strictly above `bound`, capped at `max_digits`
/// significant digits so every match parses without int64 overflow. The
/// zero-pad lane admits `0x0ff`-style padding, which Go reads without a
/// value change; decimal must stay unpadded (a leading zero flips the
/// base to octal).
fn radix_windows_above(
    bound: u64,
    prefix: &str,
    radix: u64,
    max_digits: usize,
    zero_padded: bool,
) -> Vec<String> {
    let pad = if zero_padded { "0*" } else { "" };
    let digits = radix_digits(bound, radix);
    let mut alternates = Vec::new();
    // Longer spellings: a nonzero lead with more significant digits than
    // the bound is strictly above it.
    if digits.len() < max_digits {
        let lead = radix_digit_range(1, radix).unwrap_or_default();
        let any = radix_digit_range(0, radix).unwrap_or_default();
        let low = digits.len();
        let high = max_digits - 1;
        let tail = if low == high {
            format!("{any}{{{low}}}")
        } else {
            format!("{any}{{{low},{high}}}")
        };
        alternates.push(format!("{prefix}{pad}{lead}{tail}"));
    }
    // Same-length windows: literal bound digits up to position `i`, one
    // digit strictly above the bound's, then any digits.
    if digits.len() <= max_digits {
        for index in 0..digits.len() {
            let Some(digit) = digits.get(index) else {
                continue;
            };
            let Some(class) = radix_digit_range(*digit + 1, radix) else {
                continue;
            };
            let Some(prefix_digits) = digits.get(..index) else {
                continue;
            };
            let lead: String = prefix_digits
                .iter()
                .map(|&digit| radix_digit(digit))
                .collect();
            let any = radix_digit_range(0, radix).unwrap_or_default();
            let suffix = match digits.len() - index - 1 {
                0 => String::new(),
                1 => any,
                count => format!("{any}{{{count}}}"),
            };
            alternates.push(format!("{prefix}{pad}{lead}{class}{suffix}"));
        }
    }
    alternates
}

/// The radix digits of `value`, most significant first (`[0]` for zero).
fn radix_digits(value: u64, radix: u64) -> Vec<u64> {
    let mut digits = Vec::new();
    let mut rest = value;
    loop {
        digits.push(rest % radix);
        rest /= radix;
        if rest == 0 {
            break;
        }
    }
    digits.reverse();
    digits
}

/// One literal radix digit as pattern text; hex letters match both cases.
fn radix_digit(value: u64) -> String {
    if let 0..=9 = value {
        value.to_string()
    } else {
        let letter = char::from(b'a' + u8::try_from(value - 10).unwrap_or(0));
        format!("[{letter}{}]", letter.to_ascii_uppercase())
    }
}

/// The digit class `[low ..= radix-1]` as pattern text, or `None` when the
/// range is empty; hex letter digits match both cases.
fn radix_digit_range(low: u64, radix: u64) -> Option<String> {
    let high = radix - 1;
    if low > high {
        return None;
    }
    if radix <= 10 {
        return Some(match (low, high) {
            (low, high) if low == high => low.to_string(),
            (low, high) => format!("[{low}-{high}]"),
        });
    }
    let decimal_part = match low {
        0..=8 => format!("{low}-9"),
        9 => "9".to_string(),
        _ => String::new(),
    };
    let letter_low = char::from(b'a' + u8::try_from(low.max(10) - 10).unwrap_or(0));
    let letter_part = if letter_low == 'f' {
        "fF".to_string()
    } else {
        format!("{letter_low}-f{}-F", letter_low.to_ascii_uppercase())
    };
    Some(format!("[{decimal_part}{letter_part}]"))
}

// Sprig's `int`/`int64` coercion parses strings through `strconv.ParseInt`
// with base detection, so a leading zero flips the base to octal and a
// non-decimal spelling coerces to 0. The single-sign preimage patterns
// below therefore claim clean spellings only — optional sign, no
// underscores — whose parsed value provably lands in the claimed region;
// ambiguous spellings abstain. Mixed-sign regions ride the complement
// lane in `int_region_string_preimage` instead.

/// String spellings certainly parsing strictly above `bound`; only
/// non-negative bounds have a single-sign region.
pub(crate) fn decimal_strings_above(bound: i64) -> Option<String> {
    let bound = u64::try_from(bound).ok()?;
    let alternates = magnitude_windows_above(bound);
    if alternates.is_empty() {
        return None;
    }
    Some(format!("^\\+?({})$", alternates.join("|")))
}

/// String spellings certainly parsing strictly below `bound`; only
/// non-positive bounds have a single-sign region.
pub(crate) fn decimal_strings_below(bound: i64) -> Option<String> {
    if bound > 0 {
        return None;
    }
    let alternates = magnitude_windows_above(bound.unsigned_abs());
    if alternates.is_empty() {
        return None;
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

/// The Sprig `int` coercion of the value the render reads when `path` is
/// missing from the validated document: the subchart default for
/// dependency-owned paths, nil (0) otherwise. Non-decimal defaults
/// coerce to 0 like every unparsable spelling.
fn absent_coerced_int(subchart_defaults_doc: &YamlValue, path: &str) -> i64 {
    decimal_default_int_value(yaml_value_at_path(subchart_defaults_doc, path)).unwrap_or(0)
}

/// A leaf condition over the COALESCED values document helm validates.
/// `absent_holds` states whether the guard is satisfied when the key is
/// MISSING there: coalescing already filled every declared default, so a
/// missing key always reads as nil at render (helm null-deletion is the
/// only way a declared key goes missing), and each guard decides what nil
/// means for it — Helm-falsy for truthiness, 0 for the int-cast regions,
/// equal only to the null literal for equalities.
fn build_default_aware_leaf_condition_fragment(
    value_path: &str,
    ancestor_segments: &[String],
    leaf_schema: SchemaNode,
    absent_holds: bool,
) -> Option<SchemaNode> {
    let segments = split_value_path(value_path);
    let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
    if relative_segments.is_empty() {
        return Some(leaf_schema);
    }
    let explicit = build_required_condition_fragment(&relative_segments, leaf_schema)?;
    if !absent_holds {
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
pub(crate) const HELM_TRUTHY_DEFINITION_NAME: &str = "t";

pub(crate) fn helm_truthy_condition_schema() -> SchemaNode {
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

/// The "every member VIOLATES the guard" fragment of a member-quantified
/// leaf guard (`Truthy`/`With`/`HasKey` at a `parent.*.suffix` path):
/// the chain above the star carries no presence requirement — an absent
/// or empty collection satisfies the universal vacuously — while each
/// member's suffix keeps the positive fragment's requirements under the
/// negation, so a member that satisfies the guard falsifies the whole
/// fragment. `None` for guards outside the supported leaves or without a
/// member wildcard.
fn negated_member_guard_fragment(guard: &ConditionalGuard) -> Option<SchemaNode> {
    let (path, leaf) = match guard {
        ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
            (path, helm_truthy_condition_schema())
        }
        ConditionalGuard::HasKey { path, key } => (path, SchemaNode::object().require(key.clone())),
        _ => return None,
    };
    let segments = split_value_path(path);
    let star = segments.iter().position(|segment| segment == "*")?;
    let suffix = segments.get(star + 1..)?;
    let member_positive = if suffix.is_empty() {
        leaf
    } else {
        build_required_condition_fragment(suffix, leaf)?
    };
    let member = SchemaNode::not(member_positive).into_value();
    let mut fragment = SchemaNode::foreign(serde_json::json!({
        "additionalProperties": member,
        "items": member,
    }));
    for segment in segments.get(..star)?.iter().rev() {
        fragment = SchemaNode::object().property(segment.clone(), fragment);
    }
    Some(fragment)
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

/// The raw input path is missing or null before any chart-authored values
/// reconstruction supplies a fallback.
pub(crate) fn input_path_absent_condition(value_path: &str) -> Option<SchemaNode> {
    let segments = split_value_path(value_path);
    let present_non_null = build_required_condition_fragment(
        &segments,
        SchemaNode::not(SchemaNode::enum_values(vec![Value::Null])),
    )?;
    Some(SchemaNode::not(present_non_null))
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
        ConditionalGuard::ContainsTruthyMember { path, member } => {
            Some(declared_collection_contains_truthy_member(
                yaml_value_at_path(values_yaml_doc, path),
                member,
            ))
        }
        ConditionalGuard::ContainsEquals { path, value } => Some(declared_list_contains_value(
            yaml_value_at_path(values_yaml_doc, path),
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
            Some(YamlValue::Mapping(mapping))
                if *bound <= 0
                    || usize::try_from(*bound).is_ok_and(|bound| mapping.len() >= bound)
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

/// Whether the declared list carries an item equal to the scalar literal
/// (Sprig `has` deep equality; non-lists never hold a member).
fn declared_list_contains_value(yaml: Option<&YamlValue>, value: &GuardValue) -> bool {
    matches!(yaml, Some(YamlValue::Sequence(items))
        if items
            .iter()
            .any(|item| guard_value_matches_optional_yaml(value, Some(item))))
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

fn declared_collection_contains_truthy_member(yaml: Option<&YamlValue>, member: &str) -> bool {
    let items: Vec<&YamlValue> = match yaml {
        Some(YamlValue::Sequence(items)) => items.iter().collect(),
        Some(YamlValue::Mapping(mapping)) => mapping.values().collect(),
        _ => return false,
    };
    items.iter().any(|item| {
        matches!(item, YamlValue::Mapping(mapping)
            if mapping
                .get(YamlValue::String(member.to_string()))
                .is_some_and(yaml_value_is_truthy))
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
                || u64::try_from(*expected)
                    .ok()
                    .is_some_and(|expected| yaml.as_u64() == Some(expected))
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
        YamlValue::Null | YamlValue::Tagged(_) => false,
        YamlValue::Bool(value) => *value,
        YamlValue::Number(value) => {
            value.as_i64().is_some_and(|value| value != 0)
                || value.as_u64().is_some_and(|value| value != 0)
                || value.as_f64().is_some_and(|value| value != 0.0)
        }
        YamlValue::String(value) => !value.is_empty(),
        YamlValue::Sequence(value) => !value.is_empty(),
        YamlValue::Mapping(value) => !value.is_empty(),
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
        .strip_prefix(ancestor_segments)
        .map(<[String]>::to_vec)
}
