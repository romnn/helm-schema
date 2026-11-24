//! YAML boundaries before an unbounded range continuation.

use helm_schema_syntax::{ScalarStyle, SkeletonSegment, Span, YamlNodeKind, YamlOwnership};

use super::{DocumentLayout, LayoutBuilder, LayoutContext, ManifestChunks};
use crate::value::{RepeatedOutput, TextPart, text_dag::TextParts};

fn include_prefix(common: &mut Option<String>, literal: String) {
    let Some(previous) = common else {
        *common = Some(literal);
        return;
    };
    let mut shared = previous
        .bytes()
        .zip(literal.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while !previous.is_char_boundary(shared) {
        shared -= 1;
    }
    previous.truncate(shared);
}

enum RepeatPrefix {
    Empty,
    /// An empty string means an iteration can emit before producing known bytes.
    Literal(String),
}

fn repeat_prefix(repeat: &RepeatedOutput, budget: &mut usize) -> Option<RepeatPrefix> {
    let RepeatedOutput {
        bodies,
        breaks,
        inputs: _,
    } = repeat;
    let mut common = None;
    for body in bodies.iter().chain(breaks) {
        first_emission(
            vec![(&body.parts).into_iter()],
            String::new(),
            &mut common,
            budget,
        )?;
        if common.as_deref() == Some("") {
            break;
        }
    }
    Some(common.map_or(RepeatPrefix::Empty, RepeatPrefix::Literal))
}

fn first_emission(
    mut pending: Vec<<&TextParts as IntoIterator>::IntoIter>,
    mut literal: String,
    common: &mut Option<String>,
    budget: &mut usize,
) -> Option<()> {
    while let Some(parts) = pending.last_mut() {
        let Some(part) = parts.next() else {
            pending.pop();
            continue;
        };
        *budget = budget.checked_sub(1)?;
        match part.content() {
            TextPart::Bytes(text) => {
                if literal.len().saturating_add(text.len()) > 64 * 1024 {
                    return None;
                }
                literal.push_str(text);
            }
            TextPart::Select {
                decision: _,
                yes,
                no,
            } => {
                let mut alternative = pending.clone();
                alternative.push((&yes.parts).into_iter());
                first_emission(alternative, literal.clone(), common, budget)?;
                if common.as_deref() == Some("") {
                    return Some(());
                }
                pending.push((&no.parts).into_iter());
            }
            TextPart::Repeat(repeat)
                if matches!(repeat_prefix(repeat, budget)?, RepeatPrefix::Empty) => {}
            TextPart::Frozen(value) if value.exact_text().is_some_and(|text| text.is_empty()) => {}
            TextPart::Frozen(_) | TextPart::Unknown(_) | TextPart::Repeat(_) => {
                include_prefix(common, literal);
                return Some(());
            }
        }
    }
    if !literal.is_empty() {
        include_prefix(common, literal);
    }
    Some(())
}

impl LayoutBuilder<'_> {
    pub(super) fn place_repetitions(&mut self, context: &mut LayoutContext) {
        for piece in 0..context.terms.len() {
            let Some(reference) = context.terms.get(piece) else {
                continue;
            };
            let Some(TextPart::Repeat(repeat)) =
                reference.node().map(crate::value::OutputNode::content)
            else {
                continue;
            };
            let Some(prefix) = repeat_prefix(repeat, &mut self.remaining) else {
                continue;
            };
            if let RepeatPrefix::Literal(prefix) = &prefix
                && self
                    .repetition_closes_scalar(context, piece, prefix)
                    .is_none()
            {
                continue;
            }
            // Seal the zero-repetition layout with the real suffix. A nonempty
            // continuation stays opaque at its zero-width position.
            if context.replace_spelling(piece, "", None) && matches!(prefix, RepeatPrefix::Empty) {
                context
                    .skeleton
                    .segments
                    .retain(|segment| segment.piece != piece);
            }
        }
    }

    fn repetition_closes_scalar(
        &mut self,
        context: &LayoutContext,
        piece: usize,
        prefix: &str,
    ) -> Option<()> {
        let reference = context.terms.get(piece)?;
        let tail_index = context
            .skeleton
            .segments
            .iter()
            .position(|segment| segment.piece == piece && segment.placeholder)?;
        let tail = context.skeleton.segments.get(tail_index)?.range;
        let previous = context.skeleton.segments.get(tail_index.checked_sub(1)?)?;
        if !previous.placeholder || previous.range.end != tail.start {
            return None;
        }
        let hole = previous.range;
        let prefix = super::indented_spelling(prefix, reference.indentation);
        if !prefix.contains('\n') || !super::marker_ranges(&context.skeleton.text).is_empty() {
            return None;
        }
        self.remaining = self.remaining.checked_sub(2)?;

        // The zero-repetition parse includes the actual post-range suffix.
        let zero = self.syntax.documents(&context.skeleton, &[tail])?;
        let (zero_doc, zero_item, _) = owning_item(&zero, hole)?;

        // Retain the guaranteed bytes before the opaque remainder. The YAML reader must see
        // another item in the same sequence within those bytes.
        let mut placed = context.clone();
        let spelling = prefix.clone() + context.skeleton.text.get(tail.start..tail.end)?;
        if !placed.replace_spelling(piece, &spelling, None) {
            return None;
        }
        let end = tail.start + prefix.len();
        placed.skeleton.segments.get_mut(tail_index)?.range.start = end;
        placed.terms.get_mut(piece)?.start = end;
        placed.skeleton.segments.insert(
            tail_index,
            SkeletonSegment {
                range: Span::new(tail.start, end),
                piece,
                placeholder: false,
            },
        );
        let nonempty = self.syntax.documents(&placed.skeleton, &[])?;
        let (doc, item, sequence) = owning_item(&nonempty, hole)?;
        (doc.offset == zero_doc.offset
            && item.span.start == zero_item.span.start
            && doc.ownership.nodes.iter().any(|node| {
                node.kind == YamlNodeKind::Item
                    && node.parent == Some(sequence)
                    && (tail.start..end).contains(&(doc.offset + node.span.start))
            }))
        .then_some(())
    }
}

fn owning_item(
    chunks: &ManifestChunks,
    hole: Span,
) -> Option<(&DocumentLayout, &helm_schema_syntax::YamlNode, usize)> {
    let doc = chunks
        .owned
        .iter()
        .find(|doc| doc.offset <= hole.start && hole.end <= doc.scope.end)?;
    let owned: &YamlOwnership = &doc.ownership;
    let local = Span::new(hole.start - doc.offset, hole.end - doc.offset);
    let scalar = owned.nodes.iter().find(|node| {
        node.kind == YamlNodeKind::Scalar(ScalarStyle::Plain) && !node.key && node.span == local
    })?;
    if !matches!(
        super::block_parent(owned, scalar),
        Some(YamlNodeKind::Pair | YamlNodeKind::Item)
    ) {
        return None;
    }
    let item = std::iter::successors(Some(scalar), |node| {
        node.parent.and_then(|parent| owned.nodes.get(parent))
    })
    .find(|node| node.kind == YamlNodeKind::Item)?;
    let sequence = item.parent?;
    (owned.nodes.get(sequence)?.kind == YamlNodeKind::Sequence).then_some((doc, item, sequence))
}
