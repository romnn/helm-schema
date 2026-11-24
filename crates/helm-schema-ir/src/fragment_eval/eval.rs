//! Contract claims lowered directly from the sealed executed YAML owner.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{
    GuardDnf, Predicate, ValuesPath, dynamic_mapping_value_path, sequence_item_path,
};
use helm_schema_syntax::{ScalarStyle, YamlNodeKind};

use crate::analysis_db::IrAnalysisDb;
use crate::contract::ContractIr;
use crate::resource_identity::PlacedResource;
use crate::value::yaml::PayloadShape;

use crate::value::yaml::{NodeIx, PlacedDocument, PlacedTerm};
use crate::value::{DocumentExecution, Observation, ObservationId, RenderedContainerUse};
use crate::{ContractProvenance, ContractUse, SourceSpan, ValueKind, YamlPath};

/// A pathless input read observed during execution.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ValueRead {
    pub(crate) values_path: ValuesPath,
    pub(crate) kind: ValueKind,
    pub(crate) condition: GuardDnf,
    pub(crate) resource: Option<crate::ResourceRef>,
    pub(crate) provenance: Vec<ContractProvenance>,
    pub(crate) dependency: bool,
}

#[tracing::instrument(skip_all, level = "debug", fields(source_path))]
pub(crate) fn project_execution_at_offset(
    _source: &str,
    source_path: Option<&str>,
    source_offset: usize,
    db: &IrAnalysisDb,
    execution: &DocumentExecution,
) -> ContractIr {
    let mut contract = ContractIr::default();
    let mut observed = execution.observed_facts().clone();
    for document in execution.placed_documents() {
        let resources = crate::resource_identity::placed_resources(&document, execution);
        observed.resources.extend(resources.iter().map(|resource| {
            crate::observed_facts::ObservedResource {
                condition: resource.condition.clone(),
                resource: resource.resource.clone(),
            }
        }));

        execution.with_relationship_query(PLACEMENT_ALLOWANCE, |query| {
            let projection = Projection {
                document,
                query: std::cell::RefCell::new(query),
                execution,
                source_path,
                source_offset,
                db,
                resources,
                depth: 0,
            };
            let mut rows = Vec::new();
            for index in projection.document.roots() {
                projection.node(index, &YamlPath(Vec::new()), &Predicate::True, &mut rows);
            }
            for row in rows {
                contract.push(row);
            }
        });
    }
    for read in execution.projection_reads() {
        if read.condition.is_never() {
            continue;
        }
        let row = ContractUse::with_condition_and_provenances(
            read.values_path,
            YamlPath(Vec::new()),
            read.kind,
            read.condition,
            read.resource,
            read.provenance,
        );
        if read.dependency {
            contract.push_dependency_use(row);
        } else {
            contract.push(row);
        }
    }
    contract.absorb_observed_facts(&observed);
    contract
}

struct Projection<'a, 'inputs> {
    document: PlacedDocument<'a>,
    query: std::cell::RefCell<crate::value::relationship::RelationshipQuery<'a, 'inputs>>,
    execution: &'a DocumentExecution,
    source_path: Option<&'a str>,
    source_offset: usize,
    db: &'a IrAnalysisDb,
    resources: Vec<PlacedResource>,
    depth: usize,
}

/// Work one placement's relationship and guard queries may spend; each placement has its own.
const PLACEMENT_ALLOWANCE: usize = 1024;

impl<'a> Projection<'a, '_> {
    fn payload(
        &self,
        value: &'a crate::value::FrozenValue,
    ) -> Vec<crate::value::use_policy::RelationshipUse<'a>> {
        let result = self
            .query
            .borrow_mut()
            .with_allowance(PLACEMENT_ALLOWANCE, |query| query.value(value, &[]));
        result
            .known
            .into_iter()
            .filter_map(crate::value::relationship::Relationship::destination)
            .collect()
    }

    fn node(
        &self,
        index: NodeIx,
        path: &YamlPath,
        ambient: &Predicate,
        contract: &mut Vec<ContractUse>,
    ) {
        let Some(kind) = self.document.kind(index) else {
            return;
        };
        // Literal siblings still enter their parent's overwrite proof but cannot emit rows.
        if !self.document.has_hole(index) {
            return;
        }
        let guard = self.db.predicate_memo().normalize(Predicate::all(vec![
            ambient.clone(),
            self.document.condition.clone(),
            self.document.selection_guard(index),
        ]));
        if guard == Predicate::False {
            return;
        }
        let rows = self.serialized(index, path, &guard);
        if !rows.is_empty() {
            for row in rows {
                contract.push(row);
            }
            return;
        }
        match kind {
            YamlNodeKind::Mapping | YamlNodeKind::FlowMapping => {
                self.mapping(index, path, &guard, contract);
            }
            YamlNodeKind::Sequence | YamlNodeKind::FlowSequence => {
                let path = sequence_item_path(path);
                for child in self.document.children(index) {
                    self.node(*child, &path, &guard, contract);
                }
            }
            YamlNodeKind::Document | YamlNodeKind::Item => {
                for child in self.document.children(index) {
                    self.node(*child, path, &guard, contract);
                }
            }
            YamlNodeKind::Scalar(style) => {
                self.scalar(index, style, false, path, &guard, contract);
            }
            YamlNodeKind::BlockScalar => {
                self.scalar(index, ScalarStyle::Plain, true, path, &guard, contract);
            }
            YamlNodeKind::Pair
            | YamlNodeKind::FlowPair
            | YamlNodeKind::Comment
            | YamlNodeKind::DocumentEnd => {}
        }
    }

    fn literal_key(&self, index: NodeIx) -> Option<String> {
        self.document.literal_key(index).map(str::to_owned)
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::fragment_eval::projection_work"
    )]
    fn mapping(
        &self,
        index: NodeIx,
        path: &YamlPath,
        ambient: &Predicate,
        contract: &mut Vec<ContractUse>,
    ) {
        let pairs = self.document.children(index);
        let mut payloads = pairs
            .iter()
            .filter_map(|pair| {
                self.document.kind(*pair)?;
                let rows = self.serialized(*pair, path, ambient);
                (!rows.is_empty()).then_some((*pair, rows))
            })
            .collect::<BTreeMap<_, _>>();
        let siblings = pairs
            .iter()
            .filter(|pair| !payloads.contains_key(pair))
            .filter_map(|pair| {
                self.document
                    .mapping_key(*pair)
                    .and_then(|key| self.literal_key(key))
            })
            .collect::<BTreeSet<_>>();
        let active = payloads
            .keys()
            .filter_map(|index| {
                let placement = self.document.serialization(*index)?;
                let guard = placement.term.execution_guard(
                    self.execution,
                    self.db.predicate_memo(),
                    &mut { PLACEMENT_ALLOWANCE },
                );
                Some((
                    *index,
                    self.db.predicate_memo().normalize(Predicate::all(vec![
                        self.document.condition.clone(),
                        self.document.term_selection_guard(&placement.term),
                        guard,
                    ])),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let survivors = self.document.mapping_survivors(
            pairs,
            &active,
            self.db.predicate_memo(),
            |pair, name| self.payload_member_absent(pair, name),
        );
        for pair in pairs {
            if let Some(rows) = payloads.remove(pair) {
                let later_keys = pairs
                    .iter()
                    .filter(|other| *other > pair)
                    .filter_map(|other| {
                        self.document
                            .mapping_key(*other)
                            .map(|key| self.literal_key(key))
                    })
                    .collect::<Option<BTreeSet<_>>>();
                let Some(later_keys) = later_keys else {
                    continue;
                };
                // A whole-input claim requires the other unknown member contributions to be inactive.
                let unique = Predicate::all(
                    active
                        .iter()
                        .filter(|(other, _)| *other != pair)
                        .map(|(_, guard)| guard.negated())
                        .collect(),
                );
                for mut row in rows {
                    row.condition = self.guard(&row.condition, &unique);
                    row.omitted_members
                        .extend(later_keys.iter().cloned().map(|key| (key, Vec::new())));
                    row.template_supplied_member_keys = siblings.clone();
                    if !row.condition.is_never() {
                        contract.push(row);
                    }
                }
                continue;
            }
            let Some(key) = self.document.mapping_key(*pair) else {
                continue;
            };
            let child_path = if let Some(key) = self.literal_key(key) {
                let mut path = path.clone();
                path.0.push(key);
                path
            } else {
                dynamic_mapping_value_path(path)
            };
            let survives = self.db.predicate_memo().normalize(Predicate::all(vec![
                ambient.clone(),
                self.document.placement_guard(key),
                survivors.get(pair).cloned().unwrap_or(Predicate::False),
            ]));
            for child in self
                .document
                .children(*pair)
                .iter()
                .filter(|child| **child != key)
            {
                self.node(*child, &child_path, &survives, contract);
            }
        }
    }

    fn payload_member_absent(&self, pair: NodeIx, key: &str) -> Predicate {
        let Some(placement) = self.document.serialization(pair) else {
            return Predicate::False;
        };
        let Some(alternatives) = placement.term.frozen_alternatives() else {
            return Predicate::False;
        };
        let guards = self
            .query
            .borrow_mut()
            .with_allowance(PLACEMENT_ALLOWANCE, |query| {
                alternatives
                    .into_iter()
                    .map(|(condition, value)| {
                        let view = query.member(value, &[key.to_owned()]);
                        Predicate::all(vec![condition, view.presence.absent])
                    })
                    .collect()
            });
        self.db.predicate_memo().normalize(Predicate::Or(guards))
    }

    fn serialized(&self, node: NodeIx, path: &YamlPath, ambient: &Predicate) -> Vec<ContractUse> {
        let Some(placement) = self.document.serialization(node) else {
            return Vec::new();
        };
        let term = &placement.term;
        let Some(alternatives) = term.frozen_alternatives() else {
            return Vec::new();
        };
        let occurrence = term.execution_guard(self.execution, self.db.predicate_memo(), &mut {
            PLACEMENT_ALLOWANCE
        });

        let mut rows = Vec::new();
        let mut captures = BTreeMap::new();
        for (guard, value) in alternatives {
            for arm in self.payload(value) {
                if let Some((graph, _capture)) = arm.capture {
                    let target = path
                        .0
                        .iter()
                        .chain(&arm.output_path)
                        .cloned()
                        .collect::<Vec<_>>();
                    let key = (std::rc::Rc::as_ptr(graph), target);
                    let condition = self.db.predicate_memo().normalize(Predicate::all(vec![
                        ambient.clone(),
                        guard.clone(),
                        occurrence.clone(),
                        arm.condition,
                        self.document.term_placement_guard(term),
                    ]));
                    captures
                        .entry(key)
                        .and_modify(|(_, known): &mut (_, Predicate)| {
                            *known = self
                                .db
                                .predicate_memo()
                                .normalize(Predicate::Or(vec![known.clone(), condition.clone()]));
                        })
                        .or_insert((graph, condition));
                    continue;
                }
                let member = !arm.output_path.is_empty();
                let mut conditions = vec![
                    ambient.clone(),
                    guard.clone(),
                    occurrence.clone(),
                    arm.condition.clone(),
                    self.document.term_placement_guard(term),
                ];
                if !member && self.document.flow_context(node) && !arm.flow_node {
                    if !arm.omitted_members.is_empty() {
                        continue;
                    }
                    conditions.push(arm.flow_condition());
                }
                if !member && placement.shape == PayloadShape::Scalar {
                    conditions.push(arm.scalar_condition());
                    if !self.document.block_value(node) {
                        conditions.push(arm.flow_condition());
                    }
                }
                let mut row = ContractUse::with_condition_and_provenances(
                    arm.source.as_ref().clone(),
                    YamlPath(path.0.iter().chain(&arm.output_path).cloned().collect()),
                    arm.kind,
                    GuardDnf::from_conjunction(conditions),
                    None,
                    std::iter::empty(),
                );
                row.strip_marker = arm.strip_marker;
                row.stringified = arm.stringified;
                row.nil_omitting = arm.nil_omitting;
                row.digest = arm.digest;
                row.omitted_members = arm
                    .omitted_members
                    .into_iter()
                    .map(|key| (key, Vec::new()))
                    .collect();
                rows.extend(self.stamped_serialized(row, &placement, node));
            }
        }
        for ((_, target), (graph, condition)) in captures {
            rows.extend(self.captured(graph, term, node, &YamlPath(target), &condition));
        }
        rows.retain(|row| !row.condition.is_never());
        rows
    }

    fn stamped_serialized(
        &self,
        mut row: ContractUse,
        placement: &crate::value::yaml::PlacedSerialization<'_>,
        node: NodeIx,
    ) -> Vec<ContractUse> {
        let term = &placement.term;
        let mut result = Vec::new();
        if placement.sequence {
            let mut items = row.clone();
            items.source_expr.push_each_member();
            items.condition = self.guard(
                &items.condition,
                &Predicate::all(vec![
                    placement.peers.clone(),
                    Predicate::from(helm_schema_core::Guard::TypeIs {
                        path: row.source_expr.clone(),
                        schema_type: "array".to_owned(),
                    }),
                ]),
            );
            result.extend(self.stamp(items, term, node, None));
            row.condition = self.guard(&row.condition, &placement.peers.negated());
            if let Some(last) = row.path.0.pop() {
                if let Some(parent) = last.strip_suffix("[*]") {
                    if !parent.is_empty() {
                        row.path.0.push(parent.to_owned());
                    }
                } else {
                    row.path.0.push(last);
                }
            }
        }
        result.extend(self.stamp(row, term, node, None));
        result
    }

    fn captured(
        &self,
        graph: &std::rc::Rc<crate::value::TextBuffer>,
        outer: &PlacedTerm<'_>,
        node: NodeIx,
        path: &YamlPath,
        ambient: &Predicate,
    ) -> Vec<ContractUse> {
        if self.depth >= 32 {
            return Vec::new();
        }
        let placement = self
            .execution
            .place_capture(graph, ambient, &mut { PLACEMENT_ALLOWANCE });
        let Some(placement) = placement else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for document in placement.placed_documents() {
            let resources = if path.0.is_empty() {
                crate::resource_identity::placed_resources(&document, self.execution)
            } else {
                Vec::new()
            };
            let complete = document.completion_guard();
            self.execution
                .with_relationship_query(PLACEMENT_ALLOWANCE, |query| {
                    let projection = Projection {
                        document,
                        query: std::cell::RefCell::new(query),
                        execution: self.execution,
                        source_path: self.source_path,
                        source_offset: self.source_offset,
                        db: self.db,
                        resources,
                        depth: self.depth + 1,
                    };
                    let mut contract = Vec::new();
                    for index in projection.document.roots() {
                        projection.node(index, &YamlPath(Vec::new()), &complete, &mut contract);
                    }
                    for mut row in contract {
                        row.path.0.splice(0..0, path.0.iter().cloned());
                        row.condition = self.guard(&row.condition, ambient);
                        result.extend(self.stamp(row, outer, node, None));
                    }
                });
        }
        result
    }

    fn visible(&self, term: &PlacedTerm<'_>, context: &BTreeSet<ObservationId>) -> bool {
        if self.depth != 0 {
            return true;
        }
        if context.is_empty() {
            term.occurrences()
                .any(|id| self.execution.is_root_observation(id))
        } else {
            context
                .iter()
                .any(|id| self.execution.is_root_observation(*id))
        }
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::fragment_eval::projection_work"
    )]
    fn scalar(
        &self,
        node: NodeIx,
        style: ScalarStyle,
        suppressed: bool,
        path: &YamlPath,
        ambient: &Predicate,
        contract: &mut Vec<ContractUse>,
    ) {
        let terms = self.document.holes(node);
        let symbolic = terms.len();
        for term in &terms {
            let context = term.context().collect::<BTreeSet<_>>();
            if !self.visible(term, &context) {
                continue;
            }
            let whole = symbolic == 1
                && self.document.covers(term, node)
                && style == ScalarStyle::Plain
                && !suppressed
                && self.document.block_value(node);
            let (kind, path) = scalar_placement(whole, suppressed, path);
            let mut occurrences = BTreeSet::new();
            for id in term.occurrences().filter(|id| !context.contains(id)) {
                if !occurrences.insert(id) {
                    continue;
                }
                let Some(guard) = term.occurrence_guard(id) else {
                    continue;
                };
                for observation in self.execution.observations(id) {
                    let Observation::Value(site) = observation else {
                        continue;
                    };
                    if !site.rendered() {
                        continue;
                    }
                    let condition =
                        Predicate::all(vec![ambient.clone(), guard.clone(), site.condition()]);
                    let scalar = std::cell::OnceCell::new();
                    for policy in self.payload(site.value()) {
                        if policy.capture.is_some() || !policy.output_path.is_empty() {
                            continue;
                        }
                        if policy.split_segment.is_some() && !whole {
                            continue;
                        }
                        if !policy.stringified
                            && !policy.digest
                            && !scalar.get_or_init(|| site.rendered_scalar().is_some())
                        {
                            continue;
                        }
                        let mut row = ContractUse::with_condition_and_provenances(
                            policy.source.into_owned(),
                            path.clone(),
                            if policy.kind == ValueKind::Serialized || policy.digest {
                                ValueKind::Serialized
                            } else {
                                kind
                            },
                            GuardDnf::from_conjunction([
                                condition.clone(),
                                policy.condition,
                                if kind == ValueKind::PartialScalar {
                                    Predicate::True
                                } else {
                                    self.document.placement_guard(node)
                                },
                            ]),
                            None,
                            std::iter::empty(),
                        );
                        row.text_preimage = if whole { policy.text_preimage } else { None };
                        row.split_segment = policy.split_segment.cloned();
                        row.stringified = true;
                        row.digest = policy.digest;
                        row.nil_omitting = policy.nil_omitting;
                        contract.extend(self.stamp_scalar(row, term, node, id, whole));
                    }
                    if whole {
                        for RenderedContainerUse {
                            condition: selected,
                            path: source,
                            kind,
                        } in site.formatted_containers()
                        {
                            let row = ContractUse::with_condition_and_provenances(
                                source,
                                path.clone(),
                                kind,
                                GuardDnf::from_conjunction([
                                    condition.clone(),
                                    selected,
                                    self.document.placement_guard(node),
                                ]),
                                None,
                                std::iter::empty(),
                            );
                            contract.extend(self.stamp_scalar(row, term, node, id, whole));
                        }
                    }
                }
            }
        }
    }

    fn stamp_scalar(
        &self,
        mut row: ContractUse,
        term: &PlacedTerm<'_>,
        node: NodeIx,
        id: ObservationId,
        whole: bool,
    ) -> Vec<ContractUse> {
        if whole {
            row.plain_scalar_placement = helm_schema_core::PlainScalarPlacement::BlockValue;
        }
        self.stamp(row, term, node, Some(id))
    }

    fn guard(&self, condition: &GuardDnf, guard: &Predicate) -> GuardDnf {
        GuardDnf::from_conjunction([self.db.predicate_memo().normalize(Predicate::all(vec![
            condition_predicate(condition),
            guard.clone(),
        ]))])
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::fragment_eval::projection_work"
    )]
    fn stamp(
        &self,
        mut row: ContractUse,
        term: &PlacedTerm<'_>,
        node: NodeIx,
        occurrence: Option<ObservationId>,
    ) -> Vec<ContractUse> {
        let provenance = term
            .context()
            .chain(occurrence)
            .chain(term.occurrences())
            .flat_map(|id| self.execution.observations(id))
            .find_map(|observation| {
                let (path, source) = match observation {
                    Observation::Text(site) => (site.source_path(), site.span()),
                    Observation::Value(site) => (site.source_path(), site.span()),
                };
                let path = path?;
                let offset = if Some(path) == self.source_path {
                    self.source_offset
                } else {
                    0
                };
                Some(ContractProvenance::new(
                    path,
                    SourceSpan::new(
                        source.start.checked_add(offset)?,
                        source.end.checked_add(offset)?,
                    ),
                    Vec::new(),
                ))
            });
        row.provenance.extend(provenance);
        row.provenance.sort();
        row.provenance.dedup();
        let innermost = self.document.innermost(
            node,
            &self
                .resources
                .iter()
                .map(|resource| resource.node)
                .collect::<Vec<_>>(),
        );
        if innermost.is_empty() {
            return vec![row];
        }
        self.resources
            .iter()
            .filter(|resource| innermost.contains(&resource.node))
            .map(|resource| {
                let mut row = row.clone();
                row.condition = self.guard(&row.condition, &resource.condition);
                row.resource = Some(resource.resource.clone());
                if !row.stringified && row.omitted_members.is_empty() && !row.path.0.is_empty() {
                    row.condition = GuardDnf::from_disjunction(
                        row.condition.disjuncts().iter().map(|conjunction| {
                            conjunction.iter().map(|predicate| match predicate.kind() {
                                helm_schema_core::PredicateKind::Approximate {
                                    role,
                                    sound_subset: Some(subset),
                                    ..
                                } if role
                                    == helm_schema_core::ApproximationRole::OutputSelection
                                    && predicate.value_paths().contains(&row.source_expr) =>
                                {
                                    subset.clone()
                                }
                                _ => predicate.clone(),
                            })
                        }),
                    );
                }

                if row.path.0.starts_with(&resource.path_prefix) {
                    row.path.0.drain(..resource.path_prefix.len());
                }
                row
            })
            .collect()
    }
}

fn condition_predicate(condition: &GuardDnf) -> Predicate {
    Predicate::Or(
        condition
            .disjuncts()
            .iter()
            .map(|conjunction| Predicate::all(conjunction.iter().cloned().collect()))
            .collect(),
    )
}

fn scalar_placement(whole: bool, suppressed: bool, path: &YamlPath) -> (ValueKind, YamlPath) {
    if suppressed {
        (ValueKind::Serialized, YamlPath(Vec::new()))
    } else if whole {
        (ValueKind::Scalar, path.clone())
    } else {
        (ValueKind::PartialScalar, path.clone())
    }
}
