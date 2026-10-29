//! Fragment projection from the file's sealed executed-output ownership.

use std::collections::BTreeSet;
use std::rc::Rc;

use helm_schema_core::{GuardDnf, Predicate};
use helm_schema_syntax::{ScalarStyle, Span, YamlNode, YamlNodeKind};

use crate::analysis_db::IrAnalysisDb;
use crate::observed_facts::ObservedFacts;
use crate::resource_identity::PlacedResource;
use crate::value::yaml::{PlacedDocument, PlacedTerm};
use crate::value::{DocumentExecution, ExecutedSite, RenderedContainerUse};
use crate::value::{Observation, ObservationId};
use crate::{ContractProvenance, ResourceRef, SourceSpan, ValueKind};

use super::domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, Mapping, MappingEntry, Sequence,
    SiteFacts, Splice, SpliceMeta, StringPart, TaintPart, stamp_fragment_sites,
};
use super::lower::{lower_executed_scalar, lower_executed_scalar_arms};
/// The result of evaluating one template source: the abstract rendered
/// document plus the pathless value reads observed along the way.
#[derive(Debug, Default)]
pub(crate) struct EvaluatedDocument {
    /// The guarded abstract fragment for the whole source (all YAML
    /// documents merged; per-document projection is a later-stage concern).
    pub root: Guarded<AbstractFragment>,
    /// `.Values` reads that never render: condition reads, assignment
    /// right-hand sides, helper-internal guard reads, and range headers.
    pub reads: Vec<ValueRead>,
    pub(crate) observed_facts: ObservedFacts,
    /// Strictly string-consumed paths whose consumers execute BEFORE the
    /// values-root wrapper rewrite: their nodes must not gain the wrapper
    /// alternative (see the interpreter field of the same name).
    pub(crate) pre_rewrite_strict_paths: BTreeSet<helm_schema_core::ValuesPath>,
}

/// One pathless `.Values` read with the guards active at the read site.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ValueRead {
    /// The dotted `.Values` path that was read.
    pub values_path: helm_schema_core::ValuesPath,
    /// The value shape observed at the read (helper rows demoted at capture
    /// sites keep their fragment/scalar kind).
    pub kind: crate::ValueKind,
    /// The disjunction of predicate conjunctions under which the read occurs.
    pub condition: GuardDnf,
    /// The resource containing the read site, for site-scoped read classes
    /// (condition, bound-value, templated-key, and rendered-effect reads);
    /// helper-internal reads carry none.
    pub resource: Option<ResourceRef>,
    /// Source sites justifying the read.
    pub provenance: Vec<ContractProvenance>,
    /// Whether the read belongs to the dependency lane (helper rows demoted
    /// at capture sites) instead of the document lane.
    pub dependency: bool,
}

#[tracing::instrument(skip_all, level = "debug", fields(source_path))]
pub(crate) fn project_execution_at_offset(
    _source: &str,
    source_path: Option<&str>,
    source_offset: usize,
    db: &IrAnalysisDb,
    execution: &DocumentExecution,
) -> EvaluatedDocument {
    let mut root = Guarded::empty();
    let mut observed_facts = execution.observed_facts().clone();
    for document in execution.placed_documents() {
        let resources = crate::resource_identity::placed_resources(&document, execution);
        observed_facts
            .resources
            .extend(
                resources
                    .iter()
                    .map(|resource| crate::observed_facts::ObservedResource {
                        condition: resource.condition.clone(),
                        resource: resource.resource.clone(),
                    }),
            );
        let mut children = vec![Vec::new(); document.ownership.nodes.len()];
        let mut roots = Vec::new();
        for (index, node) in document.ownership.nodes.iter().enumerate() {
            if let Some(parent) = node.parent.and_then(|parent| children.get_mut(parent)) {
                parent.push(index);
            } else {
                roots.push(index);
            }
        }
        let projection = Projection {
            document,
            execution,
            source_path,
            source_offset,
            db,
            resources,
            children,
        };
        for index in roots {
            root.extend(projection.node(index));
        }
    }
    EvaluatedDocument {
        root,
        reads: execution.projection_reads(),
        observed_facts,
        pre_rewrite_strict_paths: BTreeSet::new(),
    }
}

struct Projection<'a> {
    document: PlacedDocument<'a>,
    execution: &'a DocumentExecution,
    source_path: Option<&'a str>,
    source_offset: usize,
    db: &'a IrAnalysisDb,
    resources: Vec<PlacedResource>,
    children: Vec<Vec<usize>>,
}

struct KeyArm {
    condition: Predicate,
    key: EntryKey,
}

impl Projection<'_> {
    fn node(&self, index: usize) -> Guarded<AbstractFragment> {
        let Some(node) = self.document.ownership.nodes.get(index) else {
            return Guarded::empty();
        };
        let YamlNode {
            parent: _,
            kind,
            key: _,
            span,
            pieces: _,
        } = *node;
        let mut result = match kind {
            YamlNodeKind::Document | YamlNodeKind::Item => {
                let mut result = Guarded::empty();
                for child in self.children.get(index).into_iter().flatten() {
                    result.extend(self.node(*child));
                }
                result
            }
            YamlNodeKind::Mapping | YamlNodeKind::FlowMapping => self.mapping(index),
            YamlNodeKind::Sequence | YamlNodeKind::FlowSequence => {
                let items = self
                    .children
                    .get(index)
                    .into_iter()
                    .flatten()
                    .map(|child| self.node(*child))
                    .filter(|item| !item.is_empty())
                    .collect();
                Guarded::unconditional(AbstractFragment::Sequence(Sequence { items }))
            }
            YamlNodeKind::Scalar(style) => self.scalar(span, style, false),
            YamlNodeKind::BlockScalar => self.scalar(span, ScalarStyle::Plain, true),
            YamlNodeKind::Pair
            | YamlNodeKind::FlowPair
            | YamlNodeKind::Comment
            | YamlNodeKind::DocumentEnd => Guarded::empty(),
        };
        result.guard_all(&self.document.condition, self.db.predicate_memo());
        result
    }

    fn mapping(&self, index: usize) -> Guarded<AbstractFragment> {
        let mut mapping = Mapping::default();
        for pair in self.children.get(index).into_iter().flatten() {
            let Some(pair_node) = self.document.ownership.nodes.get(*pair) else {
                continue;
            };
            if !matches!(pair_node.kind, YamlNodeKind::Pair | YamlNodeKind::FlowPair) {
                continue;
            }
            let children = self.children.get(*pair).into_iter().flatten();
            let Some(key) = children
                .clone()
                .find(|child| {
                    self.document
                        .ownership
                        .nodes
                        .get(**child)
                        .is_some_and(|node| node.key)
                })
                .copied()
            else {
                continue;
            };
            let mut value = Guarded::empty();
            for child in children.filter(|child| **child != key) {
                value.extend(self.node(*child));
            }
            for KeyArm { condition, key } in self.keys(key) {
                let mut value = value.clone();
                value.guard_all(&condition, self.db.predicate_memo());
                if !value.is_empty() {
                    mapping.entries.push(MappingEntry { key, value });
                }
            }
        }
        if mapping.entries.is_empty() {
            Guarded::empty()
        } else {
            Guarded::unconditional(AbstractFragment::Mapping(mapping))
        }
    }

    fn keys(&self, index: usize) -> Vec<KeyArm> {
        let Some(node) = self.document.ownership.nodes.get(index) else {
            return Vec::new();
        };
        if self.document.terms(node.span).all(|term| !term.placeholder) {
            return self
                .document
                .ownership
                .skeleton
                .text
                .get(node.span.start..node.span.end)
                .and_then(|text| helm_schema_syntax::parse_yaml_key(&format!("{text}:")))
                .map(|key| {
                    vec![KeyArm {
                        condition: self.document.placement_guard(node.span),
                        key: EntryKey::Literal(key),
                    }]
                })
                .unwrap_or_default();
        }
        let mut keys = Vec::new();
        for (condition, fragment) in self.scalar(node.span, ScalarStyle::Plain, false).arms {
            let parts = match fragment {
                AbstractFragment::Scalar(scalar) => scalar.parts,
                AbstractFragment::Splice(splice) => vec![StringPart::Splice(splice)],
                AbstractFragment::Mapping(_) | AbstractFragment::Sequence(_) => Vec::new(),
            };
            keys.push(KeyArm {
                condition,
                key: EntryKey::Dynamic(AbstractString {
                    parts,
                    suppressed: false,
                }),
            });
        }
        keys
    }

    fn scalar(
        &self,
        span: Span,
        style: ScalarStyle,
        suppressed: bool,
    ) -> Guarded<AbstractFragment> {
        let terms = self.document.terms(span).collect::<Vec<_>>();
        let symbolic = terms.iter().filter(|term| term.placeholder).count();
        let mut result = Guarded::empty();
        for term in &terms {
            if !term.placeholder {
                continue;
            }
            let context = term.context().collect::<BTreeSet<_>>();
            let visible = if context.is_empty() {
                term.occurrences()
                    .any(|id| self.execution.is_root_observation(id))
            } else {
                context
                    .iter()
                    .any(|id| self.execution.is_root_observation(*id))
            };
            if !visible {
                continue;
            }
            let whole = symbolic == 1
                && term.range == span
                && style == ScalarStyle::Plain
                && !suppressed
                && self.document.block_value(span);
            let kind = if whole {
                ValueKind::Scalar
            } else {
                ValueKind::PartialScalar
            };
            let mut found = false;
            let mut occurrences = BTreeSet::new();
            for id in term.occurrences().filter(|id| !context.contains(id)) {
                if !occurrences.insert(id) {
                    continue;
                }
                for observation in self.execution.observations(id) {
                    let site = match observation {
                        Observation::Text(_) => continue,
                        Observation::Value(site) => site,
                    };
                    if !site.rendered() {
                        continue;
                    }
                    let Some(guard) = term.occurrence_guard(id) else {
                        continue;
                    };
                    found = true;
                    let mut fragment =
                        self.emission(site, kind, suppressed, &terms, span, term.range);
                    fragment.guard_all(&guard, self.db.predicate_memo());
                    self.stamp(&mut fragment, term, span, Some(id));
                    result.extend(fragment);
                }
            }
            if !found {
                let mut fragment =
                    Guarded::unconditional(AbstractFragment::Scalar(AbstractString {
                        parts: vec![StringPart::Taint(TaintPart::abstaining(term.inputs()))],
                        suppressed,
                    }));
                self.stamp(&mut fragment, term, span, None);
                result.extend(fragment);
            }
        }
        if symbolic == 0
            && let Some(text) = self
                .document
                .ownership
                .skeleton
                .text
                .get(span.start..span.end)
        {
            result.arms.push((
                Predicate::True,
                AbstractFragment::Scalar(AbstractString {
                    parts: vec![StringPart::Text(BTreeSet::from([text.to_owned()]))],
                    suppressed,
                }),
            ));
        }
        result.guard_all(
            &self.document.placement_guard(span),
            self.db.predicate_memo(),
        );
        result
    }

    fn emission(
        &self,
        site: &ExecutedSite,
        kind: ValueKind,
        suppressed: bool,
        terms: &[PlacedTerm<'_>],
        span: Span,
        emission: Span,
    ) -> Guarded<AbstractFragment> {
        let mut fragment = if kind == ValueKind::Scalar {
            site.scalar_view()
                .as_ref()
                .and_then(|scalar| lower_executed_scalar(scalar, kind))
                .unwrap_or_else(|| {
                    Guarded::unconditional(AbstractFragment::Scalar(AbstractString {
                        parts: vec![StringPart::Taint(TaintPart::abstaining(
                            site.value().inputs(),
                        ))],
                        suppressed,
                    }))
                })
        } else {
            let parts = site
                .scalar_view()
                .as_ref()
                .and_then(|scalar| lower_executed_scalar_arms(scalar, ValueKind::PartialScalar))
                .unwrap_or_else(|| {
                    vec![(
                        Predicate::True,
                        vec![StringPart::Taint(TaintPart::abstaining(
                            site.value().inputs(),
                        ))],
                    )]
                });
            Guarded {
                arms: parts
                    .into_iter()
                    .map(|(condition, parts)| {
                        let mut before = Vec::new();
                        let mut after = Vec::new();
                        for literal in terms.iter().filter(|term| !term.placeholder) {
                            let start = literal.range.start.max(span.start);
                            let end = literal.range.end.min(span.end);
                            if let Some(text) =
                                self.document.ownership.skeleton.text.get(start..end)
                            {
                                let literal = StringPart::Text(BTreeSet::from([text.to_owned()]));
                                if end <= emission.start {
                                    before.push(literal);
                                } else {
                                    after.push(literal);
                                }
                            }
                        }
                        before.extend(parts);
                        before.extend(after);
                        (
                            condition,
                            AbstractFragment::Scalar(AbstractString {
                                parts: before,
                                suppressed,
                            }),
                        )
                    })
                    .collect(),
            }
        };
        if kind == ValueKind::Scalar {
            for RenderedContainerUse {
                condition,
                path,
                kind,
            } in site.formatted_containers()
            {
                fragment.arms.push((
                    condition,
                    AbstractFragment::Splice(Splice {
                        values_path: path,
                        kind,
                        meta: SpliceMeta::default(),
                    }),
                ));
            }
            super::domain::stamp_plain_scalar_placement(
                &mut fragment,
                site.plain_scalar_placement(),
            );
        }
        fragment.guard_all(&site.condition(), self.db.predicate_memo());
        fragment
    }

    fn stamp(
        &self,
        fragment: &mut Guarded<AbstractFragment>,
        term: &PlacedTerm<'_>,
        span: Span,
        occurrence: Option<ObservationId>,
    ) {
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
        let resources = self
            .resources
            .iter()
            .filter(|resource| resource.span.start <= span.start && span.end <= resource.span.end)
            .collect::<Vec<_>>();
        if resources.is_empty() {
            let facts = Some(Rc::new(SiteFacts {
                resource: None,
                path_prefix: Vec::new(),
                provenance,
            }));
            stamp_fragment_sites(fragment, facts.as_ref());
            return;
        }
        let mut output = Guarded::empty();
        let smallest = resources
            .iter()
            .map(|resource| resource.span.end.saturating_sub(resource.span.start))
            .min();
        for resource in resources.into_iter().filter(|resource| {
            Some(resource.span.end.saturating_sub(resource.span.start)) == smallest
        }) {
            let mut contribution = fragment.clone();
            contribution.guard_all(&resource.condition, self.db.predicate_memo());
            let facts = Some(Rc::new(SiteFacts {
                resource: Some(resource.resource.clone()),
                path_prefix: resource.path_prefix.clone(),
                provenance: provenance.clone(),
            }));
            stamp_fragment_sites(&mut contribution, facts.as_ref());
            output.extend(contribution);
        }
        *fragment = output;
    }
}
