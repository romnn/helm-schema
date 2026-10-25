//! YAML placement projected from frozen executions of the value runtime.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use helm_schema_ast::ResourceSpan;
use helm_schema_core::{GuardDnf, Predicate};
use helm_schema_syntax::{Node, ScalarPart, ScalarParts, Span, TemplatedDocument};

use crate::analysis_db::IrAnalysisDb;
use crate::observed_facts::ObservedFacts;
use crate::value::{DocumentExecution, ExecutedSite};
use crate::{ContractProvenance, ResourceRef, SourceSpan, ValueKind};

use super::domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, Mapping, MappingEntry, Sequence,
    SiteFacts, StringPart, TaintPart, stamp_fragment_sites,
};
use super::lower::{LowerScope, lower_scalar_dispatch, lower_scalar_dispatch_arms};
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

/// Execute once, then place the resulting observations in the YAML layout.
#[tracing::instrument(skip_all)]
pub(crate) fn eval_document(
    source: &str,
    source_path: Option<&str>,
    db: &IrAnalysisDb,
) -> EvaluatedDocument {
    let execution = crate::value::execute_document(source, source_path, db);
    project_execution(source, source_path, db, &execution)
}

pub(crate) fn project_execution(
    source: &str,
    source_path: Option<&str>,
    db: &IrAnalysisDb,
    execution: &DocumentExecution,
) -> EvaluatedDocument {
    let Some(tree) = helm_schema_syntax::parse_go_template(source) else {
        return EvaluatedDocument {
            reads: execution.reads(),
            observed_facts: execution.observed_facts().clone(),
            ..EvaluatedDocument::default()
        };
    };
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    let resources = crate::resource_identity::collect_resource_spans(
        &document,
        tree.root_node(),
        execution,
        source_path,
    );
    let projection = Projection {
        source,
        source_path,
        execution,
        resources,
        db,
    };
    EvaluatedDocument {
        root: projection.nodes(document.roots()),
        reads: execution.reads(),
        observed_facts: execution.observed_facts().clone(),
        pre_rewrite_strict_paths: BTreeSet::new(),
    }
}

struct Projection<'a> {
    source: &'a str,
    source_path: Option<&'a str>,
    execution: &'a DocumentExecution,
    resources: Vec<ResourceSpan>,
    db: &'a IrAnalysisDb,
}

impl Projection<'_> {
    fn nodes(&self, nodes: &[Node]) -> Guarded<AbstractFragment> {
        let mut mapping = Mapping::default();
        let mut sequence = Sequence::default();
        let mut result = Guarded::empty();
        for node in nodes {
            match node {
                Node::Mapping(entry) => {
                    let key = if entry
                        .key
                        .parts
                        .iter()
                        .all(|part| matches!(part, ScalarPart::Text(_)))
                    {
                        EntryKey::Literal(
                            helm_schema_syntax::parse_yaml_key(self.slice(entry.key.span))
                                .unwrap_or_else(|| self.slice(entry.key.span).to_owned()),
                        )
                    } else {
                        let parts =
                            self.scalar(&entry.key, false)
                                .arms
                                .into_iter()
                                .flat_map(|(_, node)| match node {
                                    AbstractFragment::Scalar(scalar) => scalar.parts,
                                    AbstractFragment::Splice(splice) => {
                                        vec![StringPart::Splice(splice)]
                                    }
                                    AbstractFragment::Opaque(opaque) => {
                                        vec![StringPart::Taint(TaintPart::abstaining(opaque.taint))]
                                    }
                                    AbstractFragment::Mapping(_)
                                    | AbstractFragment::Sequence(_) => Vec::new(),
                                })
                                .collect();
                        EntryKey::Dynamic(AbstractString {
                            parts,
                            suppressed: false,
                        })
                    };
                    let mut value = if let Some(block) = &entry.block {
                        self.span_scalar(block.body, true)
                    } else if let Some(scalar) = &entry.value {
                        self.scalar(scalar, false)
                    } else {
                        Guarded::empty()
                    };
                    value.extend(self.nodes(&entry.children));
                    value.guard_all(
                        &self
                            .execution
                            .condition_for_span(entry.span, self.source_path),
                        self.db.predicate_memo(),
                    );
                    if !value.is_empty() {
                        mapping.entries.push(MappingEntry { key, value });
                    }
                }
                Node::Sequence(item) => {
                    let mut value = if let Some(block) = &item.block {
                        self.span_scalar(block.body, true)
                    } else if let Some(scalar) = &item.value {
                        self.scalar(scalar, false)
                    } else {
                        Guarded::empty()
                    };
                    value.extend(self.nodes(&item.children));
                    value.guard_all(
                        &self
                            .execution
                            .condition_for_span(item.span, self.source_path),
                        self.db.predicate_memo(),
                    );
                    if !value.is_empty() {
                        sequence.items.push(value);
                    }
                }
                Node::Control(region) => {
                    for branch in &region.branches {
                        let branch = self.nodes(&branch.body);
                        for (condition, fragment) in branch.arms {
                            match fragment {
                                AbstractFragment::Mapping(mut children) => {
                                    for entry in &mut children.entries {
                                        entry.value.guard_all(&condition, self.db.predicate_memo());
                                    }
                                    mapping.entries.extend(children.entries);
                                }
                                AbstractFragment::Sequence(mut children) => {
                                    for item in &mut children.items {
                                        item.guard_all(&condition, self.db.predicate_memo());
                                    }
                                    sequence.items.extend(children.items);
                                }
                                other => result.arms.push((condition, other)),
                            }
                        }
                    }
                }
                Node::Output(output) => result.extend(self.hole(output.span, ValueKind::Fragment)),
                Node::Scalar(line) => result.extend(self.scalar(&line.content, false)),
                Node::Comment(_) => {}
                Node::Opaque(opaque) => match opaque.kind {
                    helm_schema_syntax::OpaqueKind::InlineRegion
                    | helm_schema_syntax::OpaqueKind::ActionLineText
                    | helm_schema_syntax::OpaqueKind::ParseError => {
                        result.extend(self.span_scalar(opaque.span, true))
                    }
                    helm_schema_syntax::OpaqueKind::Assignment
                    | helm_schema_syntax::OpaqueKind::Break
                    | helm_schema_syntax::OpaqueKind::Continue
                    | helm_schema_syntax::OpaqueKind::TemplateComment => {}
                },
            }
        }
        if !mapping.entries.is_empty() {
            result
                .arms
                .push((Predicate::True, AbstractFragment::Mapping(mapping)));
        }
        if !sequence.items.is_empty() {
            result
                .arms
                .push((Predicate::True, AbstractFragment::Sequence(sequence)));
        }
        result
    }

    fn scalar(&self, scalar: &ScalarParts, suppressed: bool) -> Guarded<AbstractFragment> {
        let holes = scalar
            .parts
            .iter()
            .filter_map(|part| match part {
                ScalarPart::Hole(span) => Some(*span),
                ScalarPart::Text(_) => None,
            })
            .collect::<Vec<_>>();
        if holes.len() == 1
            && scalar.parts.iter().all(|part| match part {
                ScalarPart::Text(span) => self.slice(*span).trim().is_empty(),
                ScalarPart::Hole(_) => true,
            })
            && !suppressed
        {
            return self.hole(holes[0], ValueKind::Scalar);
        }
        let mut arms = vec![(Predicate::True, Vec::new())];
        for part in &scalar.parts {
            let additions = match part {
                ScalarPart::Text(span) => vec![(
                    Predicate::True,
                    vec![StringPart::Text(BTreeSet::from([self
                        .slice(*span)
                        .to_owned()]))],
                )],
                ScalarPart::Hole(span) => self.hole_parts(*span),
            };
            if additions.is_empty() {
                continue;
            }
            let mut next = Vec::new();
            for (condition, parts) in &arms {
                for (other_condition, other_parts) in &additions {
                    let condition = self.db.predicate_memo().normalize(Predicate::all(vec![
                        condition.clone(),
                        other_condition.clone(),
                    ]));
                    if condition == Predicate::False {
                        continue;
                    }
                    let mut parts = parts.clone();
                    parts.extend(other_parts.iter().cloned());
                    next.push((condition, parts));
                }
            }
            arms = if next.len() > super::lower::MAX_SCALAR_ARM_FANOUT {
                super::lower::over_cap_scalar_taint(next)
            } else {
                next
            };
        }
        let mut result = Guarded {
            arms: arms
                .into_iter()
                .map(|(condition, parts)| {
                    (
                        condition,
                        AbstractFragment::Scalar(AbstractString { parts, suppressed }),
                    )
                })
                .collect(),
        };
        result.guard_all(
            &self
                .execution
                .condition_for_span(scalar.span, self.source_path),
            self.db.predicate_memo(),
        );
        stamp_fragment_sites(&mut result, self.site(scalar.span).as_ref());
        result
    }

    fn span_scalar(&self, span: Span, suppressed: bool) -> Guarded<AbstractFragment> {
        let mut parts = Vec::new();
        for site in self
            .execution
            .sites()
            .filter(|site| self.matches(site, span))
        {
            parts.extend(
                self.site_parts(site)
                    .into_iter()
                    .flat_map(|(_, parts)| parts),
            );
        }
        let mut result = Guarded {
            arms: vec![(
                self.execution.condition_for_span(span, self.source_path),
                AbstractFragment::Scalar(AbstractString { parts, suppressed }),
            )],
        };
        stamp_fragment_sites(&mut result, self.site(span).as_ref());
        result
    }

    fn hole(&self, span: Span, kind: ValueKind) -> Guarded<AbstractFragment> {
        let mut result = Guarded::empty();
        for site in self
            .execution
            .sites()
            .filter(|site| self.matches(site, span))
        {
            let mut fragment = site
                .rendered_scalar()
                .as_ref()
                .and_then(|scalar| {
                    self.with_scope(|scope| lower_scalar_dispatch(scalar, kind, scope))
                })
                .unwrap_or_else(|| {
                    Guarded::unconditional(AbstractFragment::Scalar(AbstractString {
                        parts: vec![StringPart::Taint(TaintPart::abstaining(
                            site.value().inputs(),
                        ))],
                        suppressed: false,
                    }))
                });
            fragment.guard_all(site.condition(), self.db.predicate_memo());
            stamp_fragment_sites(&mut fragment, self.site(span).as_ref());
            result.extend(fragment);
        }
        result
    }

    fn hole_parts(&self, span: Span) -> Vec<(Predicate, Vec<StringPart>)> {
        self.execution
            .sites()
            .filter(|site| self.matches(site, span))
            .flat_map(|site| self.site_parts(site))
            .collect()
    }

    fn site_parts(&self, site: &ExecutedSite) -> Vec<(Predicate, Vec<StringPart>)> {
        let parts = site
            .rendered_scalar()
            .as_ref()
            .and_then(|scalar| {
                self.with_scope(|scope| {
                    lower_scalar_dispatch_arms(scalar, ValueKind::PartialScalar, scope)
                })
            })
            .unwrap_or_else(|| {
                vec![(
                    Predicate::True,
                    vec![StringPart::Taint(TaintPart::abstaining(
                        site.value().inputs(),
                    ))],
                )]
            });
        parts
            .into_iter()
            .map(|(condition, parts)| {
                (
                    self.db
                        .predicate_memo()
                        .normalize(Predicate::all(vec![site.condition().clone(), condition])),
                    parts,
                )
            })
            .collect()
    }

    fn matches(&self, site: &ExecutedSite, span: Span) -> bool {
        site.rendered()
            && site.source_path() == self.source_path
            && span.start <= site.span().start
            && site.span().end <= span.end
    }

    fn slice(&self, span: Span) -> &str {
        self.source.get(span.start..span.end).unwrap_or_default()
    }

    fn site(&self, span: Span) -> Option<Rc<SiteFacts>> {
        let resource = self
            .resources
            .iter()
            .filter(|resource| resource.start <= span.start && span.start < resource.end)
            .min_by_key(|resource| resource.end.saturating_sub(resource.start));
        let provenance = self.source_path.map(|path| {
            ContractProvenance::new(path, SourceSpan::new(span.start, span.end), Vec::new())
        });
        if resource.is_none() && provenance.is_none() {
            return None;
        }
        Some(Rc::new(SiteFacts {
            resource: resource.map(|resource| resource.resource.clone()),
            path_prefix: resource.map_or_else(Vec::new, |resource| resource.path_prefix.clone()),
            provenance,
        }))
    }

    fn with_scope<T>(&self, project: impl FnOnce(&LowerScope<'_>) -> T) -> T {
        let paths = BTreeSet::new();
        let metadata = BTreeMap::new();
        project(&LowerScope {
            defaulted_paths: &paths,
            encoded_paths: &paths,
            derived_text_paths: &paths,
            merge_operand_paths: &paths,
            yaml_serialized_paths: &paths,
            templated_yaml_paths: &paths,
            shape_erased_paths: &paths,
            stringified_paths: &paths,
            nil_omitting_paths: &paths,
            plain_slot_string_format_paths: &paths,
            json_serialized_paths: &paths,
            chart_value_defaults: &paths,
            local_source_paths: &paths,
            local_output_meta: &metadata,
        })
    }
}
