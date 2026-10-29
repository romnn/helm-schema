use helm_schema_ast::{KindBranchSource, ResourceSpan, TemplateExpr, TemplateHeader};
use helm_schema_core::{Guard, HelperBranchBody, PredicateKind, ValuesPath};
use helm_schema_syntax::TemplatedDocument;

use super::*;
use crate::value::Observation;
use std::collections::BTreeSet;

struct ResourceCase {
    resource: ResourceRef,
    condition: Predicate,
}
struct ResourceGroup {
    source: Span,
    path_prefix: Vec<String>,
    cases: Vec<ResourceCase>,
    kind_emissions: BTreeSet<Span>,
}

pub(crate) fn collect_resource_spans(
    source: &TemplatedDocument<'_>,
    root: tree_sitter::Node<'_>,
    execution: &DocumentExecution,
    source_path: Option<&str>,
) -> Vec<ResourceSpan> {
    let mut groups: Vec<ResourceGroup> = Vec::new();
    let mut unresolved = Vec::new();
    for document in execution.placed_documents() {
        let projection = ResourceProjection::new(&document);
        for index in projection.roots() {
            let headers = projection.headers(index);
            if let Some(kind) = headers.kind
                && projection.names(kind).is_none()
                && let Some(mapping) = document.ownership.nodes.get(index)
                && let Some(span) = source_span(&document, mapping.span, execution, source_path)
            {
                unresolved.push(span);
            }
        }
        for PlacedResource {
            span,
            condition,
            resource,
            path_prefix,
        } in placed_resources(&document, execution)
        {
            let kind_emissions = document
                .ownership
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| node.span == span)
                .filter_map(|(index, _)| projection.headers(index).kind)
                .filter_map(|index| document.ownership.nodes.get(index))
                .flat_map(|node| document.terms(node.span))
                .filter(|term| term.frozen_alternatives().is_some())
                .flat_map(|term| term.occurrences().map(|id| id.span).collect::<Vec<_>>())
                .collect::<BTreeSet<_>>();
            let Some(span) = source_span(&document, span, execution, source_path) else {
                continue;
            };
            let case = ResourceCase {
                resource,
                condition,
            };
            if let Some(group) = groups
                .iter_mut()
                .find(|group| overlaps(group.source, span) && group.path_prefix == path_prefix)
            {
                group.source = Span::new(
                    group.source.start.min(span.start),
                    group.source.end.max(span.end),
                );
                group.cases.push(case);
                group.kind_emissions.extend(kind_emissions);
            } else {
                groups.push(ResourceGroup {
                    source: span,
                    path_prefix,
                    cases: vec![case],
                    kind_emissions,
                });
            }
        }
    }
    let mut spans = groups
        .into_iter()
        .filter(|group| {
            !unresolved
                .iter()
                .any(|unknown| overlaps(*unknown, group.source))
        })
        .filter_map(|group| finish_group(group, source.source(), root, execution, source_path))
        .collect::<Vec<_>>();
    spans.sort_by_key(|span| (span.start, span.end));
    spans
}

fn finish_group(
    group: ResourceGroup,
    source: &str,
    root: tree_sitter::Node<'_>,
    execution: &DocumentExecution,
    source_path: Option<&str>,
) -> Option<ResourceSpan> {
    let ResourceGroup {
        source: span,
        path_prefix,
        cases,
        kind_emissions,
    } = group;
    let first = cases.first()?;
    let mut resource = first.resource.clone();
    for ResourceCase {
        resource: candidate,
        condition: _,
    } in &cases
    {
        if candidate.kind != resource.kind && !resource.kind_candidates.contains(&candidate.kind) {
            resource.kind_candidates.push(candidate.kind.clone());
        }
        if candidate.api_version != resource.api_version
            && !resource
                .api_version_candidates
                .contains(&candidate.api_version)
        {
            resource
                .api_version_candidates
                .push(candidate.api_version.clone());
        }
    }
    let kind_branch_sources = branch_sources(&cases, span, source, root, execution, source_path);
    resource.kind_selector = selector(&cases).filter(|candidate| {
        kind_branch_sources.iter().any(|branch| {
            let mut represented = false;
            if let Some(condition) = &branch.condition {
                condition.walk(|expression| {
                    if let TemplateExpr::Field(path) = expression
                        && let Some((head, tail)) = path.split_first()
                        && head == "Values"
                        && ValuesPath::from_segments(tail.iter().cloned()) == *candidate
                    {
                        represented = true;
                    }
                });
            }
            represented
        })
    });
    // Flat candidates cannot retain guarded writes whose branch provenance is outside
    // this resource's source window. Production keeps the indexed guarded resources.
    if kind_emissions.len() == 1
        && !resource.kind_candidates.is_empty()
        && resource.kind_selector.is_none()
        && kind_branch_sources.is_empty()
    {
        return None;
    }
    Some(ResourceSpan {
        start: span.start,
        end: span.end,
        resource,
        path_prefix,
        kind_branch_sources,
    })
}

fn overlaps(left: Span, right: Span) -> bool {
    left.start < right.end && right.start < left.end
}

fn selector(cases: &[ResourceCase]) -> Option<ValuesPath> {
    let memo = PredicateMemo::new();
    let candidate = cases
        .iter()
        .find_map(|case| selector_path(&case.condition, &case.resource.kind, &memo))?;
    cases
        .iter()
        .all(|case| mentions_comparison(&case.condition, &candidate))
        .then_some(candidate)
}

fn mentions_comparison(predicate: &Predicate, path: &ValuesPath) -> bool {
    match predicate.kind() {
        PredicateKind::Guard(
            Guard::Eq {
                path: actual,
                value: _,
            }
            | Guard::NotEq {
                path: actual,
                value: _,
            },
        ) => actual == path,
        PredicateKind::And(parts) => parts.iter().any(|part| mentions_comparison(part, path)),
        PredicateKind::Not(inner) => mentions_comparison(inner, path),
        PredicateKind::True
        | PredicateKind::False
        | PredicateKind::Guard(_)
        | PredicateKind::Or(_)
        | PredicateKind::Approximate {
            marker: _,
            paths: _,
            role: _,
            sound_subset: _,
        } => false,
    }
}

fn branch_sources(
    cases: &[ResourceCase],
    span: Span,
    source: &str,
    root: tree_sitter::Node<'_>,
    execution: &DocumentExecution,
    source_path: Option<&str>,
) -> Vec<KindBranchSource> {
    let kinds = cases
        .iter()
        .map(|case| case.resource.kind.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if kinds.len() < 2 {
        return Vec::new();
    }
    let memo = PredicateMemo::new();
    let covered = Predicate::Or(cases.iter().map(|case| case.condition.clone()).collect());
    let mut headers = execution
        .sites()
        .filter(|site| {
            !site.rendered()
                && site.source_path() == source_path
                && span.start <= site.span().start
                && site.span().end <= span.end
        })
        .collect::<Vec<_>>();
    headers.sort_by_key(|site| std::cmp::Reverse(site.span().start));
    for site in headers {
        let Some(when_true) = execution.observed_site_truth(site) else {
            continue;
        };
        if !memo.exactly_implies(&site.condition(), &covered) {
            continue;
        }
        let Some(provenance) = condition_provenance(root, source, site.span()) else {
            continue;
        };
        let mut sources = Vec::new();
        let mut complete = true;
        for case in cases {
            let condition = if memo.exactly_implies(&case.condition, &when_true) {
                Some(provenance.clone())
            } else if memo.exactly_implies(&case.condition, &when_true.negated()) {
                None
            } else {
                complete = false;
                break;
            };
            let candidate = KindBranchSource {
                condition,
                kind: case.resource.kind.clone(),
            };
            if !sources.contains(&candidate) {
                sources.push(candidate);
            }
        }
        // A constant observed condition cannot distinguish the emitted kind alternatives.
        if complete
            && sources.iter().any(|branch| branch.condition.is_some())
            && sources.iter().any(|branch| branch.condition.is_none())
        {
            return sources;
        }
    }
    Vec::new()
}

// Syntax is copied only from a header whose Boolean value was actually observed.
fn condition_provenance(
    root: tree_sitter::Node<'_>,
    source: &str,
    span: Span,
) -> Option<TemplateExpr> {
    let mut node = root.descendant_for_byte_range(span.start, span.end);
    while let Some(current) = node {
        let mut cursor = current.walk();
        if let Some(condition) = current
            .children_by_field_name("condition", &mut cursor)
            .find(|condition| {
                span.start <= condition.start_byte() && condition.end_byte() <= span.end
            })
        {
            return Some(TemplateHeader::from_node(condition, source).expr().clone());
        }
        node = current.parent();
    }
    None
}

pub(crate) fn executed_scalar_body(
    execution: &DocumentExecution,
    span: Span,
    source_path: Option<&str>,
) -> HelperBranchBody {
    let mut names = Vec::new();
    for document in execution.placed_documents() {
        for node in &document.ownership.nodes {
            if !matches!(node.kind, YamlNodeKind::Scalar(_)) {
                continue;
            }
            let Some(source) = source_span(&document, node.span, execution, source_path) else {
                continue;
            };
            if source.start < span.start || span.end < source.end {
                continue;
            }
            let Some(arms) = scalar_names(&document, node.span, &PredicateMemo::new()) else {
                return HelperBranchBody::literals(Vec::new());
            };
            for NamedValue { condition: _, text } in arms {
                if !text.is_empty() && !names.contains(&text) {
                    names.push(text);
                }
            }
        }
    }
    HelperBranchBody::literals(names)
}
fn source_span(
    document: &PlacedDocument<'_>,
    span: Span,
    execution: &DocumentExecution,
    source_path: Option<&str>,
) -> Option<Span> {
    let mut result: Option<Span> = None;
    for term in document.terms(span) {
        let mut context = term.context().peekable();
        let ids = if context.peek().is_some() {
            context.collect::<Vec<_>>()
        } else {
            term.occurrences().collect()
        };
        for id in ids {
            for observation in execution.observations(id) {
                let source = match observation {
                    Observation::Text(site) if site.source_path() == source_path => {
                        let source = site.span();
                        if let Some(offset) = term.source_offset
                            && source.end.saturating_sub(source.start) == site.bytes().len()
                        {
                            Span::new(
                                source.start
                                    + offset
                                    + span
                                        .start
                                        .max(term.range.start)
                                        .saturating_sub(term.range.start),
                                source.start
                                    + offset
                                    + span
                                        .end
                                        .min(term.range.end)
                                        .saturating_sub(term.range.start),
                            )
                        } else {
                            source
                        }
                    }
                    Observation::Value(site) if site.source_path() == source_path => site.span(),
                    Observation::Text(_) | Observation::Value(_) => continue,
                };
                result = Some(result.map_or(source, |range| {
                    Span::new(range.start.min(source.start), range.end.max(source.end))
                }));
            }
        }
    }
    result
}
