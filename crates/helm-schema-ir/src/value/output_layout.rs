use std::rc::Rc;

use helm_schema_core::{PlainScalarPlacement, Predicate, PredicateMemo};
use helm_schema_syntax::Span;

use super::{Coverage, ExecutedSite, FrozenValue, State, TextBuffer, TextPart};

const MAX_SKELETON_TERMS: usize = 256;
const MAX_SKELETON_BYTES: usize = 64 * 1024;
const PLACEHOLDER: &str = "helm_plain_block_value_proof";

/// Certifies only the single-unknown-emission case from the actual frozen file output.
#[tracing::instrument(skip_all)]
pub(super) fn certify_plain_scalar_placements(state: &mut State, memo: &PredicateMemo) {
    let output = &state.output;
    for site in &mut state.sites {
        site.plain_scalar_placement = if !state.output_widened
            && site.program == state.program
            && site.rendered
            && site.coverage_exact
        {
            certify_site(output, site, memo)
        } else {
            PlainScalarPlacement::Unknown
        };
    }
}

impl ExecutedSite {
    pub(crate) fn plain_scalar_placement(&self) -> PlainScalarPlacement {
        self.plain_scalar_placement
    }
}

fn certify_site(
    output: &TextBuffer,
    site: &ExecutedSite,
    memo: &PredicateMemo,
) -> PlainScalarPlacement {
    if site.condition.contains_approximation()
        || site.condition == Predicate::False
        || site.value.exact_text().is_some()
    {
        return PlainScalarPlacement::Unknown;
    }
    let mut query = SkeletonQuery {
        target: &site.value,
        condition: &site.condition,
        memo,
        remaining: MAX_SKELETON_TERMS,
        text: String::new(),
        found: false,
    };
    if !query.append_buffer(output) || !query.found {
        return PlainScalarPlacement::Unknown;
    }
    // This certificate only supports preimages whose target cannot participate in a stripped marker.
    let text = query.text.replace("<no value>", "");
    let mut occurrences = text.match_indices(PLACEHOLDER);
    let Some((start, placeholder)) = occurrences.next() else {
        return PlainScalarPlacement::Unknown;
    };
    if occurrences.next().is_some() {
        return PlainScalarPlacement::Unknown;
    }
    let span = Span::new(start, start + placeholder.len());
    if plain_block_value(&text, span) {
        PlainScalarPlacement::BlockValue
    } else {
        PlainScalarPlacement::Unknown
    }
}

struct SkeletonQuery<'a> {
    target: &'a FrozenValue,
    condition: &'a Predicate,
    memo: &'a PredicateMemo,
    remaining: usize,
    text: String,
    found: bool,
}

impl SkeletonQuery<'_> {
    fn append_buffer(&mut self, buffer: &TextBuffer) -> bool {
        for part in &buffer.parts {
            if self.remaining == 0 {
                return false;
            }
            self.remaining -= 1;
            let appended = match part {
                TextPart::Bytes(bytes) => self.append(bytes),
                TextPart::Frozen(value) => {
                    if let Some(text) = value.exact_text() {
                        self.append(&text)
                    } else {
                        if self.found
                            || !Rc::ptr_eq(&value.heap, &self.target.heap)
                            || !Rc::ptr_eq(&value.value.node, &self.target.value.node)
                        {
                            return false;
                        }
                        self.found = true;
                        self.append(PLACEHOLDER)
                    }
                }
                TextPart::Unknown(_) => false,
                TextPart::Select { decision, yes, no } => {
                    if decision.coverage != Coverage::Exact {
                        return false;
                    }
                    match (
                        self.memo
                            .exactly_implies(self.condition, &decision.when_true),
                        self.memo
                            .exactly_implies(self.condition, &decision.when_false),
                    ) {
                        (true, false) => self.append_buffer(yes),
                        (false, true) => self.append_buffer(no),
                        (true, true) | (false, false) => false,
                    }
                }
            };
            if !appended {
                return false;
            }
        }
        true
    }

    fn append(&mut self, bytes: &str) -> bool {
        if self.text.len().saturating_add(bytes.len()) > MAX_SKELETON_BYTES {
            return false;
        }
        self.text.push_str(bytes);
        true
    }
}

fn plain_block_value(text: &str, span: Span) -> bool {
    let Some(tree) = helm_schema_syntax::parse_yaml(text) else {
        return false;
    };
    if tree.root_node().has_error() {
        return false;
    }
    let Some(mut node) = tree
        .root_node()
        .descendant_for_byte_range(span.start, span.end)
    else {
        return false;
    };
    while node.kind() != "plain_scalar" {
        let Some(parent) = node.parent() else {
            return false;
        };
        node = parent;
    }
    if node.start_byte() != span.start || node.end_byte() != span.end {
        return false;
    }
    let mut block_value = false;
    while let Some(parent) = node.parent() {
        node = parent;
        if node.is_missing() {
            return false;
        }
        match node.kind() {
            "flow_mapping"
            | "flow_sequence"
            | "single_quote_scalar"
            | "double_quote_scalar"
            | "block_scalar"
            | "tag"
            | "anchor"
            | "alias" => return false,
            "block_mapping_pair" if !block_value => {
                block_value = node.child_by_field_name("value").is_some_and(|value| {
                    value.start_byte() == span.start && value.end_byte() == span.end
                });
                if !block_value {
                    return false;
                }
            }
            "block_sequence_item" if !block_value => {
                let mut cursor = node.walk();
                block_value = node
                    .named_children(&mut cursor)
                    .any(|value| value.start_byte() == span.start && value.end_byte() == span.end);
                if !block_value {
                    return false;
                }
            }
            _ => {}
        }
    }
    block_value
}

#[cfg(test)]
#[path = "../tests/runtime_output_layout.rs"]
mod tests;
