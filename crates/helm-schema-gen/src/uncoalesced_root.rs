//! The documents `helm lint` validates, as further must-accept documents.

use std::borrow::Cow;
use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use helm_schema_core::{
    ConditionalGuard, GuardConjunction, GuardPrefix, RequirementCondition,
    RequirementConditionBuilder, RequirementConditionNode, RequirementDecision, Segment,
    ValuesPath,
};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::condition_encoding::{
    AbsenceDefaults, ConditionFragmentCache, ConditionPolarity, EncodedConditionPlan,
    EncodedConjunction, evaluate_guard_set_on_values, guard_ignores_default_presence,
    input_path_present_condition,
};
use crate::emission_report::{LintDocument, LintOutcome, LintWithdrawal};
use crate::lint_validators::{ConstraintValidator, LintValidators};
use crate::schema_node::SchemaNode;
use crate::terminal_conditions::{ProofGuardRole, RequirementPartition, partition_requirement};

/// No unknown members: every constraint is judged on the lint documents.
static KNOWN: BTreeSet<Vec<String>> = BTreeSet::new();

/// How a constraint encodes its guards, so the lint projection is encoded
/// alike.
#[derive(Clone, Copy)]
pub(crate) struct GuardEncoding<'a> {
    pub(crate) values_yaml_doc: &'a YamlValue,
    pub(crate) absence: AbsenceDefaults<'a>,
    pub(crate) polarity: ConditionPolarity,
}

struct FloorPrefixFacts {
    prefix: GuardPrefix,
    held_prefix: GuardPrefix,
    table_paths: BTreeSet<ValuesPath>,
    anchored_bases: RefCell<BTreeMap<ValuesPath, Rc<Value>>>,
    dependency_boundaries: RefCell<BTreeMap<DependencyBoundaryKey, BTreeSet<ValuesPath>>>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct DependencyBoundaryKey {
    anchor: ValuesPath,
    roots: BTreeSet<Vec<String>>,
    composed: usize,
    runtime: usize,
}

#[derive(Default)]
struct FloorOverrideCache {
    bases: BTreeMap<BTreeSet<ValuesPath>, Rc<Value>>,
    created_bases: usize,
    hits: usize,
    shared_path_hits: usize,
    extra_full_clones: usize,
}

/// A condition whose successful or failed compilation is shared by one fact's checks.
enum LintCondition {
    Flat(Value),
    Planned(EncodedConjunction),
}

impl LintCondition {
    fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Flat(left), Self::Flat(right)) => left == right,
            (Self::Planned(left), Self::Planned(right)) => left.legacy_eq(right),
            (Self::Flat(value), Self::Planned(plan)) | (Self::Planned(plan), Self::Flat(value)) => {
                *value == plan.flat_schema().into_value()
            }
        }
    }
}

struct LintConstraint {
    condition: LintCondition,
    validator: OnceCell<Option<ConstraintValidator>>,
}

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LintGuardPaths {
    override_tables: BTreeSet<ValuesPath>,
}

impl LintGuardPaths {
    pub(crate) fn join(&self, other: &Self) -> Self {
        if self == other {
            return self.clone();
        }
        Self {
            override_tables: union(&self.override_tables, &other.override_tables),
        }
    }
}

fn union(left: &BTreeSet<ValuesPath>, right: &BTreeSet<ValuesPath>) -> BTreeSet<ValuesPath> {
    left.union(right).cloned().collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LintProofDocument {
    Raw,
    Coalesced,
    Floor,
    CreatedFloor,
    ProjectedCoalesced,
}

impl LintProofDocument {
    fn projects_guards(self) -> bool {
        matches!(
            self,
            Self::Floor | Self::CreatedFloor | Self::ProjectedCoalesced
        )
    }
}

struct LintProofInstance<'a> {
    document: LintProofDocument,
    value: &'a Value,
}

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct LintProofVerdict {
    disabled: BTreeSet<usize>,
    raw_uncompiled: bool,
    projection_uncompiled: bool,
}

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct LintDecidingContext {
    boundaries: BTreeSet<ValuesPath>,
    paths: BTreeSet<ValuesPath>,
    absence_tests: BTreeSet<ValuesPath>,
}

impl LintDecidingContext {
    fn join(&self, other: &Self) -> Self {
        if self == other {
            return self.clone();
        }
        Self {
            boundaries: union(&self.boundaries, &other.boundaries),
            paths: union(&self.paths, &other.paths),
            absence_tests: union(&self.absence_tests, &other.absence_tests),
        }
    }
}

struct FailingLintProofDocuments<'a> {
    raw: Option<&'a Value>,
    floor: Vec<&'a Value>,
    dependency_boundaries: Option<GuardEncoding<'a>>,
}

struct LintProofConstraint<'a> {
    ancestor: &'a [String],
    anchor: &'a ValuesPath,
    graph: &'a RequirementCondition,
    extra_guards: Vec<&'a ConditionalGuard>,
    then_value: &'a Value,
}

#[derive(Default)]
struct LintProofEmissions {
    kept: Vec<LintRequirement>,
    withdrawals: Vec<LintWithdrawal>,
}

impl LintProofVerdict {
    fn join(&self, other: &Self) -> Self {
        if self == other {
            return self.clone();
        }
        Self {
            disabled: self.disabled.union(&other.disabled).copied().collect(),
            raw_uncompiled: self.raw_uncompiled || other.raw_uncompiled,
            projection_uncompiled: self.projection_uncompiled || other.projection_uncompiled,
        }
    }

    fn fails_on(&self, instances: &[LintProofInstance<'_>], document: LintProofDocument) -> bool {
        let uncompiled = if document.projects_guards() {
            self.projection_uncompiled
        } else {
            self.raw_uncompiled
        };
        !uncompiled
            && instances.iter().enumerate().any(|(index, instance)| {
                instance.document == document && !self.disabled.contains(&index)
            })
    }
}

fn partition_lint_verdicts(
    graph: &RequirementCondition,
    instances: &[LintProofInstance<'_>],
    observe: impl Fn(&ConditionalGuard, ProofGuardRole) -> LintProofVerdict,
) -> Option<Vec<RequirementPartition<LintProofVerdict>>> {
    if !instances.is_empty() {
        return partition_requirement(graph, observe, LintProofVerdict::join);
    }
    // With no rejected instances, every guard has the same neutral verdict.
    let partition =
        (!matches!(graph.node(graph.root())?, RequirementConditionNode::False)).then(|| {
            RequirementPartition {
                context: LintProofVerdict::default(),
                condition: graph.clone(),
            }
        });
    Some(partition.into_iter().collect())
}

pub(crate) struct LintRequirement {
    pub(crate) guards: GuardConjunction,
    pub(crate) then_value: Value,
}

/// Relaxes constraints that `helm lint` documents fail.
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
/// Terminal failures additionally require the dependency roots they read and
/// the dependency-supplied members absent from the raw defaults to be supplied.
/// An arbitrary override can delete a root or activate a dormant dependency;
/// judging only the default documents cannot establish its render-time guard.
pub(crate) struct UncoalescedRootGate<'a> {
    uncoalesced_root: Option<&'a Value>,
    /// Members whose collision winners differ in either lint document. The gate withdraws a
    /// constraint whose guards or target overlap one.
    unknown: &'a BTreeSet<Vec<String>>,
    lint_floor: Option<(Rc<Value>, YamlValue)>,
    coalesced_defaults: &'a Value,
    relaxed: Vec<LintWithdrawal>,
    conditions: RefCell<ConditionFragmentCache>,
    validators: LintValidators<'a>,
    floor_prefixes: RefCell<BTreeMap<usize, Rc<FloorPrefixFacts>>>,
    floor_overrides: RefCell<FloorOverrideCache>,
}

impl<'a> UncoalescedRootGate<'a> {
    pub(crate) fn record_storage(&self) {
        self.conditions.borrow().record_storage();
        self.validators.record_storage();
        tracing::info_span!(
            target: "helm_schema_gen::counters", "lint_gate_storage",
            withdrawals = self.relaxed.len(), floor_prefixes = self.floor_prefixes.borrow().len(),
            floor_bases = self.floor_overrides.borrow().bases.len(),
        )
        .in_scope(|| {});
    }

    pub(crate) fn new(
        uncoalesced_root: Option<&'a Value>,
        coalesced_defaults: &'a Value,
        definitions: &'a BTreeMap<String, Value>,
    ) -> Self {
        let lint_floor = uncoalesced_root.map(|root| {
            let floor = root_declared_part(coalesced_defaults, root);
            let floor_yaml = serde_yaml::to_value(&floor).unwrap_or(YamlValue::Null);
            (Rc::new(floor), floor_yaml)
        });
        let mut conditions = ConditionFragmentCache::new();
        for (name, definition) in definitions {
            conditions.reserve(&serde_json::json!({ "$ref": helm_schema_json_schema_walk::GENERATED_DEFINITIONS.reference(name) }));
            conditions.reserve(definition);
        }
        Self {
            uncoalesced_root,
            unknown: &KNOWN,
            lint_floor,
            coalesced_defaults,
            relaxed: Vec::new(),
            conditions: RefCell::new(conditions),
            validators: LintValidators::new(definitions),
            floor_prefixes: RefCell::new(BTreeMap::new()),
            floor_overrides: RefCell::default(),
        }
    }

    /// Withdraws constraints whose guards or target overlap these unknown members.
    pub(crate) fn with_unknown(mut self, unknown: &'a BTreeSet<Vec<String>>) -> Self {
        self.unknown = unknown;
        self
    }

    pub(crate) fn reserve_condition_names(&self, schema: &Value) {
        self.conditions.borrow_mut().reserve(schema);
    }

    pub(crate) fn condition_clauses(
        &self,
        guards: &[ConditionalGuard],
        ancestor: &[String],
        values: &YamlValue,
        absence: AbsenceDefaults<'_>,
        polarity: ConditionPolarity,
    ) -> Vec<SchemaNode> {
        self.conditions
            .borrow_mut()
            .clauses(guards, ancestor, values, absence, polarity)
    }

    pub(crate) fn planned_conjunction(
        &self,
        guards: &GuardConjunction,
        ancestor: &[String],
        values: &YamlValue,
        absence: AbsenceDefaults<'_>,
        polarity: ConditionPolarity,
    ) -> EncodedConjunction {
        self.conditions
            .borrow_mut()
            .planned_conjunction(guards, ancestor, values, absence, polarity)
    }

    pub(crate) fn supports_scope_guards<'guard>(
        &self,
        guards: impl IntoIterator<Item = &'guard ConditionalGuard>,
    ) -> bool {
        self.conditions.borrow().supports_scope_guards(guards)
    }

    pub(crate) fn requirements_encode_fully<'guard>(
        &self,
        guards: impl IntoIterator<Item = &'guard ConditionalGuard>,
        ancestor: &[String],
        encoding: GuardEncoding<'_>,
    ) -> bool {
        let GuardEncoding {
            values_yaml_doc,
            absence,
            polarity,
        } = encoding;
        let mut conditions = self.conditions.borrow_mut();
        guards.into_iter().all(|guard| {
            conditions.requirements_encode_fully(
                guard,
                ancestor,
                values_yaml_doc,
                absence,
                polarity,
            )
        })
    }

    pub(crate) fn allows_terminal_coalescing(&self) -> bool {
        self.conditions.borrow().allows_shared_conditions()
    }

    pub(crate) fn conjunction_is_type_partition(&self, condition: &EncodedConjunction) -> bool {
        let mut conditions = self.conditions.borrow_mut();
        condition.legacy_type_partition(|fragment| conditions.is_type_partition(fragment))
    }

    pub(crate) fn append_planned_conditional(
        &self,
        document: &mut crate::schema_tree::SchemaDocument,
        ancestor: &[String],
        plan: &EncodedConditionPlan,
        then_schema: SchemaNode,
    ) {
        let mut conditions = self.conditions.borrow_mut();
        let type_partition =
            plan.legacy_type_partition(|fragment| conditions.is_type_partition(fragment));
        let materializing = tracing::info_span!("materialize_planned_condition");
        let materializing_scope = materializing.enter();
        let condition = conditions.materialize_condition(plan);
        drop(materializing_scope);
        drop(materializing);
        let _append = tracing::info_span!("append_materialized_condition").entered();
        document.append_conditional_with_type_partition(
            ancestor,
            condition,
            then_schema,
            type_partition,
        );
    }

    #[tracing::instrument(skip_all, fields(anchor_depth = ancestor.len()))]
    pub(crate) fn append_conditional(
        &self,
        document: &mut crate::schema_tree::SchemaDocument,
        ancestor: &[String],
        condition: SchemaNode,
        then_schema: SchemaNode,
    ) {
        let type_partition = self.conditions.borrow_mut().is_type_partition(&condition);
        document.append_conditional_with_type_partition(
            ancestor,
            condition,
            then_schema,
            type_partition,
        );
    }

    pub(crate) fn compact_condition_junctions(
        &mut self,
        document: &crate::schema_tree::SchemaDocument,
    ) {
        self.conditions
            .get_mut()
            .compact_condition_junctions(document);
    }

    pub(crate) fn take_condition_definitions(&mut self) -> BTreeMap<String, Value> {
        self.conditions.borrow_mut().take_definitions()
    }

    pub(crate) fn begin_terminal_conditions(&mut self) {
        self.conditions.borrow_mut().begin_terminal_conditions();
    }

    /// Releases compiled judges after the conditional lint checks have completed.
    pub(crate) fn finish_conditional_lint(&mut self) {
        self.record_floor_override_cache();
        self.validators.release_compiled_parts();
    }

    /// The `then` part `if condition then then_value`, anchored at
    /// `ancestor_segments`, keeps under the lint contract: unchanged,
    /// conditioned on the paths the lint documents lack, or `None` when the
    /// constraint is withdrawn.
    /// `condition` encodes `guards` with `encoding`.
    #[tracing::instrument(skip_all, fields(guards = guard_row.len(), anchor_depth = ancestor_segments.len()))]
    pub(crate) fn lint_safe_then(
        &mut self,
        ancestor_segments: &[String],
        guard_row: &GuardConjunction,
        condition: &SchemaNode,
        then_value: Value,
        encoding: GuardEncoding<'_>,
    ) -> Option<Value> {
        if self.uncoalesced_root.is_none() {
            return Some(then_value);
        }
        if self.overlaps_unknown(ancestor_segments, guard_row, &then_value) {
            self.withdraw_overlap(ancestor_segments);
            return None;
        }
        self.lint_safe_constraint(
            ancestor_segments,
            guard_row,
            &LintConstraint {
                condition: LintCondition::Flat(condition.clone().into_value()),
                validator: OnceCell::new(),
            },
            then_value,
            encoding,
        )
    }

    pub(crate) fn lint_safe_planned_then(
        &mut self,
        ancestor_segments: &[String],
        guard_row: &GuardConjunction,
        condition: &EncodedConjunction,
        then_value: Value,
        encoding: GuardEncoding<'_>,
    ) -> Option<Value> {
        if self.uncoalesced_root.is_none() {
            return Some(then_value);
        }
        if self.overlaps_unknown(ancestor_segments, guard_row, &then_value) {
            self.withdraw_overlap(ancestor_segments);
            return None;
        }
        self.lint_safe_constraint(
            ancestor_segments,
            guard_row,
            &LintConstraint {
                condition: LintCondition::Planned(condition.clone()),
                validator: OnceCell::new(),
            },
            then_value,
            encoding,
        )
    }

    pub(crate) fn lint_guard_paths(&self, guard: &ConditionalGuard) -> LintGuardPaths {
        let Some((floor, _)) = self.lint_floor.as_ref() else {
            return LintGuardPaths::default();
        };
        if self.guard_overlaps_unknown(guard) {
            return LintGuardPaths::default();
        }
        let mut paths = LintGuardPaths::default();
        for path in guard.value_paths().iter().filter_map(ValuesPath::parent) {
            let segments = path.segments().collect::<Vec<_>>();
            collect_missing_override_tables(
                Some(floor),
                self.coalesced_defaults,
                &segments,
                &ValuesPath::default(),
                &mut paths.override_tables,
            );
        }
        paths
    }

    /// Checks each shared proof region under its existing lint document judgments.
    ///
    /// The caller supplies a region with one set of floor override tables.
    /// Partition by guard truth at the comparison instances before choosing deciding paths.
    /// A coalesced failure in one alternative must not prevent
    /// relaxation of another alternative that fails only under lint.
    pub(crate) fn lint_requirement_partitions(
        &mut self,
        ancestor: &[String],
        guards: &GuardConjunction,
        graph: &RequirementCondition,
        then_value: Value,
        encoding: GuardEncoding<'_>,
    ) -> Vec<LintRequirement> {
        if self.uncoalesced_root.is_some() && self.overlaps_unknown(ancestor, guards, &then_value) {
            self.withdraw_overlap(ancestor);
            return Vec::new();
        }
        let Some((raw, (floor, floor_yaml))) = self.uncoalesced_root.zip(self.lint_floor.as_ref())
        else {
            return vec![LintRequirement {
                guards: guards.clone(),
                then_value,
            }];
        };
        let anchor = ValuesPath::from_segments(
            ancestor
                .iter()
                .map(|segment| Segment::from_encoded_component(segment)),
        );
        let mut created = Cow::Borrowed(floor.as_ref());
        self.create_requirement_floor_tables(&anchor, guards, &then_value, &mut created);
        let instances = self.lint_proof_instances(
            [
                (LintProofDocument::Raw, raw),
                (LintProofDocument::Coalesced, self.coalesced_defaults),
                (LintProofDocument::Floor, floor.as_ref()),
                (LintProofDocument::CreatedFloor, created.as_ref()),
                (
                    LintProofDocument::ProjectedCoalesced,
                    self.coalesced_defaults,
                ),
            ],
            ancestor,
            &then_value,
        );
        let then_compiles = instances.is_some();
        let instances = instances.unwrap_or_default();
        let observe = |guard: &ConditionalGuard, role: ProofGuardRole| {
            self.lint_proof_guard_verdict(guard, role, ancestor, encoding, floor_yaml, &instances)
        };
        let extra_guards = guards.iter().filter(|guard| {
            !matches!(guard, ConditionalGuard::Requirement(other) if std::ptr::eq(graph, other))
        }).collect::<Vec<_>>();
        let extras = extra_guards
            .iter()
            .fold(LintProofVerdict::default(), |combined, guard| {
                combined.join(&observe(guard, ProofGuardRole::Leaf))
            });
        let Some(partitions) = partition_lint_verdicts(graph, &instances, observe) else {
            return Vec::new();
        };
        let dependency_boundaries = (then_value == Value::Bool(false)
            && !encoding.absence.dependency_roots.is_empty())
        .then_some(encoding);
        let mut emissions = LintProofEmissions::default();
        for partition in partitions {
            let verdict = partition.context.join(&extras);
            let coalesced_holds = !verdict.fails_on(&instances, LintProofDocument::Coalesced);
            let raw_fails = coalesced_holds && verdict.fails_on(&instances, LintProofDocument::Raw);
            let projected_coalesced_holds =
                !verdict.fails_on(&instances, LintProofDocument::ProjectedCoalesced);
            let failing_floor = [
                (LintProofDocument::Floor, floor.as_ref()),
                (LintProofDocument::CreatedFloor, created.as_ref()),
            ]
            .into_iter()
            .filter_map(|(document, value)| {
                (projected_coalesced_holds && verdict.fails_on(&instances, document))
                    .then_some(value)
            })
            .collect::<Vec<_>>();
            let lacking = if coalesced_holds && then_compiles && !verdict.raw_uncompiled {
                self.member_requirements_the_floor_lacks(&anchor, &then_value)
            } else {
                BTreeSet::new()
            };
            let failing = FailingLintProofDocuments {
                raw: raw_fails.then_some(raw),
                floor: failing_floor,
                dependency_boundaries,
            };
            self.emit_lint_proof(
                LintProofConstraint {
                    ancestor,
                    anchor: &anchor,
                    graph: &partition.condition,
                    extra_guards: extra_guards.clone(),
                    then_value: &then_value,
                },
                &failing,
                raw,
                floor_yaml,
                lacking,
                &mut emissions,
            );
        }
        self.relaxed.extend(emissions.withdrawals);
        emissions.kept
    }

    fn emit_lint_proof(
        &self,
        proof: LintProofConstraint<'_>,
        failing: &FailingLintProofDocuments<'_>,
        raw: &Value,
        floor_yaml: &YamlValue,
        mut then_paths: BTreeSet<ValuesPath>,
        emissions: &mut LintProofEmissions,
    ) {
        let LintProofConstraint {
            ancestor,
            anchor,
            graph,
            extra_guards,
            then_value,
        } = proof;
        for document in failing.raw.into_iter().chain(failing.floor.iter().copied()) {
            self.collect_deciding_paths(
                document,
                std::iter::empty(),
                then_value,
                anchor,
                &mut then_paths,
            );
        }
        let lint_document = if failing.raw.is_some() {
            Some(LintDocument::Root)
        } else if !failing.floor.is_empty() || !then_paths.is_empty() {
            Some(LintDocument::Floor)
        } else {
            None
        };
        let row_with = |condition| -> GuardConjunction {
            extra_guards
                .iter()
                .map(|&guard| guard.clone())
                .chain(std::iter::once(ConditionalGuard::Requirement(condition)))
                .collect::<Vec<_>>()
                .into()
        };
        if lint_document.is_none() && failing.dependency_boundaries.is_none() {
            emissions.kept.push(LintRequirement {
                guards: row_with(graph.clone()),
                then_value: then_value.clone(),
            });
            return;
        }
        let observe = |guard: &ConditionalGuard, _role| {
            self.lint_deciding_context(anchor, guard, raw, floor_yaml, failing)
        };
        let Some(decisions) = partition_requirement(graph, observe, LintDecidingContext::join)
        else {
            return;
        };
        let extra_context = extra_guards
            .iter()
            .fold(LintDecidingContext::default(), |combined, guard| {
                combined.join(&observe(guard, ProofGuardRole::Leaf))
            });
        for decision in decisions {
            let LintDecidingContext {
                boundaries,
                mut paths,
                absence_tests,
            } = decision.context.join(&extra_context);
            let boundaries = dependency_boundary_leaves(boundaries.iter());
            let lint_document =
                lint_document.or_else(|| (!boundaries.is_empty()).then_some(LintDocument::Floor));
            paths.extend(boundaries);
            paths.extend(then_paths.iter().cloned());
            let guard_row = row_with(decision.condition);
            if let Some(document) = lint_document {
                let deciding_absence = !paths.is_disjoint(&absence_tests);
                let then_value = (!deciding_absence)
                    .then(|| {
                        condition_on_deciding_paths(
                            ancestor,
                            &guard_row,
                            &paths,
                            then_value.clone(),
                        )
                    })
                    .flatten();
                let outcome = if then_value.is_some() {
                    LintOutcome::Conditioned
                } else {
                    LintOutcome::Withdrawn
                };
                emissions.withdrawals.push(LintWithdrawal {
                    anchor: anchor.clone(),
                    document,
                    deciding_paths: paths.into_iter().collect(),
                    outcome,
                });
                if let Some(then_value) = then_value {
                    emissions.kept.push(LintRequirement {
                        guards: guard_row,
                        then_value,
                    });
                }
            } else {
                emissions.kept.push(LintRequirement {
                    guards: guard_row,
                    then_value: then_value.clone(),
                });
            }
        }
    }

    fn lint_deciding_context(
        &self,
        anchor: &ValuesPath,
        guard: &ConditionalGuard,
        raw: &Value,
        floor_yaml: &YamlValue,
        failing: &FailingLintProofDocuments<'_>,
    ) -> LintDecidingContext {
        let mut context = LintDecidingContext::default();
        if let Some(encoding) = failing.dependency_boundaries {
            context.boundaries =
                self.collect_dependency_boundaries(raw, anchor, std::iter::once(guard), encoding);
        }
        let keep_floor_guard = !failing.floor.is_empty() && holds_on(guard, floor_yaml);
        for document in failing
            .raw
            .into_iter()
            .chain(failing.floor.iter().copied().filter(|_| keep_floor_guard))
        {
            self.collect_deciding_paths(
                document,
                std::iter::once(guard),
                &Value::Bool(false),
                &ValuesPath::default(),
                &mut context.paths,
            );
        }
        if let ConditionalGuard::Absent { path } = guard {
            context.absence_tests.insert(path.clone());
        }
        context
    }

    fn create_requirement_floor_tables(
        &self,
        anchor: &ValuesPath,
        guards: &GuardConjunction,
        then_value: &Value,
        created: &mut Cow<'_, Value>,
    ) {
        let mut tables = BTreeSet::from([anchor.clone()]);
        for guard in guards {
            tables.extend(guard.value_paths().iter().filter_map(ValuesPath::parent));
        }
        let mut paths = BTreeSet::new();
        collect_property_paths(then_value, anchor, &mut paths);
        tables.extend(paths.iter().filter_map(ValuesPath::parent));
        for path in tables {
            let segments = path.segments().collect::<Vec<_>>();
            create_override_tables(created, self.coalesced_defaults, &segments);
        }
    }

    fn lint_proof_instances<'v>(
        &self,
        documents: impl IntoIterator<Item = (LintProofDocument, &'v Value)>,
        ancestor: &[String],
        then_value: &Value,
    ) -> Option<Vec<LintProofInstance<'v>>> {
        let definitions = self.conditions.borrow();
        let validator = self.validators.constraint_validator(
            &Value::Bool(true),
            then_value,
            &definitions.definitions,
        )?;
        let mut picked = Vec::new();
        for (document, value) in documents {
            let mut instances = Vec::new();
            collect_anchored_instances(value, ancestor, &mut instances);
            picked.extend(
                instances
                    .into_iter()
                    .filter(|instance| !validator.is_valid(instance))
                    .map(|value| LintProofInstance { document, value }),
            );
        }
        Some(picked)
    }

    fn lint_proof_guard_verdict(
        &self,
        guard: &ConditionalGuard,
        role: ProofGuardRole,
        ancestor: &[String],
        encoding: GuardEncoding<'_>,
        floor: &YamlValue,
        instances: &[LintProofInstance<'_>],
    ) -> LintProofVerdict {
        if instances.is_empty() {
            return LintProofVerdict::default();
        }
        let condition = SchemaNode::all_of(self.condition_clauses(
            std::slice::from_ref(guard),
            ancestor,
            encoding.values_yaml_doc,
            encoding.absence,
            match role {
                ProofGuardRole::Leaf => encoding.polarity,
                ProofGuardRole::SplitTest => ConditionPolarity::Exact,
            },
        ))
        .into_value();
        let projected_away = !holds_on(guard, floor);
        let definitions = self.conditions.borrow();
        let Some(validator) = self.validators.constraint_validator(
            &condition,
            &Value::Bool(false),
            &definitions.definitions,
        ) else {
            return LintProofVerdict {
                raw_uncompiled: true,
                projection_uncompiled: !projected_away,
                ..LintProofVerdict::default()
            };
        };
        LintProofVerdict {
            disabled: instances
                .iter()
                .enumerate()
                .filter_map(|(index, instance)| {
                    (!(projected_away && instance.document.projects_guards())
                        && validator.is_valid(instance.value))
                    .then_some(index)
                })
                .collect(),
            raw_uncompiled: false,
            projection_uncompiled: false,
        }
    }

    /// Whether a constraint's guards or target lie on an unknown member's path. Lint
    /// acceptance there is undecidable, so the gate withdraws the constraint. A `false` target
    /// rejects every instance and reads no member.
    fn overlaps_unknown(
        &self,
        ancestor_segments: &[String],
        guards: &GuardConjunction,
        then_value: &Value,
    ) -> bool {
        let target = ancestor_segments.iter().map(|segment| {
            Segment::from_encoded_component(segment)
                .literal()
                .map(str::to_owned)
        });
        guards
            .iter()
            .any(|guard| self.guard_overlaps_unknown(guard))
            || (*then_value != Value::Bool(false)
                && self
                    .unknown
                    .iter()
                    .any(|unknown| on_path(target.clone(), unknown)))
    }

    fn guard_overlaps_unknown(&self, guard: &ConditionalGuard) -> bool {
        !self.unknown.is_empty()
            && guard.value_paths().iter().any(|path| {
                self.unknown.iter().any(|unknown| {
                    on_path(
                        path.segments()
                            .map(|segment| segment.literal().map(str::to_owned)),
                        unknown,
                    )
                })
            })
    }

    fn withdraw_overlap(&mut self, ancestor_segments: &[String]) {
        self.relaxed.push(LintWithdrawal {
            anchor: ValuesPath::from_segments(
                ancestor_segments
                    .iter()
                    .map(|segment| Segment::from_encoded_component(segment)),
            ),
            document: LintDocument::Root,
            deciding_paths: Vec::new(),
            outcome: LintOutcome::Withdrawn,
        });
    }

    fn lint_safe_constraint(
        &mut self,
        ancestor_segments: &[String],
        guard_row: &GuardConjunction,
        constraint: &LintConstraint,
        then_value: Value,
        encoding: GuardEncoding<'_>,
    ) -> Option<Value> {
        let Some(uncoalesced_root) = self.uncoalesced_root else {
            return Some(then_value);
        };
        let anchor = ValuesPath::from_segments(
            ancestor_segments
                .iter()
                .map(|segment| Segment::from_encoded_component(segment)),
        );
        let mut instances = Vec::new();
        collect_anchored_instances(uncoalesced_root, ancestor_segments, &mut instances);
        let mut coalesced_instances = Vec::new();
        collect_anchored_instances(
            self.coalesced_defaults,
            ancestor_segments,
            &mut coalesced_instances,
        );
        let mut deciding_paths = if then_value == Value::Bool(false) {
            self.dependency_boundary_paths(&anchor, guard_row, encoding)
        } else {
            BTreeSet::new()
        };
        let mut document = (!deciding_paths.is_empty()).then_some(LintDocument::Floor);
        if self.fails_only_lint(&instances, &coalesced_instances, constraint, &then_value) {
            document = Some(LintDocument::Root);
            self.collect_deciding_paths(
                uncoalesced_root,
                guard_row,
                &then_value,
                &anchor,
                &mut deciding_paths,
            );
        }
        if let Some((held, failing)) = self.floor_failure(
            &anchor,
            ancestor_segments,
            guard_row,
            &then_value,
            encoding,
            constraint,
        ) {
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
            && self
                .prepared_validator(constraint, &then_value)
                .is_some_and(|validator| {
                    coalesced_instances
                        .iter()
                        .all(|instance| validator.is_valid(instance))
                })
        {
            document.get_or_insert(LintDocument::Floor);
            deciding_paths.extend(lacking);
        }
        let Some(document) = document else {
            return Some(then_value);
        };
        let kept =
            condition_on_deciding_paths(ancestor_segments, guard_row, &deciding_paths, then_value);
        let outcome = if kept.is_some() {
            LintOutcome::Conditioned
        } else {
            LintOutcome::Withdrawn
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

    /// Flat proofs already share their disjunction in the schema. An arm
    /// can relax only the presence tests imposed by its siblings.
    pub(crate) fn terminal_omission_alternatives(
        &self,
        ancestor: &[String],
        guards: &GuardConjunction,
        encoding: GuardEncoding<'_>,
    ) -> Vec<GuardConjunction> {
        if self.uncoalesced_root.is_none() || encoding.absence.dependency_roots.is_empty() {
            return Vec::new();
        }
        let Some((index, ConditionalGuard::AnyOf(arms))) = guards
            .iter()
            .enumerate()
            .find(|(_, guard)| matches!(guard, ConditionalGuard::AnyOf(_)))
        else {
            return Vec::new();
        };
        let anchor = ValuesPath::from_segments(
            ancestor
                .iter()
                .map(|segment| Segment::from_encoded_component(segment)),
        );
        let joint = self.dependency_boundary_paths(&anchor, guards, encoding);
        if joint.is_empty() {
            return Vec::new();
        }
        let mut independent = Vec::new();
        for arm in arms.iter() {
            let mut row = guards
                .iter()
                .enumerate()
                .filter(|(offset, _)| *offset != index)
                .map(|(_, guard)| guard.clone())
                .collect::<Vec<_>>();
            if let ConditionalGuard::AllOf(members) = arm {
                row.extend(members.iter().cloned());
            } else {
                row.push(arm.clone());
            }
            let row = GuardConjunction::from(row);
            let boundaries = self.dependency_boundary_paths(&anchor, &row, encoding);
            if joint.iter().any(|path| {
                !boundaries
                    .iter()
                    .any(|kept| kept == path || kept.is_descendant_of(path))
            }) {
                independent.push(row);
            }
        }
        independent
    }

    /// The inputs whose absence can select a dependency refill or a default
    /// omitted from raw lint values, independently of the defaults' liveness.
    ///
    /// A dependency root at or above `anchor` is none of them. The anchored
    /// clause is judged only on a supplied anchor table. Where the root values
    /// declare that table, lint coalescing deletes an override's null for it
    /// (Helm v4.2.3 `pkg/chart/common/util/coalesce.go:313-358`); where they do
    /// not, the members only the dependency supplies stay deciding.
    fn dependency_boundary_paths(
        &self,
        anchor: &ValuesPath,
        guards: &GuardConjunction,
        encoding: GuardEncoding<'_>,
    ) -> BTreeSet<ValuesPath> {
        let dependency_roots = encoding.absence.dependency_roots;
        let Some(raw) = self
            .uncoalesced_root
            .filter(|_| !dependency_roots.is_empty())
        else {
            return BTreeSet::new();
        };
        let Some(((prefix, extras), (_, floor))) =
            guards.canonical_parts().zip(self.lint_floor.as_ref())
        else {
            let boundaries = self.collect_dependency_boundaries(raw, anchor, guards, encoding);
            return dependency_boundary_leaves(boundaries.iter());
        };
        let extra_boundaries = self.collect_dependency_boundaries(raw, anchor, extras, encoding);
        let facts = self.floor_prefix_facts(prefix, floor);
        let mut cached = facts.dependency_boundaries.borrow_mut();
        let boundaries = cached
            .entry(DependencyBoundaryKey {
                anchor: anchor.clone(),
                roots: dependency_roots.clone(),
                composed: std::ptr::from_ref(encoding.values_yaml_doc).addr(),
                runtime: std::ptr::from_ref(encoding.absence.runtime_defaults).addr(),
            })
            .or_insert_with(|| {
                self.collect_dependency_boundaries(raw, anchor, prefix.canonical().iter(), encoding)
            });
        dependency_boundary_leaves(boundaries.union(&extra_boundaries))
    }

    fn collect_dependency_boundaries<'guard>(
        &self,
        raw: &Value,
        anchor: &ValuesPath,
        guards: impl IntoIterator<Item = &'guard ConditionalGuard>,
        encoding: GuardEncoding<'_>,
    ) -> BTreeSet<ValuesPath> {
        let mut boundaries = BTreeSet::new();
        for guard in guards {
            let mut paths = guard.value_paths();
            collect_tested_keys(guard, &mut paths);
            for path in &paths {
                let mut under_dependency = false;
                for root in encoding.absence.dependency_roots {
                    let root = ValuesPath::from_segments(root.iter().cloned().map(Segment::from));
                    if path != &root && !path.is_descendant_of(&root) {
                        continue;
                    }
                    under_dependency = true;
                    // A missing dependency root can be recreated before template execution.
                    if anchor != &root && !anchor.is_descendant_of(&root) {
                        boundaries.insert(root);
                    }
                }
                if under_dependency {
                    let segments = path.segments().collect::<Vec<_>>();
                    if presence_at(raw, &segments) != Presence::Value
                        && presence_at(self.coalesced_defaults, &segments) == Presence::Value
                        && !guard_ignores_default_presence(
                            guard,
                            path,
                            encoding.values_yaml_doc,
                            encoding.absence.runtime_defaults,
                        )
                    {
                        boundaries.insert(path.clone());
                    }
                }
            }
        }
        boundaries
    }

    /// Adds the constraint's paths whose presence differs between `judged`
    /// and the coalesced defaults: absent or null in `judged` where the
    /// defaults supply a value, or a null in `judged` that coalescing
    /// deletes.
    fn collect_deciding_paths<'guard>(
        &self,
        judged: &Value,
        guards: impl IntoIterator<Item = &'guard ConditionalGuard>,
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

    fn floor_prefix_facts(&self, prefix: &GuardPrefix, floor: &YamlValue) -> Rc<FloorPrefixFacts> {
        let address = prefix.as_slice().as_ptr().addr();
        let mut prefixes = self.floor_prefixes.borrow_mut();
        let facts = prefixes.entry(address).or_insert_with(|| {
            let canonical = prefix.canonical();
            let mut held = Vec::new();
            let mut table_paths = BTreeSet::new();
            for guard in canonical.iter() {
                if holds_on(guard, floor) {
                    held.push(guard.clone());
                }
                table_paths.extend(guard.value_paths().iter().filter_map(ValuesPath::parent));
            }
            let held_prefix = if held.len() == canonical.len() {
                prefix.clone()
            } else {
                GuardPrefix::new(held)
            };
            Rc::new(FloorPrefixFacts {
                prefix: prefix.clone(),
                held_prefix,
                table_paths,
                anchored_bases: RefCell::default(),
                dependency_boundaries: RefCell::default(),
            })
        });
        debug_assert!(facts.prefix.shares_storage(prefix));
        Rc::clone(facts)
    }

    fn floor_with_prefix(
        &self,
        floor: &Rc<Value>,
        anchor: &ValuesPath,
        facts: &FloorPrefixFacts,
    ) -> Rc<Value> {
        let mut anchored = facts.anchored_bases.borrow_mut();
        let mut overrides = self.floor_overrides.borrow_mut();
        if let Some(base) = anchored.get(anchor) {
            overrides.hits += 1;
            return Rc::clone(base);
        }

        let mut paths = facts.table_paths.clone();
        paths.insert(anchor.clone());
        let base = if let Some(base) = overrides.bases.get(&paths).cloned() {
            overrides.hits += 1;
            overrides.shared_path_hits += 1;
            base
        } else {
            // Every request creates its own parents and leaves existing values alone.
            // Applying the same set of requests therefore makes the same document.
            let mut created = Cow::Borrowed(floor.as_ref());
            for path in &paths {
                let segments = path.segments().collect::<Vec<_>>();
                create_override_tables(&mut created, self.coalesced_defaults, &segments);
            }
            let base = match created {
                Cow::Borrowed(_) => Rc::clone(floor),
                Cow::Owned(value) => {
                    overrides.created_bases += 1;
                    Rc::new(value)
                }
            };
            overrides.bases.insert(paths, Rc::clone(&base));
            base
        };
        anchored.insert(anchor.clone(), Rc::clone(&base));
        base
    }

    fn record_floor_override_cache(&self) {
        let overrides = self.floor_overrides.borrow();
        tracing::info_span!(
            target: "helm_schema_gen::counters",
            "lint_floor_override_cache",
            lint_enabled = self.uncoalesced_root.is_some(),
            requested_sets = overrides.bases.len(),
            created_bases = overrides.created_bases,
            cache_hits = overrides.hits,
            shared_path_hits = overrides.shared_path_hits,
            extra_full_clones = overrides.extra_full_clones,
        )
        .in_scope(|| {});
    }

    /// The guards already holding on the lint floor and the floor documents
    /// failing the constraint's lint projection (those guards, every other
    /// guard switched on): the floor itself and the floor with the tables
    /// an override creates along the constraint's paths.
    fn floor_failure(
        &self,
        anchor: &ValuesPath,
        ancestor_segments: &[String],
        guard_row: &GuardConjunction,
        then_value: &Value,
        encoding: GuardEncoding<'_>,
        constraint: &LintConstraint,
    ) -> Option<(Vec<ConditionalGuard>, Vec<Rc<Value>>)> {
        let (floor, floor_yaml) = self.lint_floor.as_ref()?;
        let parts = guard_row.canonical_parts();
        let prepared = parts.map(|(prefix, _)| self.floor_prefix_facts(prefix, floor_yaml));
        let base = prepared.as_deref().map_or_else(
            || Rc::clone(floor),
            |facts| self.floor_with_prefix(floor, anchor, facts),
        );
        let mut held_extras = Vec::new();
        let mut created = Cow::Borrowed(base.as_ref());
        if prepared.is_none() {
            let anchor_segments = anchor.segments().collect::<Vec<_>>();
            create_override_tables(&mut created, self.coalesced_defaults, &anchor_segments);
        }
        let (extras, flat_guards) = match parts {
            Some((_, extras)) => (Some(extras.iter()), None),
            None => (None, Some(guard_row.iter())),
        };
        for guard in extras
            .into_iter()
            .flatten()
            .chain(flat_guards.into_iter().flatten())
        {
            if let ConditionalGuard::Requirement(graph) = guard {
                if let Some(projected) = requirement_on_lint_floor(graph, floor_yaml) {
                    held_extras.push(projected);
                }
            } else if holds_on(guard, floor_yaml) {
                held_extras.push(guard.clone());
            }
            for path in guard.value_paths() {
                let segments = path.segments().collect::<Vec<_>>();
                if let Some((_, parents)) = segments.split_last() {
                    create_override_tables(&mut created, self.coalesced_defaults, parents);
                }
            }
        }
        let mut member_paths = BTreeSet::new();
        collect_property_paths(then_value, anchor, &mut member_paths);
        for path in member_paths {
            let segments = path.segments().collect::<Vec<_>>();
            if let Some((_, parents)) = segments.split_last() {
                create_override_tables(&mut created, self.coalesced_defaults, parents);
            }
        }
        let held_row = if let Some(prepared) = prepared.as_ref() {
            GuardConjunction::prefixed(held_extras, prepared.held_prefix.clone(), Vec::new())
                .canonicalized()
        } else {
            held_extras.into()
        };
        let projected = LintConstraint {
            condition: LintCondition::Planned(self.planned_conjunction(
                &held_row,
                ancestor_segments,
                encoding.values_yaml_doc,
                encoding.absence,
                encoding.polarity,
            )),
            validator: OnceCell::new(),
        };
        let constraint = if projected.condition.matches(&constraint.condition) {
            constraint
        } else {
            &projected
        };
        let mut coalesced_instances = Vec::new();
        collect_anchored_instances(
            self.coalesced_defaults,
            ancestor_segments,
            &mut coalesced_instances,
        );
        let mut failing = Vec::new();
        let created = match created {
            Cow::Borrowed(_) => Rc::clone(&base),
            Cow::Owned(value) => {
                if prepared.is_some() {
                    self.floor_overrides.borrow_mut().extra_full_clones += 1;
                }
                Rc::new(value)
            }
        };
        for document in [Rc::clone(floor), created] {
            let mut instances = Vec::new();
            collect_anchored_instances(&document, ancestor_segments, &mut instances);
            if self.fails_only_lint(&instances, &coalesced_instances, constraint, then_value) {
                failing.push(document);
            }
        }
        if failing.is_empty() {
            None
        } else {
            Some((held_row.to_vec(), failing))
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

    /// Whether the validator rejects one of
    /// `lint_instances` while every one of `coalesced_instances` satisfies
    /// it; both are values at the constraint's anchor.
    fn fails_only_lint(
        &self,
        lint_instances: &[&Value],
        coalesced_instances: &[&Value],
        constraint: &LintConstraint,
        then_value: &Value,
    ) -> bool {
        if lint_instances.is_empty() {
            return false;
        }
        let Some(validator) = self.prepared_validator(constraint, then_value) else {
            return false;
        };
        lint_instances
            .iter()
            .any(|instance| !validator.is_valid(instance))
            && coalesced_instances
                .iter()
                .all(|instance| validator.is_valid(instance))
    }

    fn prepared_validator<'b>(
        &self,
        constraint: &'b LintConstraint,
        then_value: &Value,
    ) -> Option<&'b ConstraintValidator> {
        constraint
            .validator
            .get_or_init(|| {
                let conditions = self.conditions.borrow();
                match &constraint.condition {
                    LintCondition::Flat(condition) => self.validators.constraint_validator(
                        condition,
                        then_value,
                        &conditions.definitions,
                    ),
                    LintCondition::Planned(condition) => {
                        self.validators.planned_constraint_validator(
                            condition,
                            then_value,
                            &conditions.definitions,
                        )
                    }
                }
            })
            .as_ref()
    }

    /// The relaxed constraints, in relaxation order.
    pub(crate) fn into_relaxed(self) -> Vec<LintWithdrawal> {
        self.record_floor_override_cache();
        self.relaxed
    }
}

fn requirement_on_lint_floor(
    graph: &RequirementCondition,
    floor: &YamlValue,
) -> Option<ConditionalGuard> {
    let mut builder = RequirementConditionBuilder::default();
    let mut projected = HashMap::<usize, usize>::new();
    for id in graph.reachable_ids() {
        let result = match graph.node(id)? {
            RequirementConditionNode::False => RequirementConditionBuilder::FALSE,
            RequirementConditionNode::True => RequirementConditionBuilder::TRUE,
            RequirementConditionNode::Guard(guard) => {
                if holds_on(guard, floor) {
                    builder.guard(guard.clone())
                } else {
                    RequirementConditionBuilder::TRUE
                }
            }
            RequirementConditionNode::And(children) => {
                let children = children
                    .iter()
                    .map(|child| projected.get(child).copied())
                    .collect::<Option<Vec<_>>>()?;
                builder.and(children)
            }
            RequirementConditionNode::Or(children) => {
                let children = children
                    .iter()
                    .map(|child| projected.get(child).copied())
                    .collect::<Option<Vec<_>>>()?;
                builder.union(children)
            }
            RequirementConditionNode::Decision(RequirementDecision {
                test,
                when_true,
                when_false,
            })
            | RequirementConditionNode::Split(RequirementDecision {
                test,
                when_true,
                when_false,
            }) => {
                let yes = *projected.get(when_true)?;
                let no = *projected.get(when_false)?;
                let positive = if holds_on(test, floor) {
                    builder.split(test.clone(), yes, RequirementConditionBuilder::FALSE)
                } else {
                    yes
                };
                let negated = ConditionalGuard::Not(Arc::new(test.clone()));
                let negative = if holds_on(&negated, floor) {
                    builder.split(test.clone(), RequirementConditionBuilder::FALSE, no)
                } else {
                    no
                };
                builder.union([positive, negative])
            }
        };
        projected.insert(id, result);
    }
    let root = *projected.get(&graph.root())?;
    Some(ConditionalGuard::Requirement(
        builder.freeze().root(root, false),
    ))
}

fn dependency_boundary_leaves<'a>(
    boundaries: impl Iterator<Item = &'a ValuesPath>,
) -> BTreeSet<ValuesPath> {
    // A supplied descendant requires its tables. Prune in the complete path order so a
    // sibling such as `kid-aux` between `kid` and `kid.leaf` keeps the original decision.
    let mut ordered = boundaries.peekable();
    let mut leaves = BTreeSet::new();
    while let Some(path) = ordered.next() {
        if ordered
            .peek()
            .is_none_or(|next| !next.is_descendant_of(path))
        {
            leaves.insert(path.clone());
        }
    }
    leaves
}

fn condition_on_deciding_paths<'guard>(
    ancestor_segments: &[String],
    guards: impl IntoIterator<Item = &'guard ConditionalGuard>,
    deciding_paths: &BTreeSet<ValuesPath>,
    then_value: Value,
) -> Option<Value> {
    // A guard testing a deciding path for absence contradicts its
    // presence: conditioning would leave a constraint that never fires.
    if deciding_paths.is_empty()
        || guards.into_iter().any(|guard| match guard {
            ConditionalGuard::Absent { path } => deciding_paths.contains(path),
            ConditionalGuard::Requirement(graph) => {
                requirement_requires_a_deciding_absence(graph, deciding_paths)
            }
            _ => false,
        })
    {
        return None;
    }
    let mut presence = Vec::new();
    for path in deciding_paths {
        presence.push(input_path_present_condition(path, ancestor_segments)?);
    }
    let mut conditioned = serde_json::Map::new();
    conditioned.insert("if".to_string(), SchemaNode::all_of(presence).into_value());
    conditioned.insert("then".to_string(), then_value);
    Some(Value::Object(conditioned))
}

fn requirement_requires_a_deciding_absence(
    graph: &RequirementCondition,
    deciding_paths: &BTreeSet<ValuesPath>,
) -> bool {
    let tests_absence = |guard: &ConditionalGuard| match guard {
        ConditionalGuard::Absent { path } => deciding_paths.contains(path),
        _ => false,
    };
    let mut possible = HashMap::<usize, bool>::new();
    for id in graph.reachable_ids() {
        let Some(node) = graph.node(id) else {
            return false;
        };
        let child = |child| possible.get(child).copied().unwrap_or(true);
        let holds_without_absence = match node {
            RequirementConditionNode::False => false,
            RequirementConditionNode::True => true,
            RequirementConditionNode::Guard(guard) => !tests_absence(guard),
            RequirementConditionNode::And(children) => children.iter().all(child),
            RequirementConditionNode::Or(children) => children.iter().any(child),
            RequirementConditionNode::Decision(RequirementDecision {
                test,
                when_true,
                when_false,
            })
            | RequirementConditionNode::Split(RequirementDecision {
                test,
                when_true,
                when_false,
            }) => (!tests_absence(test) && child(when_true)) || child(when_false),
        };
        possible.insert(id, holds_without_absence);
    }
    possible.get(&graph.root()) == Some(&false)
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
            for guard in guards.iter() {
                collect_tested_keys(guard, paths);
            }
        }
        ConditionalGuard::Decision(graph) => {
            for RequirementDecision {
                test,
                when_true: _,
                when_false: _,
            } in graph.decisions()
            {
                collect_tested_keys(test, paths);
            }
        }
        ConditionalGuard::Requirement(graph) => {
            for guard in graph.guards() {
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
pub(crate) fn create_override_tables(
    document: &mut Cow<'_, Value>,
    coalesced: &Value,
    segments: &[&Segment],
) {
    if matches!(document, Cow::Borrowed(_))
        && !override_tables_missing(document, coalesced, segments)
    {
        return;
    }
    create_override_tables_in(document.to_mut(), coalesced, segments);
}

fn override_tables_missing(document: &Value, coalesced: &Value, segments: &[&Segment]) -> bool {
    let Some((head, tail)) = segments.split_first() else {
        return false;
    };
    let (Value::Object(members), Value::Object(coalesced_members)) = (document, coalesced) else {
        return false;
    };
    if let Some(key) = head.literal() {
        let Some(coalesced_member @ Value::Object(_)) = coalesced_members.get(key) else {
            return false;
        };
        return members
            .get(key)
            .is_none_or(|member| override_tables_missing(member, coalesced_member, tail));
    }
    coalesced_members.iter().any(|(key, coalesced_member)| {
        coalesced_member.is_object()
            && members
                .get(key)
                .is_none_or(|member| override_tables_missing(member, coalesced_member, tail))
    })
}

fn collect_missing_override_tables(
    document: Option<&Value>,
    coalesced: &Value,
    segments: &[&Segment],
    prefix: &ValuesPath,
    tables: &mut BTreeSet<ValuesPath>,
) {
    let Some((head, tail)) = segments.split_first() else {
        return;
    };
    if document.is_some_and(|value| !value.is_object()) {
        return;
    }
    let Some(coalesced) = coalesced.as_object() else {
        return;
    };
    for (key, value) in coalesced {
        if !value.is_object() || head.literal().is_some_and(|literal| literal != key) {
            continue;
        }
        let mut path = prefix.clone();
        path.push(key.as_str());
        let declared = document.and_then(|instance| instance.get(key));
        if declared.is_none() {
            tables.insert(path.clone());
        }
        collect_missing_override_tables(declared, value, tail, &path, tables);
    }
}

fn create_override_tables_in(document: &mut Value, coalesced: &Value, segments: &[&Segment]) {
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
            create_override_tables_in(member, coalesced_member, tail);
        }
        return;
    }
    for (key, coalesced_member) in coalesced_members {
        if coalesced_member.is_object() {
            let member = members
                .entry(key.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            create_override_tables_in(member, coalesced_member, tail);
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

/// Whether a constraint path, whose `None` segments match any member, and a literal document
/// path lie on one root-to-leaf line.
fn on_path(pattern: impl Iterator<Item = Option<String>>, path: &[String]) -> bool {
    pattern
        .zip(path)
        .all(|(segment, key)| segment.is_none_or(|segment| segment == *key))
}
