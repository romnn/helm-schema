//! Proven YAML failures derived from frozen file output, after template execution.

use std::collections::BTreeMap;
use std::rc::Rc;

use helm_schema_core::{Predicate, PredicateMemo, ValuesPath};
use helm_schema_syntax::{ScalarStyle, Skeleton, SkeletonSegment, Span, YamlNodeKind};

use super::quoted::{self, LexicalState};
use super::scalar_value::{ScalarContent, ScalarRenderPart};
use super::{
    Coverage, Datum, Exit, FileOutput, Flow, FrozenValue, Leaf, TextBuffer, TextPart, ValueNode,
};

// This is Helm 4.2.3's SplitManifests expression, with Go's ASCII Perl whitespace class.
static SPLITTER: std::sync::LazyLock<Result<regex::Regex, regex::Error>> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(?:^|[ \t\r\n\x0c]*\n)---[ \t\r\n\x0c]*"));

#[derive(Clone)]
enum Piece {
    Text(String),
    Input(ValuesPath),
}

#[derive(Clone)]
struct Arm {
    condition: Predicate,
    pieces: Vec<Piece>,
}

/// Unknown output remains a possible survivor of the universal-abort calculation.
#[tracing::instrument(skip_all)]
pub(super) fn abort_predicate(exits: &[Exit], output: FileOutput) -> Predicate {
    let mut failures = Vec::new();
    let mut survivors = Vec::new();
    for exit in exits {
        let condition = exit.state.path_predicate();
        if !super::output::exit_may_continue(exit) {
            failures.push(condition);
            continue;
        }
        let invalid = if matches!(output, FileOutput::Text)
            || !matches!(exit.flow, Flow::Normal(_))
            || exit
                .state
                .active
                .iter()
                .any(|edge| edge.test.coverage == Coverage::Partial)
        {
            Predicate::False
        } else {
            invalid_output(&exit.state.output, &exit.state.heap.arena.predicate_memo)
        };
        failures.push(Predicate::all(vec![condition.clone(), invalid.clone()]));
        survivors.push(Predicate::all(vec![condition, invalid.negated()]));
    }
    Predicate::all(vec![
        Predicate::Or(failures),
        Predicate::Or(survivors).negated(),
    ])
    .normalize_boolean()
}

fn invalid_output(output: &TextBuffer, memo: &PredicateMemo) -> Predicate {
    let mut arms = vec![Arm {
        condition: Predicate::True,
        pieces: Vec::new(),
    }];
    if append_buffer(output, &mut arms, &mut 1024, memo).is_none() {
        return Predicate::False;
    }
    let mut failures = Vec::new();
    let mut survivors = Vec::new();
    for Arm { condition, pieces } in arms {
        let invalid = invalid_pieces(&pieces, memo);
        failures.push(Predicate::all(vec![condition.clone(), invalid.clone()]));
        survivors.push(Predicate::all(vec![condition, invalid.negated()]));
    }
    memo.normalize(Predicate::all(vec![
        Predicate::Or(failures),
        Predicate::Or(survivors).negated(),
    ]))
}

fn append_buffer(
    buffer: &TextBuffer,
    arms: &mut Vec<Arm>,
    remaining: &mut usize,
    memo: &PredicateMemo,
) -> Option<()> {
    for part in &buffer.parts {
        *remaining = remaining.checked_sub(1)?;
        match part {
            TextPart::Bytes(text) => append(arms, &Piece::Text(text.clone())),
            TextPart::Unknown(_) => return None,
            TextPart::Frozen(value) => append_value(value, arms, remaining, memo)?,
            TextPart::Select { decision, yes, no } => {
                let mut selected = Vec::new();
                for (taken, buffer) in [(true, yes), (false, no)] {
                    let mut branch = select(arms, decision, taken, memo)?;
                    append_buffer(buffer, &mut branch, remaining, memo)?;
                    selected.extend(branch);
                }
                if selected.len() > 32 {
                    return None;
                }
                *arms = selected;
            }
        }
    }
    Some(())
}

fn select(
    arms: &[Arm],
    decision: &super::FrozenTest,
    taken: bool,
    memo: &PredicateMemo,
) -> Option<Vec<Arm>> {
    if decision.coverage != Coverage::Exact
        || decision.when_true.contains_approximation()
        || decision.when_false.contains_approximation()
    {
        return None;
    }
    let guard = if taken {
        &decision.when_true
    } else {
        &decision.when_false
    };
    Some(
        arms.iter()
            .filter_map(|arm| {
                let condition =
                    memo.normalize(Predicate::all(vec![arm.condition.clone(), guard.clone()]));
                (condition != Predicate::False).then(|| Arm {
                    condition,
                    pieces: arm.pieces.clone(),
                })
            })
            .collect(),
    )
}

fn append(arms: &mut [Arm], piece: &Piece) {
    for arm in arms {
        if let (Some(Piece::Text(previous)), Piece::Text(text)) = (arm.pieces.last_mut(), piece) {
            previous.push_str(text);
        } else {
            arm.pieces.push(piece.clone());
        }
    }
}

fn append_value(
    value: &FrozenValue,
    arms: &mut Vec<Arm>,
    remaining: &mut usize,
    memo: &PredicateMemo,
) -> Option<()> {
    *remaining = remaining.checked_sub(1)?;
    if let Some(path) = value.identity() {
        append(arms, &Piece::Input(path.clone()));
        return Some(());
    }
    if let Some(text) = value.exact_text() {
        append(arms, &Piece::Text(text));
        return Some(());
    }
    match value.value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => {
            let mut selected = Vec::new();
            for (taken, branch) in [(true, yes), (false, no)] {
                let mut branch_arms = select(arms, decision, taken, memo)?;
                append_value(
                    &FrozenValue {
                        heap: Rc::clone(&value.heap),
                        value: branch.clone(),
                    },
                    &mut branch_arms,
                    remaining,
                    memo,
                )?;
                selected.extend(branch_arms);
            }
            if selected.len() > 32 {
                return None;
            }
            *arms = selected;
        }
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
                    content: Some(ScalarContent::Rendered(parts)),
                    go_type: _,
                },
            origin: _,
        }) => {
            for part in parts {
                match part {
                    ScalarRenderPart::Text(text) => append(arms, &Piece::Text(text.clone())),
                    ScalarRenderPart::Identity {
                        path,
                        stringified: _,
                        lexical_escapes,
                    } if lexical_escapes.is_empty() => append(arms, &Piece::Input(path.clone())),
                    ScalarRenderPart::Identity {
                        path: _,
                        stringified: _,
                        lexical_escapes: _,
                    } => return None,
                }
            }
        }
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        }) => return None,
    }
    Some(())
}

fn invalid_pieces(pieces: &[Piece], memo: &PredicateMemo) -> Predicate {
    if !pieces.iter().any(|piece| matches!(piece, Piece::Input(_))) {
        return Predicate::False;
    }
    let mut skeleton = Skeleton {
        text: String::new(),
        segments: Vec::new(),
    };
    for (index, piece) in pieces.iter().enumerate() {
        let start = skeleton.text.len();
        let (text, placeholder) = match piece {
            Piece::Text(text) => (text.as_str(), false),
            Piece::Input(_) => ("00000000", true),
        };
        // A marker spanning symbolic pieces can change lexical state after concatenation.
        if text.contains('<') || start.saturating_add(text.len()) > 64 * 1024 {
            return Predicate::False;
        }
        skeleton.text.push_str(text);
        skeleton.segments.push(SkeletonSegment {
            range: Span::new(start, skeleton.text.len()),
            piece: index,
            placeholder,
        });
    }
    let Ok(splitter) = &*SPLITTER else {
        return Predicate::False;
    };
    let mut ranges = Vec::new();
    let mut start = 0;
    let trimmed = skeleton.text.trim_start_matches(char::is_whitespace);
    let trim = skeleton.text.len() - trimmed.len();
    for separator in splitter.find_iter(trimmed) {
        ranges.push(Span::new(start, trim + separator.start()));
        start = trim + separator.end();
    }
    ranges.push(Span::new(start, skeleton.text.len()));
    let mut failures = Vec::new();
    let mut prefix = Predicate::True;
    for Span { start, end } in ranges {
        let Some(text) = skeleton.text.get(start..end) else {
            return Predicate::False;
        };
        let trimmed = text.trim_start_matches(char::is_whitespace);
        let start = end - trimmed.len();
        if trimmed.is_empty() {
            continue;
        }
        let document = Skeleton {
            text: trimmed.to_owned(),
            segments: skeleton
                .segments
                .iter()
                .filter_map(|segment| {
                    let from = segment.range.start.max(start);
                    let to = segment.range.end.min(end);
                    (from < to).then(|| SkeletonSegment {
                        range: Span::new(from - start, to - start),
                        ..*segment
                    })
                })
                .collect(),
        };
        let Some(QuotedPrefix {
            invalid,
            preserved: valid_prefix,
        }) = quoted_tokens(document, pieces, &prefix, memo)
        else {
            break;
        };
        failures.push(invalid);
        prefix = valid_prefix;
    }
    memo.normalize(Predicate::Or(failures))
}

// Only ownership before an uncertain substitution is usable. A broken quoted token is a
// proven failure; leaving the certified token through another production remains unknown.
struct QuotedPrefix {
    invalid: Predicate,
    preserved: Predicate,
}

fn quoted_tokens(
    skeleton: Skeleton,
    pieces: &[Piece],
    prefix: &Predicate,
    memo: &PredicateMemo,
) -> Option<QuotedPrefix> {
    let owned = helm_schema_syntax::skeleton_ownership(skeleton).ok()?;
    let mut prefix = prefix.clone();
    let mut failures = Vec::new();
    let mut consumed = 0;
    for node in &owned.nodes {
        let YamlNodeKind::Scalar(style @ (ScalarStyle::DoubleQuoted | ScalarStyle::SingleQuoted)) =
            node.kind
        else {
            continue;
        };
        if owned.skeleton.segments.iter().any(|segment| {
            segment.placeholder
                && consumed <= segment.range.start
                && segment.range.start < node.span.start
        }) {
            break;
        }
        let state = if style == ScalarStyle::DoubleQuoted {
            LexicalState::Double
        } else {
            LexicalState::Single
        };
        let mut states = BTreeMap::from([(state, Predicate::True)]);
        for segment in &owned.skeleton.segments {
            let start = segment.range.start.max(node.span.start + 1);
            let end = segment.range.end.min(node.span.end);
            if start >= end {
                continue;
            }
            states = match pieces.get(segment.piece)? {
                Piece::Text(_) => {
                    quoted::literal(states, owned.skeleton.text.get(start..end)?, memo)
                }
                Piece::Input(path) => quoted::input(states, path, memo),
            };
        }
        let invalid = states
            .remove(&LexicalState::Invalid)
            .unwrap_or(Predicate::False);
        failures.push(Predicate::all(vec![prefix.clone(), invalid]));
        let closed = if style == ScalarStyle::DoubleQuoted {
            LexicalState::Closed
        } else {
            LexicalState::SingleEnd
        };
        prefix = memo.normalize(Predicate::all(vec![
            prefix,
            states.remove(&closed).unwrap_or(Predicate::False),
        ]));
        consumed = node.span.end;
    }
    if owned
        .skeleton
        .segments
        .iter()
        .any(|segment| segment.placeholder && segment.range.start >= consumed)
    {
        prefix = Predicate::False;
    }
    Some(QuotedPrefix {
        invalid: memo.normalize(Predicate::Or(failures)),
        preserved: prefix,
    })
}
