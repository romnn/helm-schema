use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use serde_json::Value;

use crate::condition_encoding::{EncodedConditionPlan, EncodedConjunction};
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;
use crate::uncoalesced_root::UncoalescedRootGate;

use helm_schema_core::{
    ConditionalGuard, GuardConjunction, GuardPrefix, RequirementCondition,
    RequirementConditionBuilder, RequirementConditionNode, RequirementDecision, Segment,
    ValuesPath,
};
use serde_yaml::Value as YamlValue;
use std::sync::Arc;

use crate::condition_encoding::{AbsenceDefaults, evaluate_guard_set_on_values};
use crate::values_yaml::yaml_value_at_values_path;

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct GuardPaths {
    common: Option<Vec<String>>,
    parents: Option<Vec<String>>,
    blocks_refill: bool,
}

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
enum RowArity {
    #[default]
    Empty,
    Absence(ValuesPath),
    Other,
}

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TerminalContext {
    paths: GuardPaths,
    arity: RowArity,
    requires_presence: bool,
}

impl TerminalContext {
    pub(crate) fn of_guard(
        guard: &ConditionalGuard,
        requires_presence: bool,
        role: ProofGuardRole,
    ) -> Self {
        Self {
            paths: GuardPaths::collect(std::iter::once(guard)),
            arity: match (role, guard) {
                (ProofGuardRole::Leaf, ConditionalGuard::Absent { path }) => {
                    RowArity::Absence(path.clone())
                }
                _ => RowArity::Other,
            },
            requires_presence: matches!(role, ProofGuardRole::Leaf) && requires_presence,
        }
    }

    pub(crate) fn join(&self, other: &Self) -> Self {
        let mut paths = self.paths.clone();
        paths.extend(other.paths.clone());
        let arity = match (&self.arity, &other.arity) {
            (RowArity::Empty, arity) | (arity, RowArity::Empty) => arity.clone(),
            (RowArity::Absence(left), RowArity::Absence(right)) if left == right => {
                RowArity::Absence(left.clone())
            }
            _ => RowArity::Other,
        };
        Self {
            paths,
            arity,
            requires_presence: self.requires_presence || other.requires_presence,
        }
    }

    pub(crate) fn ancestor(&self, values: &YamlValue) -> Vec<String> {
        let ancestor = self.paths.parents.clone().unwrap_or_default();
        if matches!(self.arity, RowArity::Absence(_))
            && !matches!(
                yaml_value_at_values_path(values, &path_from_segments(&ancestor)),
                Some(YamlValue::Mapping(_))
            )
        {
            Vec::new()
        } else {
            ancestor
        }
    }

    pub(crate) fn requires_presence(&self) -> bool {
        self.requires_presence
    }
}

fn path_from_segments(segments: &[String]) -> ValuesPath {
    ValuesPath::from_segments(
        segments
            .iter()
            .map(|part| Segment::from_encoded_component(part)),
    )
}

pub(crate) struct RequirementPartition<Context> {
    pub(crate) context: Context,
    pub(crate) condition: RequirementCondition,
}

type ConditionsByContext<Context> = BTreeMap<Context, usize>;

#[derive(Clone, PartialEq, Eq, Hash)]
enum RequirementRecipe<'graph> {
    False,
    True,
    Guard(&'graph ConditionalGuard),
    And([usize; 2]),
    Union(Rc<[usize]>),
    Split {
        test: &'graph ConditionalGuard,
        when_true: usize,
        when_false: usize,
    },
}

impl RequirementRecipe<'_> {
    fn children(&self) -> impl Iterator<Item = usize> + '_ {
        let (parts, branches) = match self {
            Self::False | Self::True | Self::Guard(_) => (&[][..], None),
            Self::And(parts) => (parts.as_slice(), None),
            Self::Union(parts) => (parts.as_ref(), None),
            Self::Split {
                test: _,
                when_true,
                when_false,
            } => (&[][..], Some([*when_true, *when_false])),
        };
        parts.iter().copied().chain(branches.into_iter().flatten())
    }

    fn materialize(
        &self,
        builder: &mut RequirementConditionBuilder,
        mapped: &[Option<usize>],
    ) -> Option<usize> {
        let resolve = |id: usize| mapped.get(id).copied().flatten();
        Some(match self {
            Self::False => RequirementConditionBuilder::FALSE,
            Self::True => RequirementConditionBuilder::TRUE,
            Self::Guard(guard) => builder.guard((*guard).clone()),
            Self::And([left, right]) => builder.and([resolve(*left)?, resolve(*right)?]),
            Self::Union(parts) => builder.union(
                parts
                    .iter()
                    .copied()
                    .map(resolve)
                    .collect::<Option<Vec<_>>>()?,
            ),
            Self::Split {
                test,
                when_true,
                when_false,
            } => builder.split((*test).clone(), resolve(*when_true)?, resolve(*when_false)?),
        })
    }
}

/// Records shared builder operations without cloning or normalizing borrowed guards.
struct RequirementRecipes<'graph> {
    nodes: Vec<RequirementRecipe<'graph>>,
    interned: HashMap<RequirementRecipe<'graph>, usize>,
}

impl<'graph> RequirementRecipes<'graph> {
    const FALSE: usize = 0;
    const TRUE: usize = 1;

    fn new() -> Self {
        Self {
            nodes: vec![RequirementRecipe::False, RequirementRecipe::True],
            interned: HashMap::new(),
        }
    }

    fn guard(&mut self, guard: &'graph ConditionalGuard) -> usize {
        // Each guard leaf already has one interned source node and one postorder visit.
        let id = self.nodes.len();
        self.nodes.push(RequirementRecipe::Guard(guard));
        id
    }

    fn and(&mut self, mut children: [usize; 2]) -> usize {
        children.sort_unstable();
        match children {
            [Self::FALSE, _] => Self::FALSE,
            [Self::TRUE, right] => right,
            [left, right] if left == right => left,
            children => self.intern(RequirementRecipe::And(children)),
        }
    }

    fn union(&mut self, mut children: Vec<usize>) -> usize {
        children.retain(|child| *child != Self::FALSE);
        children.sort_unstable();
        children.dedup();
        match children.as_slice() {
            [] => Self::FALSE,
            [only] => *only,
            _ => self.intern(RequirementRecipe::Union(children.into())),
        }
    }

    fn split(
        &mut self,
        test: &'graph ConditionalGuard,
        when_true: usize,
        when_false: usize,
    ) -> usize {
        if when_true == Self::FALSE && when_false == Self::FALSE {
            return Self::FALSE;
        }
        self.intern(RequirementRecipe::Split {
            test,
            when_true,
            when_false,
        })
    }

    fn intern(&mut self, recipe: RequirementRecipe<'graph>) -> usize {
        *self.interned.entry(recipe).or_insert_with_key(|recipe| {
            let id = self.nodes.len();
            self.nodes.push(recipe.clone());
            id
        })
    }

    fn materialize<Context>(
        self,
        roots: ConditionsByContext<Context>,
    ) -> Option<Vec<RequirementPartition<Context>>> {
        let Self { nodes, interned } = self;
        drop(interned);
        let mut reachable = vec![false; nodes.len()];
        for &id in roots.values() {
            *reachable.get_mut(id)? = true;
        }
        for (id, recipe) in nodes.iter().enumerate().rev() {
            if *reachable.get(id)? {
                for child in recipe.children() {
                    *reachable.get_mut(child)? = true;
                }
            }
        }
        let mut builder = RequirementConditionBuilder::default();
        let mut mapped = vec![None; nodes.len()];
        let mut materialized_recipes = 0_usize;
        for (id, (recipe, live)) in nodes.into_iter().zip(reachable).enumerate() {
            if live {
                let resolved = recipe.materialize(&mut builder, &mapped)?;
                *mapped.get_mut(id)? = Some(resolved);
                materialized_recipes += 1;
            }
        }
        tracing::Span::current().record("materialized_recipes", materialized_recipes);
        let conditions = builder.freeze();
        roots
            .into_iter()
            .map(|(context, id)| {
                Some(RequirementPartition {
                    context,
                    condition: conditions.root(mapped.get(id).copied().flatten()?, false),
                })
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ProofGuardRole {
    Leaf,
    SplitTest,
}

struct PartitionWork<'a> {
    remaining: usize,
    complete: &'a mut bool,
}

impl PartitionWork<'_> {
    fn charge(&mut self, requested: usize) -> bool {
        let Some(next) = self.remaining.checked_sub(requested) else {
            *self.complete = false;
            let _bound = tracing::debug_span!(
                target: "helm_schema_gen::counters", "requirement_partition_bound",
                requested, remaining = self.remaining
            )
            .entered();
            return false;
        };
        self.remaining = next;
        true
    }
}

/// Groups a requirement's proof paths by the facts a consequence can observe.
///
/// Every intermediate partition keeps a root per judgment context.
/// A bounded node retains only completed proofs. An exhausted conjunction contributes
/// no proofs, but its siblings may still establish the requirement.
/// A complete final singleton can keep the original graph.
#[tracing::instrument(
    target = "helm_schema_gen::counters",
    skip_all,
    fields(
        source_nodes = tracing::field::Empty,
        max_node_contexts = tracing::field::Empty,
        contexts = tracing::field::Empty,
        recipes = tracing::field::Empty,
        materialized_recipes = 0_usize,
    )
)]
pub(crate) fn partition_requirement<Context: Clone + Default + Ord>(
    graph: &RequirementCondition,
    observe: impl Fn(&ConditionalGuard, ProofGuardRole) -> Context,
    join: impl Fn(&Context, &Context) -> Context,
) -> Option<Vec<RequirementPartition<Context>>> {
    let mut recipes = RequirementRecipes::new();
    let mut partitions = HashMap::<usize, ConditionsByContext<Context>>::new();
    let source_nodes = graph.reachable_ids();
    let mut max_node_contexts = 0;
    let mut complete = true;
    for id in source_nodes.iter().copied() {
        let mut work = PartitionWork {
            remaining: 1 << 14,
            complete: &mut complete,
        };
        let parts = match graph.node(id)? {
            RequirementConditionNode::False => BTreeMap::new(),
            RequirementConditionNode::True => {
                BTreeMap::from([(Context::default(), RequirementRecipes::TRUE)])
            }
            RequirementConditionNode::Guard(guard) => {
                BTreeMap::from([(observe(guard, ProofGuardRole::Leaf), recipes.guard(guard))])
            }
            RequirementConditionNode::And(children) => {
                partition_conjunction(children, &partitions, &join, &mut recipes, &mut work)?
            }
            RequirementConditionNode::Or(children) => {
                let mut alternatives = BTreeMap::<Context, Vec<usize>>::new();
                for child in children {
                    let child = partitions.get(child)?;
                    if !work.charge(child.len()) {
                        continue;
                    }
                    for (context, &root) in child {
                        alternatives.entry(context.clone()).or_default().push(root);
                    }
                }
                disjoin_partitions(alternatives, &mut recipes)
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
            }) => partition_decision(
                test,
                partitions.get(when_true)?,
                partitions.get(when_false)?,
                &observe,
                &join,
                &mut recipes,
                &mut work,
            ),
        };
        max_node_contexts = max_node_contexts.max(parts.len());
        partitions.insert(id, parts);
    }
    let roots = partitions.remove(&graph.root())?;
    let span = tracing::Span::current();
    span.record("source_nodes", source_nodes.len());
    span.record("max_node_contexts", max_node_contexts);
    span.record("contexts", roots.len());
    span.record("recipes", recipes.nodes.len());
    if roots.is_empty() {
        return Some(Vec::new());
    }
    if complete && roots.len() == 1 {
        return Some(
            roots
                .into_keys()
                .map(|context| RequirementPartition {
                    context,
                    condition: graph.clone(),
                })
                .collect(),
        );
    }
    recipes.materialize(roots)
}

fn partition_conjunction<Context: Clone + Default + Ord>(
    children: &[usize],
    partitions: &HashMap<usize, ConditionsByContext<Context>>,
    join: &impl Fn(&Context, &Context) -> Context,
    recipes: &mut RequirementRecipes<'_>,
    work: &mut PartitionWork<'_>,
) -> Option<ConditionsByContext<Context>> {
    let mut joint = BTreeMap::from([(Context::default(), RequirementRecipes::TRUE)]);
    for child in children {
        let right_parts = partitions.get(child)?;
        let pairs = joint.len().saturating_mul(right_parts.len());
        if !work.charge(pairs) {
            return Some(BTreeMap::new());
        }
        let mut combined = BTreeMap::<Context, Vec<usize>>::new();
        for (left, &left_root) in &joint {
            for (right, &right_root) in right_parts {
                combined
                    .entry(join(left, right))
                    .or_default()
                    .push(recipes.and([left_root, right_root]));
            }
        }
        joint = disjoin_partitions(combined, recipes);
    }
    Some(joint)
}

fn disjoin_partitions<Context: Ord>(
    parts: BTreeMap<Context, Vec<usize>>,
    recipes: &mut RequirementRecipes<'_>,
) -> ConditionsByContext<Context> {
    parts
        .into_iter()
        .map(|(context, roots)| (context, recipes.union(roots)))
        .collect()
}

#[derive(Default)]
struct DecisionAlternatives {
    when_true: Vec<usize>,
    when_false: Vec<usize>,
}

fn partition_decision<'graph, Context: Ord>(
    test: &'graph ConditionalGuard,
    when_true: &ConditionsByContext<Context>,
    when_false: &ConditionsByContext<Context>,
    observe: &impl Fn(&ConditionalGuard, ProofGuardRole) -> Context,
    join: &impl Fn(&Context, &Context) -> Context,
    recipes: &mut RequirementRecipes<'graph>,
    work: &mut PartitionWork<'_>,
) -> ConditionsByContext<Context> {
    let positive = observe(test, ProofGuardRole::SplitTest);
    let negative = observe(
        &ConditionalGuard::Not(Arc::new(test.clone())),
        ProofGuardRole::SplitTest,
    );
    let mut partitions = BTreeMap::<Context, DecisionAlternatives>::new();
    if work.charge(when_true.len()) {
        for (context, &root) in when_true {
            partitions
                .entry(join(context, &positive))
                .or_default()
                .when_true
                .push(root);
        }
    }
    if work.charge(when_false.len()) {
        for (context, &root) in when_false {
            partitions
                .entry(join(context, &negative))
                .or_default()
                .when_false
                .push(root);
        }
    }
    partitions
        .into_iter()
        .map(
            |(
                context,
                DecisionAlternatives {
                    when_true,
                    when_false,
                },
            )| {
                let when_true = recipes.union(when_true);
                let when_false = recipes.union(when_false);
                (context, recipes.split(test, when_true, when_false))
            },
        )
        .collect()
}

impl GuardPaths {
    fn collect<'a>(guards: impl IntoIterator<Item = &'a ConditionalGuard>) -> Self {
        let mut paths = Self::default();
        for guard in guards {
            for path in guard.value_paths() {
                let mut segments = path
                    .segments()
                    .map(Segment::encode_component)
                    .collect::<Vec<_>>();
                paths.blocks_refill |= segments.iter().any(|part| part == "*" || part == "global");
                intersect_prefix(&mut paths.common, &segments);
                segments.pop();
                intersect_prefix(&mut paths.parents, &segments);
            }
        }
        paths
    }

    fn extend(&mut self, other: Self) {
        let Self {
            common,
            parents,
            blocks_refill,
        } = other;
        if let Some(common) = common {
            intersect_prefix(&mut self.common, &common);
        }
        if let Some(parents) = parents {
            intersect_prefix(&mut self.parents, &parents);
        }
        self.blocks_refill |= blocks_refill;
    }
}

fn intersect_prefix(common: &mut Option<Vec<String>>, path: &[String]) {
    if let Some(common) = common {
        let length = common
            .iter()
            .zip(path)
            .take_while(|(left, right)| left == right)
            .count();
        common.truncate(length);
    } else {
        *common = Some(path.to_vec());
    }
}

struct PrefixPaths {
    prefix: GuardPrefix,
    paths: GuardPaths,
    refill_truth: BTreeMap<usize, bool>,
}

/// Paths read by terminal guards during one immutable document projection.
#[derive(Default)]
pub(crate) struct TerminalPathCache {
    prefixes: BTreeMap<usize, PrefixPaths>,
}

impl TerminalPathCache {
    fn prefix(&mut self, prefix: &GuardPrefix) -> &mut PrefixPaths {
        let address = prefix.as_slice().as_ptr().addr();
        let cached = self.prefixes.entry(address).or_insert_with(|| PrefixPaths {
            prefix: prefix.clone(),
            paths: GuardPaths::collect(prefix.canonical().iter()),
            refill_truth: BTreeMap::new(),
        });
        debug_assert!(cached.prefix.shares_storage(prefix));
        cached
    }

    fn paths(&mut self, row: &GuardConjunction) -> GuardPaths {
        let Some((prefix, extras)) = row.canonical_parts() else {
            return GuardPaths::collect(row);
        };
        let mut paths = self.prefix(prefix).paths.clone();
        paths.extend(GuardPaths::collect(extras));
        paths
    }

    pub(crate) fn ancestor(&mut self, row: &GuardConjunction, values: &YamlValue) -> Vec<String> {
        let ancestor = self.paths(row).parents.unwrap_or_default();
        let path = ValuesPath::from_segments(
            ancestor
                .iter()
                .map(|part| Segment::from_encoded_component(part)),
        );
        if row.len() == 1
            && matches!(row.iter().next(), Some(ConditionalGuard::Absent { .. }))
            && !matches!(
                yaml_value_at_values_path(values, &path),
                Some(YamlValue::Mapping(_))
            )
        {
            Vec::new()
        } else {
            ancestor
        }
    }

    pub(crate) fn deleted_dependency_root<'a>(
        &mut self,
        row: &GuardConjunction,
        absence: AbsenceDefaults<'a>,
    ) -> Option<&'a [String]> {
        let Some((prefix, extras)) = row.canonical_parts() else {
            return crate::condition_encoding::deleted_dependency_root_terminates(row, absence);
        };
        let paths = self.paths(row);
        if paths.blocks_refill {
            return None;
        }
        let common = paths.common?;
        let root = absence
            .dependency_roots
            .iter()
            .filter(|root| common.starts_with(root))
            .max_by_key(|root| root.len())?;
        let cached = self.prefix(prefix);
        // The projection keeps each refill document immutable and alive with this cache.
        let document = std::ptr::from_ref(absence.dependency_refill).addr();
        let prefix_holds = *cached.refill_truth.entry(document).or_insert_with(|| {
            evaluate_guard_set_on_values(prefix.canonical().iter(), absence.dependency_refill)
                == Some(true)
        });
        let holds = prefix_holds
            && evaluate_guard_set_on_values(extras, absence.dependency_refill) == Some(true);
        holds.then_some(root.as_slice())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TerminalKey {
    ancestor: Vec<String>,
    then_schema: Value,
    type_partition: bool,
}

struct TerminalClass {
    ordinal: usize,
    key: TerminalKey,
}

struct TerminalRow {
    class: Rc<TerminalClass>,
    condition: EncodedConjunction,
}

struct TerminalBatch {
    class: Rc<TerminalClass>,
    conditions: Vec<EncodedConjunction>,
}

/// Consequences already judged against their individual runtime and lint contexts.
pub(crate) struct TerminalEmissions {
    classes: HashMap<TerminalKey, Rc<TerminalClass>>,
    rows: Vec<TerminalRow>,
    coalescible: bool,
}

impl Default for TerminalEmissions {
    fn default() -> Self {
        Self {
            classes: HashMap::new(),
            rows: Vec::new(),
            coalescible: true,
        }
    }
}

impl TerminalEmissions {
    pub(crate) fn record_storage(&self) {
        tracing::info_span!(
            target: "helm_schema_gen::counters", "terminal_emission_storage",
            rows = self.rows.len(), classes = self.classes.len(),
            flat_rows = self.rows.iter().filter(|row|!row.condition.has_shared_prefix()).count(),
            owned_fragments = self.rows.iter().map(|row|row.condition.owned_fragment_capacity()).sum::<usize>(),
        ).in_scope(|| {});
    }

    pub(crate) fn push(
        &mut self,
        gate: &UncoalescedRootGate<'_>,
        ancestor: &[String],
        condition: EncodedConjunction,
        then_schema: Value,
    ) {
        self.coalescible &= then_schema == Value::Bool(false)
            || then_schema.as_object().is_some_and(|object| {
                object.len() == 2
                    && object.contains_key("if")
                    && object.get("then") == Some(&Value::Bool(false))
            });
        let key = TerminalKey {
            ancestor: ancestor.to_vec(),
            then_schema,
            type_partition: gate.conjunction_is_type_partition(&condition),
        };
        let class = if let Some(class) = self.classes.get(&key) {
            Rc::clone(class)
        } else {
            let class = Rc::new(TerminalClass {
                ordinal: self.classes.len(),
                key: key.clone(),
            });
            self.classes.insert(key, Rc::clone(&class));
            class
        };
        self.rows.push(TerminalRow { class, condition });
    }

    #[tracing::instrument(skip_all, fields(rows = self.rows.len(), classes = self.classes.len()))]
    pub(crate) fn append(self, document: &mut SchemaDocument, gate: &UncoalescedRootGate<'_>) {
        let Self {
            classes,
            rows,
            coalescible,
        } = self;
        drop(classes);
        if !coalescible || !gate.allows_terminal_coalescing() {
            for TerminalRow { class, condition } in rows {
                gate.append_planned_conditional(
                    document,
                    &class.key.ancestor,
                    &EncodedConditionPlan::any_of(vec![condition]),
                    SchemaNode::foreign(class.key.then_schema.clone()),
                );
            }
            return;
        }
        let mut batches = BTreeMap::<usize, TerminalBatch>::new();
        for TerminalRow { class, condition } in rows {
            batches
                .entry(class.ordinal)
                .or_insert_with(|| TerminalBatch {
                    class,
                    conditions: Vec::new(),
                })
                .conditions
                .push(condition);
        }
        for TerminalBatch { class, conditions } in batches.into_values() {
            gate.append_planned_conditional(
                document,
                &class.key.ancestor,
                &EncodedConditionPlan::any_of(conditions),
                SchemaNode::foreign(class.key.then_schema.clone()),
            );
        }
    }
}
