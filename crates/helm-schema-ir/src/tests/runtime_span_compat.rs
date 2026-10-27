//! Test-only source-span observation queries for execution-law fixtures.

use crate::value::{DocumentExecution, Flow};
use helm_schema_core::Predicate;

struct SpanTextPiece {
    ordinal: usize,
    condition: Predicate,
    text: Option<String>,
}

struct SpanTextAlternative {
    condition: Predicate,
    text: Option<String>,
}

fn partition_span_text(
    condition: Predicate,
    mut pieces: Vec<SpanTextPiece>,
    memo: &helm_schema_core::PredicateMemo,
) -> Option<Vec<SpanTextAlternative>> {
    pieces.sort_by_key(|piece| piece.ordinal);
    let mut alternatives = vec![SpanTextAlternative {
        condition,
        text: Some(String::new()),
    }];
    for SpanTextPiece {
        ordinal: _,
        condition: guard,
        text: piece,
    } in pieces
    {
        let mut next = Vec::new();
        for SpanTextAlternative { condition, text } in alternatives {
            let included = memo.normalize(Predicate::all(vec![condition.clone(), guard.clone()]));
            let excluded = memo.normalize(Predicate::all(vec![condition, guard.clone().negated()]));
            if !memo.exactly_implies(&included, &Predicate::False) {
                next.push(SpanTextAlternative {
                    condition: included,
                    text: text.clone().zip(piece.as_ref()).map(|(mut text, piece)| {
                        text.push_str(piece);
                        text
                    }),
                });
            }
            if !memo.exactly_implies(&excluded, &Predicate::False) {
                next.push(SpanTextAlternative {
                    condition: excluded,
                    text,
                });
            }
            if next.len() > super::MAX_SCALAR_DISPATCH_STATES {
                return None;
            }
        }
        alternatives = next;
    }
    Some(alternatives)
}

impl DocumentExecution {
    pub(crate) fn text_for_span<'a>(
        &'a self,
        span: helm_schema_syntax::Span,
        source_path: Option<&'a str>,
    ) -> impl Iterator<Item = (Predicate, Option<String>)> + 'a {
        self.exits
            .iter()
            .filter(|exit| matches!(exit.flow, Flow::Normal(_)))
            .flat_map(move |exit| {
                let condition = exit.state.path_predicate();
                let fallback = || vec![(condition.clone(), None)];
                if condition.contains_approximation()
                    || exit.state.observation_count.overflow_under(
                        &exit.state.heap.arena.runtime_path(&exit.state.active),
                        &exit.state.heap.arena,
                    )
                    || exit
                        .state
                        .active
                        .iter()
                        .any(|edge| edge.test.coverage == super::Coverage::Partial)
                {
                    return fallback();
                }
                let memo = helm_schema_core::PredicateMemo::new();
                let mut pieces = Vec::new();
                for site in &exit.state.text_sites {
                    if !self.root_programs.contains(&site.program)
                        || site.source_path.as_deref() != source_path
                        || site.span.start >= span.end
                        || span.start >= site.span.end
                        || memo.exactly_implies(&condition, &site.condition.negated())
                    {
                        continue;
                    }
                    let start = span
                        .start
                        .saturating_sub(site.span.start)
                        .min(site.bytes.len());
                    let end = span
                        .end
                        .saturating_sub(site.span.start)
                        .min(site.bytes.len());
                    if site.condition.contains_approximation() {
                        return fallback();
                    }
                    pieces.push(SpanTextPiece {
                        ordinal: site.ordinal,
                        condition: site.condition.clone(),
                        text: (site.coverage_exact && site.rendered_unchanged)
                            .then(|| site.bytes.get(start..end).map(str::to_owned))
                            .flatten(),
                    });
                    if pieces.len() > super::MAX_SCALAR_DISPATCH_STATES {
                        return fallback();
                    }
                }
                for site in &exit.state.sites {
                    if !self.root_programs.contains(&site.program)
                        || !site.rendered
                        || site.source_path.as_deref() != source_path
                        || site.span.start >= span.end
                        || span.start >= site.span.end
                        || memo.exactly_implies(&condition, &site.condition.negated())
                    {
                        continue;
                    }
                    if site.condition.contains_approximation() {
                        return fallback();
                    }
                    pieces.push(SpanTextPiece {
                        ordinal: site.ordinal,
                        condition: site.condition.clone(),
                        text: (site.coverage_exact && site.rendered_unchanged)
                            .then(|| site.value.exact_text())
                            .flatten(),
                    });
                    if pieces.len() > super::MAX_SCALAR_DISPATCH_STATES {
                        return fallback();
                    }
                }
                let Some(alternatives) = partition_span_text(condition.clone(), pieces, &memo)
                else {
                    return fallback();
                };
                alternatives
                    .into_iter()
                    .map(|SpanTextAlternative { condition, text }| (condition, text))
                    .collect()
            })
    }
    pub(crate) fn condition_for_span(
        &self,
        span: helm_schema_syntax::Span,
        source_path: Option<&str>,
    ) -> Predicate {
        let text = self
            .exits
            .iter()
            .filter(|exit| matches!(exit.flow, Flow::Normal(_)))
            .flat_map(|exit| &exit.state.text_sites)
            .filter(|site| {
                self.root_programs.contains(&site.program)
                    && site.source_path.as_deref() == source_path
                    && site.span.start < span.end
                    && span.start < site.span.end
            })
            .map(|site| site.condition.clone());
        let actions = self
            .sites()
            .filter(|site| {
                self.root_programs.contains(&site.program)
                    && site.source_path() == source_path
                    && site.span.start < span.end
                    && span.start < site.span.end
            })
            .map(|site| site.condition.clone());
        Predicate::Or(text.chain(actions).collect()).normalize_boolean()
    }
}
