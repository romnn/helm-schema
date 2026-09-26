//! The documents `helm lint` validates, as further must-accept documents.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{ConditionalGuard, Segment, ValuesPath};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::condition_encoding::{
    AbsenceDefaults, ConditionPolarity, HELM_TRUTHY_DEFINITION_NAME, build_condition_clauses,
    evaluate_guard_set_on_values, helm_truthy_definition_schema, input_path_present_condition,
    value_references_helm_truthy,
};
use crate::emission_report::{LintDocument, LintOutcome, LintWithdrawal};
use crate::schema_node::SchemaNode;

/// How a constraint encodes its guards, so the lint projection is encoded
/// alike.
#[derive(Clone, Copy)]
pub(crate) struct GuardEncoding<'a> {
    pub(crate) values_yaml_doc: &'a YamlValue,
    pub(crate) absence: AbsenceDefaults<'a>,
    pub(crate) polarity: ConditionPolarity,
}

/// Relaxes conditional constraints that `helm lint` documents fail.
///
/// `helm template` validates the coalesced values document, but `helm lint`
/// validates `CoalesceTables(overrides, values.yaml)`: the root values with
/// the user's override files and never with dependency defaults (Helm
/// v4.2.3 `pkg/chart/v2/lint/rules/values.go:62-68`). A constraint is
/// relaxed when one of two lint documents fails it while the coalesced
/// defaults satisfy it:
///
/// - the root `values.yaml` as written, judged by the whole constraint: the
///   document `helm lint` validates without overrides;
/// - the lint floor, the coalesced defaults restricted to the members the
///   root declares, with the root's nulls kept (`CoalesceTables` keeps
///   them where coalescing deletes them), judged by the constraint's lint
///   projection: the guards that already hold on the floor stay, and every
///   other guard is taken as switched on, since an override can set any
///   key, including one only a dependency declares. Setting a key creates
///   every table above it, so the floor is also judged with each table the
///   dependency defaults supply along the constraint's paths created empty:
///   the anchor and the parents of the members its guards test and its
///   `then` constrains, with every member table at a ranged segment.
///
/// A member contract, whose member conditions sit inside `then` where the
/// projection cannot switch them on, fails the lint floor at each key it
/// requires of a member that the floor lacks and the dependency defaults
/// supply: an override can switch the conditions on for that member.
///
/// The failing documents differ from the coalesced defaults only in the
/// presence of some of the constraint's paths: the ones only a dependency
/// supplies, and the root's nulls. The constraint is therefore conditioned
/// on those paths being present and non-null, which every failing lint
/// document falsifies, so its checks still apply wherever the values are
/// supplied. Only when no such path can be encoded is it withdrawn. Each
/// relaxation is recorded.
///
/// A constraint the coalesced defaults violate too is kept: the chart's own
/// defaults are rejected either way.
pub(crate) struct UncoalescedRootGate<'a> {
    uncoalesced_root: Option<&'a Value>,
    lint_floor: Option<(Value, YamlValue)>,
    coalesced_defaults: &'a Value,
    definitions: &'a BTreeMap<String, Value>,
    relaxed: Vec<LintWithdrawal>,
}

impl<'a> UncoalescedRootGate<'a> {
    pub(crate) fn new(
        uncoalesced_root: Option<&'a Value>,
        coalesced_defaults: &'a Value,
        definitions: &'a BTreeMap<String, Value>,
    ) -> Self {
        let lint_floor = uncoalesced_root.map(|root| {
            let floor = root_declared_part(coalesced_defaults, root);
            let floor_yaml = serde_yaml::to_value(&floor).unwrap_or(YamlValue::Null);
            (floor, floor_yaml)
        });
        Self {
            uncoalesced_root,
            lint_floor,
            coalesced_defaults,
            definitions,
            relaxed: Vec::new(),
        }
    }

    /// The `then` part `if condition then then_schema`, anchored at
    /// `ancestor_segments`, keeps under the lint contract: unchanged,
    /// conditioned on the paths the lint documents lack, or `None` when the
    /// constraint is withdrawn. `condition` encodes `guards` with
    /// `encoding`.
    pub(crate) fn lint_safe_then(
        &mut self,
        ancestor_segments: &[String],
        guards: &[ConditionalGuard],
        condition: &SchemaNode,
        then_schema: &SchemaNode,
        encoding: GuardEncoding<'_>,
    ) -> Option<SchemaNode> {
        let Some(uncoalesced_root) = self.uncoalesced_root else {
            return Some(then_schema.clone());
        };
        let anchor = ValuesPath::from_segments(
            ancestor_segments
                .iter()
                .map(|segment| Segment::from_encoded_component(segment)),
        );
        let then_value = then_schema.clone().into_value();
        let mut document = None;
        let mut deciding_paths = BTreeSet::new();
        if self.root_fails(uncoalesced_root, ancestor_segments, condition, then_schema) {
            document = Some(LintDocument::Root);
            self.collect_deciding_paths(
                uncoalesced_root,
                guards,
                &then_value,
                &anchor,
                &mut deciding_paths,
            );
        }
        if let Some((held, failing)) =
            self.floor_failure(&anchor, ancestor_segments, guards, then_schema, encoding)
        {
            document.get_or_insert(LintDocument::Floor);
            for floor in &failing {
                self.collect_deciding_paths(
                    floor,
                    &held,
                    &then_value,
                    &anchor,
                    &mut deciding_paths,
                );
            }
        }
        let lacking = self.member_requirements_the_floor_lacks(&anchor, &then_value);
        if !lacking.is_empty()
            && self.coalesced_satisfies(ancestor_segments, condition, then_schema)
        {
            document.get_or_insert(LintDocument::Floor);
            deciding_paths.extend(lacking);
        }
        let Some(document) = document else {
            return Some(then_schema.clone());
        };
        // A guard testing a deciding path for absence contradicts its
        // presence: conditioning would leave a constraint that never fires.
        let mut encodable = !deciding_paths.is_empty();
        for guard in guards {
            if let ConditionalGuard::Absent { path } = guard
                && deciding_paths.contains(path)
            {
                encodable = false;
            }
        }
        let mut presence = Vec::new();
        for path in &deciding_paths {
            match input_path_present_condition(path, ancestor_segments) {
                Some(present) => presence.push(present),
                None => encodable = false,
            }
        }
        let (outcome, kept) = if encodable {
            let conditioned = SchemaNode::foreign(serde_json::json!({
                "if": SchemaNode::all_of(presence).into_value(),
                "then": then_value,
            }));
            (LintOutcome::Conditioned, Some(conditioned))
        } else {
            (LintOutcome::Withdrawn, None)
        };
        tracing::debug!(
            anchor = anchor.encode(),
            ?document,
            ?outcome,
            "relaxed a constraint that a helm lint document fails"
        );
        self.relaxed.push(LintWithdrawal {
            anchor,
            document,
            deciding_paths: deciding_paths.into_iter().collect(),
            outcome,
        });
        kept
    }

    /// Adds the constraint's paths whose presence differs between `judged`
    /// and the coalesced defaults: absent or null in `judged` where the
    /// defaults supply a value, or a null in `judged` that coalescing
    /// deletes.
    fn collect_deciding_paths(
        &self,
        judged: &Value,
        guards: &[ConditionalGuard],
        then_value: &Value,
        anchor: &ValuesPath,
        deciding_paths: &mut BTreeSet<ValuesPath>,
    ) {
        let mut candidates = BTreeSet::new();
        for guard in guards {
            candidates.extend(guard.value_paths());
            collect_tested_keys(guard, &mut candidates);
        }
        collect_property_paths(then_value, anchor, &mut candidates);
        for path in candidates {
            let segments = path.segments().collect::<Vec<_>>();
            let judged_state = presence_at(judged, &segments);
            let coalesced_state = presence_at(self.coalesced_defaults, &segments);
            let differs = matches!(
                (judged_state, coalesced_state),
                (Presence::Missing | Presence::Null, Presence::Value)
                    | (Presence::Null, Presence::Missing)
            );
            if differs {
                deciding_paths.insert(path);
            }
        }
    }

    /// Whether the root document as written fails the whole constraint.
    fn root_fails(
        &self,
        root: &Value,
        ancestor_segments: &[String],
        condition: &SchemaNode,
        then_schema: &SchemaNode,
    ) -> bool {
        let mut instances = Vec::new();
        collect_anchored_instances(root, ancestor_segments, &mut instances);
        let mut coalesced_instances = Vec::new();
        collect_anchored_instances(
            self.coalesced_defaults,
            ancestor_segments,
            &mut coalesced_instances,
        );
        self.fails_only_lint(&instances, &coalesced_instances, condition, then_schema)
    }

    /// The guards already holding on the lint floor and the floor documents
    /// failing the constraint's lint projection (those guards, every other
    /// guard switched on): the floor itself and the floor with the tables
    /// an override creates along the constraint's paths.
    fn floor_failure(
        &self,
        anchor: &ValuesPath,
        ancestor_segments: &[String],
        guards: &[ConditionalGuard],
        then_schema: &SchemaNode,
        encoding: GuardEncoding<'_>,
    ) -> Option<(Vec<ConditionalGuard>, Vec<Value>)> {
        let (floor, floor_yaml) = self.lint_floor.as_ref()?;
        let mut held = Vec::new();
        let mut created = floor.clone();
        let anchor_segments = anchor.segments().collect::<Vec<_>>();
        create_override_tables(&mut created, self.coalesced_defaults, &anchor_segments);
        for guard in guards {
            if holds_on(guard, floor_yaml) {
                held.push(guard.clone());
            }
            for path in guard.value_paths() {
                let segments = path.segments().collect::<Vec<_>>();
                if let Some((_, parents)) = segments.split_last() {
                    create_override_tables(&mut created, self.coalesced_defaults, parents);
                }
            }
        }
        let mut member_paths = BTreeSet::new();
        collect_property_paths(&then_schema.clone().into_value(), anchor, &mut member_paths);
        for path in member_paths {
            let segments = path.segments().collect::<Vec<_>>();
            if let Some((_, parents)) = segments.split_last() {
                create_override_tables(&mut created, self.coalesced_defaults, parents);
            }
        }
        let projection = SchemaNode::all_of(build_condition_clauses(
            &held,
            ancestor_segments,
            encoding.values_yaml_doc,
            encoding.absence,
            encoding.polarity,
        ));
        let mut coalesced_instances = Vec::new();
        collect_anchored_instances(
            self.coalesced_defaults,
            ancestor_segments,
            &mut coalesced_instances,
        );
        let mut failing = Vec::new();
        for document in [floor.clone(), created] {
            let mut instances = Vec::new();
            collect_anchored_instances(&document, ancestor_segments, &mut instances);
            if self.fails_only_lint(&instances, &coalesced_instances, &projection, then_schema) {
                failing.push(document);
            }
        }
        if failing.is_empty() {
            None
        } else {
            Some((held, failing))
        }
    }

    /// The member paths the constraint's member contracts require that the
    /// lint floor lacks while the coalesced defaults supply them, each
    /// ranged member named by its key.
    ///
    /// A member contract carries its member conditions inside `then` (JSON
    /// Schema cannot navigate from a member back to the document), where
    /// the lint projection cannot switch them on. An override can set any
    /// member key, including a member only a dependency declares, which
    /// Helm then coalesces with the dependency's member. So every presence
    /// requirement of the switched-on contract on a key the dependency
    /// supplies fails a lint document `helm template` renders.
    fn member_requirements_the_floor_lacks(
        &self,
        anchor: &ValuesPath,
        then_value: &Value,
    ) -> BTreeSet<ValuesPath> {
        let mut lacking = BTreeSet::new();
        let Some((floor, _)) = &self.lint_floor else {
            return lacking;
        };
        let mut required = BTreeSet::new();
        collect_member_requirements(then_value, anchor, false, &mut required);
        let anchor_depth = anchor.segments().count();
        for path in required {
            let segments = path.segments().collect::<Vec<_>>();
            collect_lacking_instances(
                Some(floor),
                self.coalesced_defaults,
                &segments,
                anchor_depth,
                &ValuesPath::default(),
                &mut lacking,
            );
        }
        lacking
    }

    /// Whether every coalesced defaults instance at the constraint's anchor
    /// satisfies `if condition then then_schema`.
    fn coalesced_satisfies(
        &self,
        ancestor_segments: &[String],
        condition: &SchemaNode,
        then_schema: &SchemaNode,
    ) -> bool {
        let Some(validator) = self.constraint_validator(condition, then_schema) else {
            return false;
        };
        let mut coalesced_instances = Vec::new();
        collect_anchored_instances(
            self.coalesced_defaults,
            ancestor_segments,
            &mut coalesced_instances,
        );
        coalesced_instances
            .iter()
            .all(|instance| validator.is_valid(instance))
    }

    /// Whether `if condition then then_schema` rejects one of
    /// `lint_instances` while every one of `coalesced_instances` satisfies
    /// it; both are values at the constraint's anchor.
    fn fails_only_lint(
        &self,
        lint_instances: &[&Value],
        coalesced_instances: &[&Value],
        condition: &SchemaNode,
        then_schema: &SchemaNode,
    ) -> bool {
        if lint_instances.is_empty() {
            return false;
        }
        let Some(validator) = self.constraint_validator(condition, then_schema) else {
            return false;
        };
        if lint_instances
            .iter()
            .all(|instance| validator.is_valid(instance))
        {
            return false;
        }
        coalesced_instances
            .iter()
            .all(|instance| validator.is_valid(instance))
    }

    /// The validator of `if condition then then_schema`, or `None` when it
    /// does not compile: an uncompilable constraint cannot be judged, so it
    /// stays.
    fn constraint_validator(
        &self,
        condition: &SchemaNode,
        then_schema: &SchemaNode,
    ) -> Option<jsonschema::Validator> {
        let constraint = serde_json::json!({
            "if": condition.clone().into_value(),
            "then": then_schema.clone().into_value(),
        });
        let mut definitions =
            crate::provider_definitions::definitions_reachable_from(&constraint, self.definitions);
        if value_references_helm_truthy(&constraint) {
            definitions.insert(
                HELM_TRUTHY_DEFINITION_NAME.to_string(),
                helm_truthy_definition_schema(),
            );
        }
        let document = serde_json::json!({
            "$defs": definitions,
            "allOf": [constraint],
        });
        jsonschema::validator_for(&document).ok()
    }

    /// The relaxed constraints, in relaxation order.
    pub(crate) fn into_relaxed(self) -> Vec<LintWithdrawal> {
        self.relaxed
    }
}

/// Whether `guard` already holds on the lint floor, so no override is
/// needed to switch it on. A guard over ranged members, or one the typed
/// evaluator cannot decide, counts as switchable.
fn holds_on(guard: &ConditionalGuard, floor: &YamlValue) -> bool {
    for path in guard.value_paths() {
        if path.segments().any(Segment::is_each_member) {
            return false;
        }
    }
    evaluate_guard_set_on_values(std::slice::from_ref(guard), floor) == Some(true)
}

/// The members a `hasKey` test inside `guard` looks up.
fn collect_tested_keys(guard: &ConditionalGuard, paths: &mut BTreeSet<ValuesPath>) {
    match guard {
        ConditionalGuard::HasKey { path, key } => {
            let mut member = path.clone();
            member.push(key.as_str());
            paths.insert(member);
        }
        ConditionalGuard::Not(inner) => collect_tested_keys(inner, paths),
        ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
            for guard in guards {
                collect_tested_keys(guard, paths);
            }
        }
        _ => {}
    }
}

/// Creates, along `segments`, each table the coalesced defaults hold where
/// `document` has nothing: the table an override setting a member below it
/// creates. A ranged segment creates every member table the coalesced
/// defaults hold, as an override naming that member does. A null or scalar
/// the document holds stops the walk.
fn create_override_tables(document: &mut Value, coalesced: &Value, segments: &[&Segment]) {
    let Some((head, tail)) = segments.split_first() else {
        return;
    };
    let (Value::Object(members), Value::Object(coalesced_members)) = (document, coalesced) else {
        return;
    };
    if let Some(key) = head.literal() {
        if let Some(coalesced_member @ Value::Object(_)) = coalesced_members.get(key) {
            let member = members
                .entry(key.to_string())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            create_override_tables(member, coalesced_member, tail);
        }
        return;
    }
    for (key, coalesced_member) in coalesced_members {
        if coalesced_member.is_object() {
            let member = members
                .entry(key.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            create_override_tables(member, coalesced_member, tail);
        }
    }
}

/// The member paths `schema` requires below a ranged member (`in_member`),
/// through `properties`, `additionalProperties` (the ranged member),
/// `allOf`, `anyOf`, `oneOf`, `then` and `else`: every branch, since an
/// override can switch on each member condition.
fn collect_member_requirements(
    schema: &Value,
    at: &ValuesPath,
    in_member: bool,
    paths: &mut BTreeSet<ValuesPath>,
) {
    let Value::Object(keywords) = schema else {
        return;
    };
    if in_member && let Some(Value::Array(names)) = keywords.get("required") {
        for name in names {
            if let Some(name) = name.as_str() {
                let mut path = at.clone();
                path.push(name);
                paths.insert(path);
            }
        }
    }
    if let Some(Value::Object(properties)) = keywords.get("properties") {
        for (name, property) in properties {
            let mut path = at.clone();
            path.push(name.as_str());
            collect_member_requirements(property, &path, in_member, paths);
        }
    }
    if let Some(member) = keywords.get("additionalProperties") {
        let mut path = at.clone();
        path.push_each_member();
        collect_member_requirements(member, &path, true, paths);
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(Value::Array(branches)) = keywords.get(keyword) {
            for branch in branches {
                collect_member_requirements(branch, at, in_member, paths);
            }
        }
    }
    for keyword in ["then", "else"] {
        if let Some(branch) = keywords.get(keyword) {
            collect_member_requirements(branch, at, in_member, paths);
        }
    }
}

/// Adds each instance of the path `segments` spell, below `at`, that
/// `judged` lacks (missing or null) while `coalesced` supplies a value. A
/// ranged segment below the anchor (the first `anchor_depth` segments)
/// names each member the coalesced defaults hold; one within the anchor
/// stays ranged, since the constraint quantifies over those members itself.
fn collect_lacking_instances(
    judged: Option<&Value>,
    coalesced: &Value,
    segments: &[&Segment],
    anchor_depth: usize,
    at: &ValuesPath,
    paths: &mut BTreeSet<ValuesPath>,
) {
    let Some((head, tail)) = segments.split_first() else {
        if !coalesced.is_null() && judged.is_none_or(Value::is_null) {
            paths.insert(at.clone());
        }
        return;
    };
    let Value::Object(coalesced_members) = coalesced else {
        return;
    };
    let judged_members = judged.and_then(Value::as_object);
    let judged_member = |key: &str| judged_members.and_then(|members| members.get(key));
    if let Some(key) = head.literal() {
        if let Some(coalesced_member) = coalesced_members.get(key) {
            let mut member_at = at.clone();
            member_at.push(key);
            collect_lacking_instances(
                judged_member(key),
                coalesced_member,
                tail,
                anchor_depth,
                &member_at,
                paths,
            );
        }
        return;
    }
    let within_anchor = at.segments().count() < anchor_depth;
    for (key, coalesced_member) in coalesced_members {
        let mut member_at = at.clone();
        if within_anchor {
            member_at.push_each_member();
        } else {
            member_at.push(key.as_str());
        }
        collect_lacking_instances(
            judged_member(key),
            coalesced_member,
            tail,
            anchor_depth,
            &member_at,
            paths,
        );
    }
}

/// The members `schema` requires or constrains under `at`, through
/// `required`, `properties` and `allOf`.
fn collect_property_paths(schema: &Value, at: &ValuesPath, paths: &mut BTreeSet<ValuesPath>) {
    let Value::Object(keywords) = schema else {
        return;
    };
    if let Some(Value::Array(names)) = keywords.get("required") {
        for name in names {
            if let Some(name) = name.as_str() {
                let mut path = at.clone();
                path.push(name);
                paths.insert(path);
            }
        }
    }
    if let Some(Value::Object(properties)) = keywords.get("properties") {
        for (name, property) in properties {
            let mut path = at.clone();
            path.push(name.as_str());
            paths.insert(path.clone());
            collect_property_paths(property, &path, paths);
        }
    }
    if let Some(Value::Array(branches)) = keywords.get("allOf") {
        for branch in branches {
            collect_property_paths(branch, at, paths);
        }
    }
}

/// The coalesced defaults restricted to the members `root` declares, with
/// the root's nulls kept: the lint document whose overrides leave every
/// dependency default out.
fn root_declared_part(coalesced: &Value, root: &Value) -> Value {
    let (Value::Object(coalesced_members), Value::Object(root_members)) = (coalesced, root) else {
        return coalesced.clone();
    };
    let mut declared = serde_json::Map::new();
    for (key, root_value) in root_members {
        match (root_value, coalesced_members.get(key)) {
            (Value::Null, _) => {
                declared.insert(key.clone(), Value::Null);
            }
            (_, Some(value)) => {
                declared.insert(key.clone(), root_declared_part(value, root_value));
            }
            (_, None) => {}
        }
    }
    Value::Object(declared)
}

/// Whether a document holds a value at a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Presence {
    Missing,
    Null,
    Value,
}

/// What `value` holds at `segments`; a ranged segment asks whether any
/// member holds a value there.
fn presence_at(value: &Value, segments: &[&Segment]) -> Presence {
    let Some((head, tail)) = segments.split_first() else {
        return match value {
            Value::Null => Presence::Null,
            _ => Presence::Value,
        };
    };
    let Value::Object(members) = value else {
        return Presence::Missing;
    };
    if let Some(key) = head.literal() {
        return match members.get(key) {
            Some(member) => presence_at(member, tail),
            None => Presence::Missing,
        };
    }
    let any_value = members
        .values()
        .any(|member| presence_at(member, tail) == Presence::Value);
    if any_value {
        Presence::Value
    } else {
        Presence::Missing
    }
}

/// The values a constraint anchored at `ancestor_segments` applies to: the
/// constraint lands under `properties` (and under `additionalProperties`
/// for a `*` segment), so a missing key leaves nothing to validate.
fn collect_anchored_instances<'v>(
    value: &'v Value,
    ancestor_segments: &[String],
    instances: &mut Vec<&'v Value>,
) {
    let Some((head, tail)) = ancestor_segments.split_first() else {
        instances.push(value);
        return;
    };
    let Value::Object(members) = value else {
        return;
    };
    match Segment::from_encoded_component(head).literal() {
        Some(key) => {
            if let Some(member) = members.get(key) {
                collect_anchored_instances(member, tail, instances);
            }
        }
        None => {
            for member in members.values() {
                collect_anchored_instances(member, tail, instances);
            }
        }
    }
}
