//! Shared, bounded coverage projections of frozen heap history.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use ahash::AHashSet;
use helm_schema_core::{Predicate, ValuesPath};

use super::heap_query::QueryBudget;
use super::{
    AliasSet, HeapNode, HeapVersion, Multiplicity, ObjectData, ObjectId, ObjectVersion, Remainder,
    SnapshotId,
};

#[cfg(test)]
#[path = "../tests/runtime_heap_coverage.rs"]
mod tests;

type Link = Option<Rc<IndexNode>>;
type Regions = Option<Rc<RegionExpr>>;

#[derive(Debug)]
struct RegionExpr {
    id: u64,
    kind: RegionKind,
    allocation: CacheAllocation,
}

#[derive(Debug)]
enum RegionKind {
    Paths(Rc<BTreeSet<ValuesPath>>),
    InputObject(ObjectId),
    Union {
        yes: Rc<RegionExpr>,
        no: Rc<RegionExpr>,
    },
}

#[derive(Debug)]
struct IndexNode {
    regions: Regions,
    kind: IndexKind,
    allocation: CacheAllocation,
}

#[derive(Debug)]
enum IndexKind {
    Leaf {
        object: ObjectId,
        cursor: SnapshotId,
        storage: StorageCoverage,
    },
    Branch {
        left: Link,
        right: Link,
    },
}

struct Contribution {
    regions: Regions,
    storage: StorageCoverage,
}

enum WidenedJoin {
    NotApplicable,
    Included(Contribution),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WidenedCoverage {
    revision: SnapshotId,
    multiplicity: Multiplicity,
    inputs: Rc<BTreeSet<ValuesPath>>,
}

#[derive(Debug)]
enum StorageCoverage {
    Widened(WidenedCoverage),
    EmptyMap {
        multiplicity: Multiplicity,
        inputs: Option<Rc<BTreeSet<ValuesPath>>>,
    },
    Other,
}

struct WidenedMerge<'coverage> {
    inputs: &'coverage Rc<BTreeSet<ValuesPath>>,
    other: Option<&'coverage Rc<BTreeSet<ValuesPath>>>,
    first: Multiplicity,
    second: Multiplicity,
    combine: InputCombination,
}

enum InputCombination {
    Join,
    Extend,
}

impl<'coverage> WidenedMerge<'coverage> {
    fn from_storage(
        yes: &'coverage StorageCoverage,
        no: &'coverage StorageCoverage,
    ) -> Option<Self> {
        Some(match (yes, no) {
            (StorageCoverage::Widened(yes), StorageCoverage::Widened(no)) => Self {
                inputs: &yes.inputs,
                other: Some(&no.inputs),
                first: yes.multiplicity,
                second: no.multiplicity,
                combine: InputCombination::Join,
            },
            (
                StorageCoverage::Widened(wide),
                StorageCoverage::EmptyMap {
                    multiplicity,
                    inputs,
                },
            ) => Self {
                inputs: &wide.inputs,
                other: inputs.as_ref(),
                first: wide.multiplicity,
                second: *multiplicity,
                combine: InputCombination::Extend,
            },
            (
                StorageCoverage::EmptyMap {
                    multiplicity,
                    inputs,
                },
                StorageCoverage::Widened(wide),
            ) => Self {
                inputs: &wide.inputs,
                other: inputs.as_ref(),
                first: *multiplicity,
                second: wide.multiplicity,
                combine: InputCombination::Extend,
            },
            (
                StorageCoverage::Widened(WidenedCoverage {
                    revision: _,
                    multiplicity: _,
                    inputs: _,
                })
                | StorageCoverage::EmptyMap {
                    multiplicity: _,
                    inputs: _,
                }
                | StorageCoverage::Other,
                StorageCoverage::Widened(WidenedCoverage {
                    revision: _,
                    multiplicity: _,
                    inputs: _,
                })
                | StorageCoverage::EmptyMap {
                    multiplicity: _,
                    inputs: _,
                }
                | StorageCoverage::Other,
            ) => return None,
        })
    }
}

#[derive(Clone, Default)]
struct CoverageView {
    height: u32,
    root: Link,
}

#[derive(Clone)]
enum CoverageAnswer {
    Known(CoverageView),
    Exhausted { work_limit: usize },
}

#[derive(Clone, Copy)]
struct PendingFrame<'version> {
    version: &'version HeapVersion,
    work_limit: usize,
}

enum Task<'version> {
    Visit(&'version HeapVersion),
    Unary(PendingFrame<'version>),
    Other {
        frame: PendingFrame<'version>,
        no: &'version HeapVersion,
    },
    Join {
        frame: PendingFrame<'version>,
        yes: CoverageView,
    },
}

impl<'version> Task<'version> {
    fn frame(&self, work_limit: usize) -> PendingFrame<'version> {
        match self {
            Self::Visit(version) => PendingFrame {
                version,
                work_limit,
            },
            Self::Unary(frame) | Self::Other { frame, no: _ } | Self::Join { frame, yes: _ } => {
                *frame
            }
        }
    }
}

#[derive(Debug)]
struct CacheAllocation {
    retained: Rc<Cell<usize>>,
    weight: usize,
}

impl CacheAllocation {
    fn release(&mut self) {
        self.retained.set(self.retained.get() - self.weight);
        self.weight = 0;
    }
}

impl Drop for CacheAllocation {
    fn drop(&mut self) {
        self.release();
    }
}

impl Drop for IndexNode {
    fn drop(&mut self) {
        self.allocation.release();
    }
}

impl Drop for RegionExpr {
    fn drop(&mut self) {
        self.allocation.release();
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct RegionPair {
    yes: u64,
    no: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FlatKey {
    region: u64,
    inputs: u64,
}

#[derive(Debug)]
struct FlatPaths {
    paths: BTreeSet<ValuesPath>,
    work: usize,
    weight: usize,
}

#[derive(Default)]
pub(super) struct CoverageProjection {
    views: BTreeMap<u64, CoverageAnswer>,
    retained: Rc<Cell<usize>>,
    next_region: u64,
    input_sets: BTreeMap<*const BTreeSet<ValuesPath>, Weak<RegionExpr>>,
    input_objects: BTreeMap<ObjectId, Weak<RegionExpr>>,
    unions: BTreeMap<RegionPair, Weak<RegionExpr>>,
    flat: BTreeMap<FlatKey, FlatPaths>,
    flat_payload: usize,
}

impl std::fmt::Debug for CoverageProjection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CoverageProjection")
            .field("views", &self.views.len())
            .field("retained", &self.retained.get())
            .finish_non_exhaustive()
    }
}

const MAX_COVERAGE_WORK: usize = 1_048_576;
const MAX_COVERAGE_CACHE: usize = 1_048_576;

impl CoverageProjection {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Equal conditions share one flattening of the derived provenance graph.
    /// Completed views are evictable; pending folds keep their operands alive.
    #[tracing::instrument(skip_all, fields(cached_views = self.views.len(), retained = self.retained.get(), roots = roots.len(), flat_hit = tracing::field::Empty))]
    pub(super) fn project(
        &mut self,
        condition: &Predicate,
        inputs: &super::input_paths::InputPaths,
        roots: &[&HeapVersion],
        reads: &mut super::read_set::ReadSet,
    ) {
        let mut seen = AHashSet::new();
        let mut paths = BTreeSet::new();
        let mut budget = QueryBudget::new(MAX_COVERAGE_WORK);
        let mut capped = false;
        'roots: for root in roots {
            let CoverageAnswer::Known(view) = self.query(root, MAX_COVERAGE_WORK) else {
                capped = true;
                continue;
            };
            let root_region = view.root.as_ref().and_then(|node| node.regions.as_ref());
            let key = (roots.len() == 1)
                .then(|| {
                    root_region.map(|region| FlatKey {
                        region: region.id,
                        inputs: inputs.id(),
                    })
                })
                .flatten();
            if let Some(flat) = key.and_then(|key| self.flat.get(&key)) {
                if tracing::enabled!(tracing::Level::DEBUG) {
                    tracing::Span::current().record("flat_hit", true);
                }
                if budget.charge(flat.work) {
                    paths.extend(flat.paths.iter().cloned());
                } else {
                    capped = true;
                }
                break;
            }
            let before = budget.remaining;
            let mut regions: Vec<_> = root_region.cloned().into_iter().collect();
            while let Some(region) = regions.pop() {
                if !seen.insert(region.id) {
                    continue;
                }
                if !budget.charge(1) {
                    capped = true;
                    break 'roots;
                }
                match &region.kind {
                    RegionKind::Paths(known) => {
                        if !budget.charge(known.len()) {
                            capped = true;
                            break 'roots;
                        }
                        paths.extend(known.iter().cloned());
                    }
                    RegionKind::InputObject(object) => {
                        if !budget.charge(inputs.lookup_work()) {
                            capped = true;
                            break 'roots;
                        }
                        paths.extend(inputs.get(object).cloned());
                    }
                    RegionKind::Union { yes, no } => {
                        regions.extend([Rc::clone(yes), Rc::clone(no)]);
                    }
                }
            }
            if let Some(key) = key {
                self.remember_paths(key, &paths, before - budget.remaining);
            }
        }
        if capped {
            paths.insert(ValuesPath::default());
        }
        for path in paths {
            reads.insert(super::RuntimeRead {
                path,
                condition: condition.clone(),
                coverage: super::ReadCoverage::UnknownRegion,
            });
        }
    }

    fn remember_paths(&mut self, key: FlatKey, paths: &BTreeSet<ValuesPath>, work: usize) {
        let weight = paths.iter().fold(1_usize, |weight, path| {
            path.segments()
                .fold(weight.saturating_add(1), |weight, segment| {
                    weight.saturating_add(
                        segment
                            .literal()
                            .map_or(1, |key| key.len().saturating_add(1)),
                    )
                })
        });
        if weight > 262_144 {
            return;
        }
        while self.flat.len() >= 4096 || self.flat_payload + weight > 262_144 {
            let Some((_, removed)) = self.flat.pop_first() else {
                return;
            };
            self.flat_payload -= removed.weight;
        }
        self.flat.insert(
            key,
            FlatPaths {
                paths: paths.clone(),
                work,
                weight,
            },
        );
        self.flat_payload += weight;
    }

    fn query(&mut self, root: &HeapVersion, work_limit: usize) -> CoverageAnswer {
        let mut budget = QueryBudget::new(work_limit);
        if let Some(view) = self.view(root, &mut budget) {
            return CoverageAnswer::Known(view);
        }
        tracing::debug!(
            root = root.id.ordinal,
            remaining = budget.remaining,
            cached_views = self.views.len(),
            retained = self.retained.get(),
            "coverage query incomplete"
        );
        self.remember_exhaustion(
            PendingFrame {
                version: root,
                work_limit,
            },
            &budget,
        );
        CoverageAnswer::Exhausted { work_limit }
    }

    fn remember_exhaustion(&mut self, frame: PendingFrame<'_>, budget: &QueryBudget) {
        if !budget.is_exhausted() {
            return;
        }
        match self.views.get(&frame.version.id.ordinal) {
            Some(CoverageAnswer::Known(_)) => return,
            Some(CoverageAnswer::Exhausted { work_limit }) if *work_limit >= frame.work_limit => {
                return;
            }
            Some(CoverageAnswer::Exhausted { work_limit: _ }) | None => {}
        }
        if self.reserve_cache(1).is_some() {
            self.views.insert(
                frame.version.id.ordinal,
                CoverageAnswer::Exhausted {
                    work_limit: frame.work_limit,
                },
            );
        }
    }

    fn reserve(&mut self, budget: &mut QueryBudget, weight: usize) -> Option<()> {
        if !budget.charge(weight) {
            return None;
        }
        self.reserve_cache(weight)
    }

    fn reserve_cache(&mut self, weight: usize) -> Option<()> {
        loop {
            let entries = self.views.len()
                + self.input_sets.len()
                + self.input_objects.len()
                + self.unions.len();
            if self
                .retained
                .get()
                .saturating_add(entries)
                .saturating_add(weight)
                <= MAX_COVERAGE_CACHE
            {
                return Some(());
            }
            if self.views.pop_first().is_some()
                || self.unions.pop_first().is_some()
                || self.input_sets.pop_first().is_some()
                || self.input_objects.pop_first().is_some()
            {
                continue;
            }
            return None;
        }
    }

    fn allocate(&mut self, budget: &mut QueryBudget, weight: usize) -> Option<CacheAllocation> {
        self.reserve(budget, weight + 1)?;
        self.retained.set(self.retained.get() + weight);
        Some(CacheAllocation {
            retained: Rc::clone(&self.retained),
            weight,
        })
    }

    fn region(
        &mut self,
        kind: RegionKind,
        weight: usize,
        budget: &mut QueryBudget,
    ) -> Option<Rc<RegionExpr>> {
        let allocation = self.allocate(budget, weight)?;
        let id = self.next_region;
        self.next_region += 1;
        Some(Rc::new(RegionExpr {
            id,
            kind,
            allocation,
        }))
    }

    fn union(&mut self, yes: Regions, no: Regions, budget: &mut QueryBudget) -> Option<Regions> {
        match (yes, no) {
            (Some(yes), Some(no)) if !Rc::ptr_eq(&yes, &no) => {
                let key = RegionPair {
                    yes: yes.id.min(no.id),
                    no: yes.id.max(no.id),
                };
                if let Some(region) = self.unions.get(&key).and_then(Weak::upgrade) {
                    return Some(Some(region));
                }
                let region = self.region(RegionKind::Union { yes, no }, 1, budget)?;
                self.unions.insert(key, Rc::downgrade(&region));
                Some(Some(region))
            }
            (yes, no) => Some(yes.or(no)),
        }
    }

    fn leaf(
        &mut self,
        object: &ObjectId,
        cursor: SnapshotId,
        contribution: Contribution,
        budget: &mut QueryBudget,
    ) -> Option<Rc<IndexNode>> {
        let Contribution { regions, storage } = contribution;
        let allocation = self.allocate(budget, 1)?;
        Some(Rc::new(IndexNode {
            allocation,
            regions,
            kind: IndexKind::Leaf {
                object: object.clone(),
                cursor,
                storage,
            },
        }))
    }

    fn branch(
        &mut self,
        left: Link,
        right: Link,
        budget: &mut QueryBudget,
    ) -> Option<Rc<IndexNode>> {
        let regions = self.union(
            left.as_ref().and_then(|node| node.regions.clone()),
            right.as_ref().and_then(|node| node.regions.clone()),
            budget,
        )?;
        let allocation = self.allocate(budget, 1)?;
        Some(Rc::new(IndexNode {
            allocation,
            regions,
            kind: IndexKind::Branch { left, right },
        }))
    }

    fn contribution(
        &mut self,
        version: &HeapVersion,
        object: &ObjectId,
        budget: &mut QueryBudget,
    ) -> Option<Contribution> {
        let storage = version.query_object(object, budget);
        if budget.remaining == 0 {
            budget.exhaust();
            return None;
        }

        self.data_contribution(storage.as_deref(), object, budget)
    }

    fn data_contribution(
        &mut self,
        stored: Option<&ObjectVersion>,
        object: &ObjectId,
        budget: &mut QueryBudget,
    ) -> Option<Contribution> {
        let data = stored.map(|stored| &stored.data);
        let regions = match data {
            Some(
                ObjectData::Widened { inputs }
                | ObjectData::Map {
                    fields: _,
                    remainder: Remainder::Unknown { inputs },
                },
            ) => {
                let known = self.known(inputs, budget)?;
                let path = self.input_object(object, budget)?;
                self.union(known, path, budget)?
            }
            Some(ObjectData::Map {
                fields: _,
                remainder: Remainder::Open,
            }) => self.input_object(object, budget)?,
            Some(
                ObjectData::Map {
                    fields: _,
                    remainder: Remainder::Closed | Remainder::Input,
                }
                | ObjectData::Strings(_)
                | ObjectData::Opaque { reachable: _ },
            )
            | None => None,
        };
        Some(Contribution {
            regions,
            storage: match stored {
                Some(ObjectVersion {
                    revision,
                    multiplicity,
                    data: ObjectData::Widened { inputs },
                }) => StorageCoverage::Widened(WidenedCoverage {
                    revision: *revision,
                    multiplicity: *multiplicity,
                    inputs: Rc::clone(inputs),
                }),
                Some(ObjectVersion {
                    revision: _,
                    multiplicity,
                    data: ObjectData::Map { fields, remainder },
                }) if fields.is_empty() => StorageCoverage::EmptyMap {
                    multiplicity: *multiplicity,
                    inputs: match remainder {
                        Remainder::Unknown { inputs } => Some(Rc::clone(inputs)),
                        Remainder::Closed | Remainder::Input | Remainder::Open => None,
                    },
                },
                Some(ObjectVersion {
                    revision: _,
                    multiplicity: _,
                    data:
                        ObjectData::Map {
                            fields: _,
                            remainder: _,
                        }
                        | ObjectData::Strings(_)
                        | ObjectData::Opaque { reachable: _ },
                })
                | None => StorageCoverage::Other,
            },
        })
    }

    fn known(
        &mut self,
        inputs: &Rc<BTreeSet<ValuesPath>>,
        budget: &mut QueryBudget,
    ) -> Option<Regions> {
        let key = Rc::as_ptr(inputs);
        if let Some(region) = self.input_sets.get(&key).and_then(Weak::upgrade) {
            return Some(Some(region));
        }
        let region = self.region(
            RegionKind::Paths(Rc::clone(inputs)),
            inputs.len() + 1,
            budget,
        )?;
        self.input_sets.insert(key, Rc::downgrade(&region));
        Some(Some(region))
    }

    fn input_object(&mut self, object: &ObjectId, budget: &mut QueryBudget) -> Option<Regions> {
        if let Some(region) = self.input_objects.get(object).and_then(Weak::upgrade) {
            return Some(Some(region));
        }
        let region = self.region(RegionKind::InputObject(object.clone()), 1, budget)?;
        self.input_objects
            .insert(object.clone(), Rc::downgrade(&region));
        Some(Some(region))
    }

    fn insert(
        &mut self,
        node: &Link,
        leaf: Rc<IndexNode>,
        height: u32,
        object: &ObjectId,
        budget: &mut QueryBudget,
    ) -> Option<Link> {
        if !budget.charge(1) {
            return None;
        }
        if height == 0 {
            return Some(Some(leaf));
        }
        let (left, right) = match node.as_deref().map(|node| &node.kind) {
            Some(IndexKind::Branch { left, right }) => (left.clone(), right.clone()),
            Some(IndexKind::Leaf {
                object: _,
                cursor: _,
                storage: _,
            })
            | None => (None, None),
        };
        let next = height - 1;
        let (left, right) = if object.ordinal & (1 << next) == 0 {
            (self.insert(&left, leaf, next, object, budget)?, right)
        } else {
            (left, self.insert(&right, leaf, next, object, budget)?)
        };
        Some(Some(self.branch(left, right, budget)?))
    }

    fn grow(
        &mut self,
        mut view: CoverageView,
        height: u32,
        budget: &mut QueryBudget,
    ) -> Option<CoverageView> {
        while view.height < height {
            if view.root.is_some() {
                view.root = Some(self.branch(view.root, None, budget)?);
            }
            view.height += 1;
        }
        Some(view)
    }

    fn update(
        &mut self,
        view: CoverageView,
        version: &HeapVersion,
        object: &ObjectId,
        budget: &mut QueryBudget,
    ) -> Option<CoverageView> {
        let height = view.height.max(object.ordinal.bit_width());
        let view = self.grow(view, height, budget)?;
        let regions = match &version.node {
            HeapNode::Install {
                parent: _,
                object: _,
                storage,
            } => self.data_contribution(Some(storage), object, budget)?,
            HeapNode::WriteSlot {
                parent: _,
                object: _,
                index: _,
                value: _,
            } => Contribution {
                regions: None,
                storage: StorageCoverage::Other,
            },
            HeapNode::Empty
            | HeapNode::Input { parent: _ }
            | HeapNode::Write {
                parent: _,
                object: _,
                key: _,
                field: _,
            }
            | HeapNode::UnknownWrite {
                parent: _,
                objects: _,
                inputs: _,
            }
            | HeapNode::Join {
                decision: _,
                yes: _,
                no: _,
            } => self.contribution(version, object, budget)?,
        };
        let leaf = self.leaf(object, version.id, regions, budget)?;
        let root = self.insert(&view.root, leaf, height, object, budget)?;
        Some(CoverageView { height, root })
    }

    fn widened_join(
        &mut self,
        version: &HeapVersion,
        object: &ObjectId,
        yes_storage: &StorageCoverage,
        no_storage: &StorageCoverage,
        budget: &mut QueryBudget,
    ) -> Option<WidenedJoin> {
        if let HeapNode::Join {
            decision: _,
            yes,
            no,
        } = &version.node
        {
            match (yes_storage, no_storage) {
                (StorageCoverage::Widened(wide), StorageCoverage::Other) => {
                    return self.widened_with_object(version, object, wide, no, budget);
                }
                (StorageCoverage::Other, StorageCoverage::Widened(wide)) => {
                    return self.widened_with_object(version, object, wide, yes, budget);
                }
                (
                    StorageCoverage::Widened(WidenedCoverage {
                        revision: _,
                        multiplicity: _,
                        inputs: _,
                    })
                    | StorageCoverage::EmptyMap {
                        multiplicity: _,
                        inputs: _,
                    }
                    | StorageCoverage::Other,
                    StorageCoverage::Widened(WidenedCoverage {
                        revision: _,
                        multiplicity: _,
                        inputs: _,
                    })
                    | StorageCoverage::EmptyMap {
                        multiplicity: _,
                        inputs: _,
                    }
                    | StorageCoverage::Other,
                ) => {}
            }
        }
        let Some(operands) = WidenedMerge::from_storage(yes_storage, no_storage) else {
            return Some(WidenedJoin::NotApplicable);
        };
        let WidenedMerge {
            inputs,
            other,
            first,
            second,
            combine,
        } = operands;
        let arena = version.arena.upgrade();
        let inputs = match combine {
            InputCombination::Join => other.map_or_else(
                || Rc::clone(inputs),
                |other| {
                    super::heap::join_input_sets(
                        inputs,
                        other,
                        arena.as_ref().map(|arena| &arena.input_unions),
                        budget,
                    )
                },
            ),
            InputCombination::Extend => {
                if !budget.charge(2) {
                    return None;
                }
                let mut combined = Rc::clone(inputs);
                if let Some(other) = other
                    && !other.is_subset(inputs)
                {
                    Rc::make_mut(&mut combined).extend(other.iter().cloned());
                }
                combined
            }
        };
        self.finish_widened(version, object, inputs, first.join(second), budget)
            .map(WidenedJoin::Included)
    }

    fn widened_with_object(
        &mut self,
        version: &HeapVersion,
        object: &ObjectId,
        wide: &WidenedCoverage,
        branch: &Rc<HeapVersion>,
        budget: &mut QueryBudget,
    ) -> Option<WidenedJoin> {
        let Some(storage) = branch.query_object(object, budget) else {
            return Some(WidenedJoin::NotApplicable);
        };
        if budget.is_exhausted() {
            return None;
        }
        if matches!(storage.data, ObjectData::Widened { inputs: _ }) {
            return Some(WidenedJoin::NotApplicable);
        }
        let added = super::FrozenValue::object_inputs(branch, object, &storage, budget);
        let mut inputs = Rc::clone(&wide.inputs);
        if !added.is_subset(&inputs) {
            Rc::make_mut(&mut inputs).extend(added);
        }
        self.finish_widened(
            version,
            object,
            inputs,
            wide.multiplicity.join(storage.multiplicity),
            budget,
        )
        .map(WidenedJoin::Included)
    }

    fn finish_widened(
        &mut self,
        version: &HeapVersion,
        object: &ObjectId,
        mut inputs: Rc<BTreeSet<ValuesPath>>,
        multiplicity: Multiplicity,
        budget: &mut QueryBudget,
    ) -> Option<Contribution> {
        if budget.is_exhausted() || !budget.charge(version.input_paths.lookup_work()) {
            return None;
        }
        if let Some(path) = version.input_paths.get(object)
            && !inputs.contains(path)
        {
            Rc::make_mut(&mut inputs).insert(path.clone());
        }
        let known = self.known(&inputs, budget)?;
        let path = self.input_object(object, budget)?;
        let regions = self.union(known, path, budget)?;
        Some(Contribution {
            regions,
            storage: StorageCoverage::Widened(WidenedCoverage {
                revision: version.id,
                multiplicity,
                inputs,
            }),
        })
    }

    fn merge(
        &mut self,
        yes: &Link,
        no: &Link,
        version: &HeapVersion,
        budget: &mut QueryBudget,
    ) -> Option<Link> {
        if !budget.charge(1) {
            return None;
        }
        match (yes.as_ref(), no.as_ref()) {
            (None, None) => Some(None),
            (Some(yes), Some(no)) if Rc::ptr_eq(yes, no) => Some(Some(Rc::clone(yes))),
            (Some(yes), Some(no)) => self.merge_nodes(yes, no, version, budget),
            (Some(node), None) | (None, Some(node)) => self.missing(node, node, version, budget),
        }
    }

    fn merge_nodes(
        &mut self,
        yes: &Rc<IndexNode>,
        no: &Rc<IndexNode>,
        version: &HeapVersion,
        budget: &mut QueryBudget,
    ) -> Option<Link> {
        match (&yes.kind, &no.kind) {
            (
                IndexKind::Leaf {
                    object: yes_object,
                    cursor: yes_cursor,
                    storage: yes_storage,
                },
                IndexKind::Leaf {
                    object: no_object,
                    cursor: no_cursor,
                    storage: no_storage,
                },
            ) => {
                if yes_object != no_object {
                    return None;
                }
                if yes_cursor == no_cursor
                    || matches!((yes_storage, no_storage), (StorageCoverage::Widened(yes), StorageCoverage::Widened(no)) if yes == no)
                {
                    return Some(Some(Rc::clone(yes)));
                }
                let contribution = match self.widened_join(
                    version,
                    yes_object,
                    yes_storage,
                    no_storage,
                    budget,
                )? {
                    WidenedJoin::Included(contribution) => contribution,
                    WidenedJoin::NotApplicable => self.contribution(version, yes_object, budget)?,
                };
                Some(Some(self.leaf(
                    yes_object,
                    version.id,
                    contribution,
                    budget,
                )?))
            }
            (
                IndexKind::Branch {
                    left: yes_left,
                    right: yes_right,
                },
                IndexKind::Branch {
                    left: no_left,
                    right: no_right,
                },
            ) => {
                let left = self.merge(yes_left, no_left, version, budget)?;
                let right = self.merge(yes_right, no_right, version, budget)?;
                Some(Some(self.branch(left, right, budget)?))
            }
            (
                IndexKind::Leaf {
                    object: _,
                    cursor: _,
                    storage: _,
                },
                IndexKind::Branch { left: _, right: _ },
            )
            | (
                IndexKind::Branch { left: _, right: _ },
                IndexKind::Leaf {
                    object: _,
                    cursor: _,
                    storage: _,
                },
            ) => None,
        }
    }

    fn missing(
        &mut self,
        node: &IndexNode,
        shared: &Rc<IndexNode>,
        version: &HeapVersion,
        budget: &mut QueryBudget,
    ) -> Option<Link> {
        match &node.kind {
            IndexKind::Leaf {
                object,
                cursor: _,
                storage: _,
            } if version.input_paths.contains_key(object) => {
                let regions = self.contribution(version, object, budget)?;
                Some(Some(self.leaf(object, version.id, regions, budget)?))
            }
            IndexKind::Leaf {
                object: _,
                cursor: _,
                storage: _,
            } => Some(Some(Rc::clone(shared))),
            IndexKind::Branch { left, right } => {
                let left = self.merge(left, &None, version, budget)?;
                let right = self.merge(right, &None, version, budget)?;
                Some(Some(self.branch(left, right, budget)?))
            }
        }
    }

    fn havoc(
        &mut self,
        node: Link,
        version: &HeapVersion,
        objects: &AliasSet,
        inputs: &Rc<BTreeSet<ValuesPath>>,
        budget: &mut QueryBudget,
    ) -> Option<Link> {
        let Some(node) = node else {
            return Some(None);
        };
        if !budget.charge(1) {
            return None;
        }
        match &node.kind {
            IndexKind::Leaf {
                object,
                cursor: _,
                storage: _,
            } => {
                let affected = match objects {
                    AliasSet::Known(objects) => objects.contains(object),
                    AliasSet::RenderGraph => true,
                };
                if !affected {
                    return Some(Some(node));
                }
                let known = self.known(inputs, budget)?;
                let path = self.input_object(object, budget)?;
                let regions = self.union(known, path, budget)?;
                Some(Some(self.leaf(
                    object,
                    version.id,
                    Contribution {
                        regions,
                        storage: StorageCoverage::Widened(WidenedCoverage {
                            revision: version.id,
                            multiplicity: version.allocations.get(object)?,
                            inputs: Rc::clone(inputs),
                        }),
                    },
                    budget,
                )?))
            }
            IndexKind::Branch { left, right } => {
                let left = self.havoc(left.clone(), version, objects, inputs, budget)?;
                let right = self.havoc(right.clone(), version, objects, inputs, budget)?;
                Some(Some(self.branch(left, right, budget)?))
            }
        }
    }

    fn joined_view(
        &mut self,
        version: &HeapVersion,
        yes: CoverageView,
        no: CoverageView,
        budget: &mut QueryBudget,
    ) -> Option<CoverageView> {
        let height = yes.height.max(no.height);
        let yes = self.grow(yes, height, budget)?;
        let no = self.grow(no, height, budget)?;
        let root = self.merge(&yes.root, &no.root, version, budget)?;
        Some(CoverageView { height, root })
    }

    fn unary_view(
        &mut self,
        version: &HeapVersion,
        parent: CoverageView,
        budget: &mut QueryBudget,
    ) -> Option<CoverageView> {
        match &version.node {
            HeapNode::Empty => Some(CoverageView::default()),
            HeapNode::Input { parent: _ } => Some(parent),
            HeapNode::Install {
                parent: _,
                object,
                storage: _,
            }
            | HeapNode::Write {
                parent: _,
                object,
                key: _,
                field: _,
            }
            | HeapNode::WriteSlot {
                parent: _,
                object,
                index: _,
                value: _,
            } => self.update(parent, version, object, budget),
            HeapNode::UnknownWrite {
                parent: _,
                objects,
                inputs,
            } => {
                let root = self.havoc(parent.root, version, objects, inputs, budget)?;
                Some(CoverageView {
                    height: parent.height,
                    root,
                })
            }
            HeapNode::Join {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }
    }

    #[tracing::instrument(skip_all, level = "debug")]
    fn view(&mut self, root: &HeapVersion, budget: &mut QueryBudget) -> Option<CoverageView> {
        let mut pending = vec![Task::Visit(root)];
        let mut results = Vec::new();
        while let Some(task) = pending.pop() {
            let frame = task.frame(budget.remaining);
            if matches!(&task, Task::Visit(_))
                && let Some(CoverageAnswer::Exhausted { work_limit }) =
                    self.views.get(&frame.version.id.ordinal)
                && frame.work_limit <= *work_limit
            {
                budget.exhaust();
                return None;
            }
            if !budget.charge(1) {
                self.remember_exhaustion(frame, budget);
                return None;
            }
            let view = match task {
                Task::Visit(version) => {
                    if let Some(CoverageAnswer::Known(view)) = self.views.get(&version.id.ordinal) {
                        results.push(view.clone());
                        continue;
                    }
                    match &version.node {
                        HeapNode::Empty => Some(CoverageView::default()),
                        HeapNode::Input { parent }
                        | HeapNode::Install {
                            parent,
                            object: _,
                            storage: _,
                        }
                        | HeapNode::Write {
                            parent,
                            object: _,
                            key: _,
                            field: _,
                        }
                        | HeapNode::WriteSlot {
                            parent,
                            object: _,
                            index: _,
                            value: _,
                        }
                        | HeapNode::UnknownWrite {
                            parent,
                            objects: _,
                            inputs: _,
                        } => {
                            pending.extend([Task::Unary(frame), Task::Visit(parent)]);
                            continue;
                        }
                        HeapNode::Join {
                            decision: _,
                            yes,
                            no,
                        } => {
                            pending.extend([Task::Other { frame, no }, Task::Visit(yes)]);
                            continue;
                        }
                    }
                }
                Task::Unary(frame) => self.unary_view(frame.version, results.pop()?, budget),
                Task::Other { frame, no } => {
                    let yes = results.pop()?;
                    pending.extend([Task::Join { frame, yes }, Task::Visit(no)]);
                    continue;
                }
                Task::Join { frame, yes } => {
                    self.joined_view(frame.version, yes, results.pop()?, budget)
                }
            };
            let Some(view) = view else {
                self.remember_exhaustion(frame, budget);
                return None;
            };
            if self.reserve(budget, 1).is_none() {
                self.remember_exhaustion(frame, budget);
                return None;
            }
            self.views.insert(
                frame.version.id.ordinal,
                CoverageAnswer::Known(view.clone()),
            );
            results.push(view);
        }
        results.pop()
    }
}
