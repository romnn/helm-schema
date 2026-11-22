use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};

use crate::schema_node::SchemaNode;

use super::ConditionFragmentCache;

const EMPTY: &[u8] = b"{}";
const ALL_OF: &[u8] = b"{\"allOf\":[";
const ANY_OF: &[u8] = b"{\"anyOf\":[";
const END: &[u8] = b"]}";
const COMMA: &[u8] = b",";

pub(super) struct EncodedFragment {
    pub(super) schema: SchemaNode,
    canonical: OnceLock<String>,
}

impl EncodedFragment {
    pub(super) fn new(schema: SchemaNode) -> Self {
        Self {
            schema,
            canonical: OnceLock::new(),
        }
    }

    fn canonical_bytes(&self) -> &[u8] {
        self.canonical
            .get_or_init(|| {
                helm_schema_json_schema_walk::canonical_json_string(
                    &self.schema.clone().into_value(),
                )
            })
            .as_bytes()
    }
}

pub(crate) struct EncodedPrefix {
    fragments: Vec<Arc<EncodedFragment>>,
    preceding_counts: Vec<usize>,
    byte_len: OnceLock<usize>,
}

impl EncodedPrefix {
    pub(super) fn new(guards: Vec<Option<Arc<EncodedFragment>>>) -> Self {
        let mut fragments = Vec::with_capacity(guards.len());
        let mut preceding_counts = Vec::with_capacity(guards.len() + 1);
        preceding_counts.push(0);
        for fragment in guards {
            fragments.extend(fragment);
            preceding_counts.push(fragments.len());
        }
        Self {
            fragments,
            preceding_counts,
            byte_len: OnceLock::new(),
        }
    }

    pub(super) fn preceding_count(&self, index: usize) -> usize {
        self.preceding_counts
            .get(index)
            .copied()
            .unwrap_or(self.fragments.len())
    }

    fn byte_len(&self) -> usize {
        *self.byte_len.get_or_init(|| {
            self.fragments
                .iter()
                .map(|fragment| fragment.canonical_bytes().len())
                .sum()
        })
    }

    pub(crate) fn flat_schema(&self) -> SchemaNode {
        SchemaNode::all_of(
            self.fragments
                .iter()
                .map(|fragment| fragment.schema.clone())
                .collect(),
        )
    }
}

#[derive(Clone)]
pub(super) struct PositionedFragment {
    pub(super) preceding_prefix: usize,
    pub(super) fragment: Arc<EncodedFragment>,
}

/// An encoded conjunction with its original flat fragment order.
#[derive(Clone)]
pub(crate) struct EncodedConjunction {
    prefix: Option<Arc<EncodedPrefix>>,
    extras: Vec<PositionedFragment>,
}

impl EncodedConjunction {
    pub(crate) fn has_shared_prefix(&self) -> bool {
        self.prefix
            .as_ref()
            .is_some_and(|prefix| !prefix.fragments.is_empty())
    }

    pub(crate) fn owned_fragment_capacity(&self) -> usize {
        self.extras.capacity()
    }

    pub(super) fn new(prefix: Option<Arc<EncodedPrefix>>, extras: Vec<PositionedFragment>) -> Self {
        Self { prefix, extras }
    }

    fn prefix_fragments(&self) -> &[Arc<EncodedFragment>] {
        self.prefix
            .as_ref()
            .map_or(&[], |prefix| prefix.fragments.as_slice())
    }

    fn fragments(&self) -> impl Iterator<Item = &EncodedFragment> {
        let mut prefix = self.prefix_fragments().iter().enumerate().peekable();
        let mut extras = self.extras.iter().peekable();
        std::iter::from_fn(move || {
            if let Some((index, _)) = prefix.peek()
                && extras
                    .peek()
                    .is_none_or(|extra| extra.preceding_prefix > *index)
            {
                prefix.next().map(|(_, fragment)| fragment.as_ref())
            } else {
                extras.next().map(|extra| extra.fragment.as_ref())
            }
        })
    }

    fn len(&self) -> usize {
        self.prefix_fragments().len() + self.extras.len()
    }

    fn legacy_len(&self) -> usize {
        let fragment_bytes = self.prefix.as_ref().map_or(0, |prefix| prefix.byte_len())
            + self
                .extras
                .iter()
                .map(|extra| extra.fragment.canonical_bytes().len())
                .sum::<usize>();
        junction_len(ALL_OF, self.len(), fragment_bytes)
    }

    fn legacy_chunks(&self) -> impl Iterator<Item = &[u8]> {
        junction_chunks(
            ALL_OF,
            self.len(),
            self.fragments()
                .map(|fragment| std::iter::once(fragment.canonical_bytes())),
        )
    }

    fn flat_clauses(&self) -> Vec<SchemaNode> {
        let mut clauses = Vec::with_capacity(self.len());
        clauses.extend(self.fragments().map(|fragment| fragment.schema.clone()));
        clauses
    }

    pub(crate) fn legacy_type_partition(
        &self,
        mut fragment_is_partition: impl FnMut(&SchemaNode) -> bool,
    ) -> bool {
        self.len() != 0
            && self
                .fragments()
                .all(|fragment| fragment_is_partition(&fragment.schema))
    }

    pub(crate) fn flat_schema(&self) -> SchemaNode {
        SchemaNode::all_of(self.flat_clauses())
    }

    /// Compares the legacy encoded schemas without building their flat trees.
    pub(crate) fn legacy_eq(&self, other: &Self) -> bool {
        self.legacy_len() == other.legacy_len()
            && cmp_chunks(self.legacy_chunks(), other.legacy_chunks()).is_eq()
    }

    pub(crate) fn shared_lint_parts(&self) -> Option<(&Arc<EncodedPrefix>, SchemaNode)> {
        let prefix = self
            .prefix
            .as_ref()
            .filter(|prefix| !prefix.fragments.is_empty())?;
        Some((prefix, self.extras_schema()))
    }

    fn extras_schema(&self) -> SchemaNode {
        SchemaNode::all_of(
            self.extras
                .iter()
                .map(|extra| extra.fragment.schema.clone())
                .collect(),
        )
    }

    fn extras_key(&self) -> Vec<u8> {
        junction_chunks(
            ALL_OF,
            self.extras.len(),
            self.extras
                .iter()
                .map(|extra| std::iter::once(extra.fragment.canonical_bytes())),
        )
        .flatten()
        .copied()
        .collect()
    }
}

#[derive(Default)]
struct ResidualRows<'a> {
    rows: Vec<&'a EncodedConjunction>,
    seen: BTreeSet<Vec<u8>>,
}

enum MaterializedDisjunct<'a> {
    Flat(&'a EncodedConjunction),
    Shared {
        reference: Arc<SchemaNode>,
        residuals: Option<ResidualRows<'a>>,
    },
}

impl<'a> MaterializedDisjunct<'a> {
    fn add_residual(&mut self, row: &'a EncodedConjunction) {
        let Self::Shared {
            reference: _,
            residuals,
        } = self
        else {
            return;
        };
        if row.extras.is_empty() {
            *residuals = None;
        } else if let Some(residuals) = residuals
            && residuals.seen.insert(row.extras_key())
        {
            residuals.rows.push(row);
        }
    }

    fn into_schema(self) -> SchemaNode {
        match self {
            Self::Flat(row) => row.flat_schema(),
            Self::Shared {
                reference,
                residuals: None,
            } => reference.as_ref().clone(),
            Self::Shared {
                reference,
                residuals: Some(ResidualRows { rows, seen }),
            } => {
                let _large = (rows.len() >= 4_096).then(|| tracing::info_span!(target: "helm_schema_gen::counters", "materialize_large_residuals", rows = rows.len()).entered());
                drop(seen);
                let extras = SchemaNode::any_of(
                    rows.into_iter()
                        .map(EncodedConjunction::extras_schema)
                        .collect(),
                );
                SchemaNode::all_of(vec![reference.as_ref().clone(), extras])
            }
        }
    }
}

/// The exact legacy encoded disjunction and its compact materialization plan.
#[derive(Clone)]
pub(crate) struct EncodedConditionPlan {
    rows: Arc<Vec<EncodedConjunction>>,
    legacy_len: usize,
}

impl EncodedConditionPlan {
    pub(crate) fn any_of(rows: Vec<EncodedConjunction>) -> Self {
        let row_bytes = rows.iter().map(EncodedConjunction::legacy_len).sum();
        let legacy_len = junction_len(ANY_OF, rows.len(), row_bytes);
        Self {
            rows: Arc::new(rows),
            legacy_len,
        }
    }

    fn legacy_chunks(&self) -> impl Iterator<Item = &[u8]> {
        junction_chunks(
            ANY_OF,
            self.rows.len(),
            self.rows.iter().map(EncodedConjunction::legacy_chunks),
        )
    }

    pub(crate) fn flat_schema(&self) -> SchemaNode {
        SchemaNode::any_of(
            self.rows
                .iter()
                .map(EncodedConjunction::flat_schema)
                .collect(),
        )
    }

    /// Classifies the legacy condition before compact materialization can remove alternatives.
    pub(crate) fn legacy_type_partition(
        &self,
        mut fragment_is_partition: impl FnMut(&SchemaNode) -> bool,
    ) -> bool {
        !self.rows.is_empty()
            && self.rows.iter().all(|row| {
                row.len() != 0
                    && row
                        .fragments()
                        .all(|fragment| fragment_is_partition(&fragment.schema))
            })
    }

    #[tracing::instrument(skip_all, fields(rows = self.rows.len(), sharing = cache.shared))]
    pub(super) fn materialize(&self, cache: &mut ConditionFragmentCache) -> SchemaNode {
        if !cache.shared {
            return self.flat_schema();
        }
        let mut disjuncts = Vec::new();
        let mut shared = HashMap::new();
        for row in self.rows.iter() {
            let Some(prefix) = row
                .prefix
                .as_ref()
                .filter(|prefix| prefix.fragments.len() > 1)
            else {
                disjuncts.push(MaterializedDisjunct::Flat(row));
                continue;
            };
            let reference = cache.compact_prefix_reference(prefix);
            let address = Arc::as_ptr(&reference).addr();
            let index = *shared.entry(address).or_insert_with(|| {
                let index = disjuncts.len();
                disjuncts.push(MaterializedDisjunct::Shared {
                    reference,
                    residuals: Some(ResidualRows::default()),
                });
                index
            });
            if let Some(disjunct) = disjuncts.get_mut(index) {
                disjunct.add_residual(row);
            }
        }
        let residual_rows = disjuncts
            .iter()
            .map(|item| match item {
                MaterializedDisjunct::Shared {
                    reference: _,
                    residuals: Some(rows),
                } => rows.rows.len(),
                MaterializedDisjunct::Shared {
                    reference: _,
                    residuals: None,
                }
                | MaterializedDisjunct::Flat(_) => 0,
            })
            .sum::<usize>();
        let residual_key_bytes = disjuncts
            .iter()
            .map(|item| match item {
                MaterializedDisjunct::Shared {
                    reference: _,
                    residuals: Some(rows),
                } => rows.seen.iter().map(Vec::len).sum::<usize>(),
                MaterializedDisjunct::Shared {
                    reference: _,
                    residuals: None,
                }
                | MaterializedDisjunct::Flat(_) => 0,
            })
            .sum::<usize>();
        tracing::info_span!(
            target: "helm_schema_gen::counters", "condition_plan_materialization",
            disjuncts = disjuncts.len(), shared_groups = shared.len(), residual_rows, residual_key_bytes,
        ).in_scope(|| {});
        SchemaNode::any_of(
            disjuncts
                .into_iter()
                .map(MaterializedDisjunct::into_schema)
                .collect(),
        )
    }
}

impl PartialEq for EncodedConditionPlan {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for EncodedConditionPlan {}

impl PartialOrd for EncodedConditionPlan {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EncodedConditionPlan {
    fn cmp(&self, other: &Self) -> Ordering {
        if Arc::ptr_eq(&self.rows, &other.rows) {
            return Ordering::Equal;
        }
        self.legacy_len
            .cmp(&other.legacy_len)
            .then_with(|| cmp_chunks(self.legacy_chunks(), other.legacy_chunks()))
    }
}

fn junction_len(opening: &[u8], count: usize, child_bytes: usize) -> usize {
    match count {
        0 => EMPTY.len(),
        1 => child_bytes,
        _ => opening.len() + child_bytes + (count - 1) + END.len(),
    }
}

fn junction_chunks<'a, Children, Child>(
    opening: &'static [u8],
    count: usize,
    children: Children,
) -> impl Iterator<Item = &'a [u8]>
where
    Children: Iterator<Item = Child>,
    Child: Iterator<Item = &'a [u8]>,
{
    let start = match count {
        0 => Some(EMPTY),
        1 => None,
        _ => Some(opening),
    };
    start
        .into_iter()
        .chain(
            children
                .enumerate()
                .flat_map(|(index, child)| (index != 0).then_some(COMMA).into_iter().chain(child)),
        )
        .chain((count > 1).then_some(END))
}

fn cmp_chunks<'a, 'b>(
    mut left: impl Iterator<Item = &'a [u8]>,
    mut right: impl Iterator<Item = &'b [u8]>,
) -> Ordering {
    let mut left_chunk = left.next();
    let mut right_chunk = right.next();
    while let (Some(left_bytes), Some(right_bytes)) = (left_chunk, right_chunk) {
        let length = left_bytes.len().min(right_bytes.len());
        let (left_head, left_rest) = left_bytes.split_at(length);
        let (right_head, right_rest) = right_bytes.split_at(length);
        if !std::ptr::eq(left_head, right_head) {
            let order = left_head.cmp(right_head);
            if !order.is_eq() {
                return order;
            }
        }
        left_chunk = if left_rest.is_empty() {
            left.next()
        } else {
            Some(left_rest)
        };
        right_chunk = if right_rest.is_empty() {
            right.next()
        } else {
            Some(right_rest)
        };
    }
    left_chunk.is_some().cmp(&right_chunk.is_some())
}
