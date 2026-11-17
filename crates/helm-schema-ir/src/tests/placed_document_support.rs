//! Test access to placed documents' source mapping, which production never reads.

use helm_schema_syntax::Span;

use super::{NodeIx, PlacedDocument, PlacedTerm};
use crate::value::{DocumentExecution, Observation};

/// Every term rendered inside `node`, literal runs included.
pub(crate) fn terms<'a>(document: &PlacedDocument<'a>, node: NodeIx) -> Vec<PlacedTerm<'a>> {
    document
        .span(node)
        .map_or_else(Vec::new, |span| document.terms(span).collect())
}

/// The source text of a node that renders no hole.
pub(crate) fn literal_text<'a>(document: &PlacedDocument<'a>, node: NodeIx) -> Option<&'a str> {
    let span = document.span(node)?;
    if document.terms(span).any(|term| term.placeholder) {
        return None;
    }
    document.ownership.skeleton.text.get(span.start..span.end)
}

/// Every node whose source extent equals `node`'s, `node` included.
pub(crate) fn same_extent(document: &PlacedDocument<'_>, node: NodeIx) -> Vec<NodeIx> {
    let span = document.span(node);
    document
        .nodes()
        .filter(|other| span.is_some() && document.span(*other) == span)
        .collect()
}

/// The template source span that rendered `node`, from its terms' observations.
pub(crate) fn source_span(
    document: &PlacedDocument<'_>,
    node: NodeIx,
    execution: &DocumentExecution,
    source_path: Option<&str>,
) -> Option<Span> {
    let span = document.span(node)?;
    let mut result: Option<Span> = None;
    for term in document.terms(span) {
        let offset =
            (document.layout.source_mapped && term.node.indentation == 0 && !term.placeholder)
                .then(|| (term.range.start + document.document.offset).checked_sub(term.node.start))
                .flatten();
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
                        if let Some(offset) = offset
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
