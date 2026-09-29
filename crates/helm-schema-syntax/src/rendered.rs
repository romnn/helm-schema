//! Rendered output of a template body: for every execution arm, the ordered
//! literal texts and action outputs that arm emits, before any YAML parse.
//!
//! Arms come from the template's control structure (the action tokens of
//! the Go-template tree), never from source lines. Delimiter trims apply to
//! the adjacent template literals, exactly as the Go lexer applies them, and
//! never to emitted values. Each source literal and action is indexed once in
//! [`RenderedBody::pieces`]; arms reference those pieces, so shared prefixes,
//! suffixes and branch bodies are stored once.

use crate::{ActionId, ActionKind, ControlKind, Span, TemplateAction, TemplatedDocument};

/// Upper bound on the execution alternatives enumerated for one body.
/// Exceeding it is a loss of certainty, never of alternatives: the body
/// becomes [`LayoutUncertainty::Overflow`] and no arm seen before the bound
/// is treated as exhaustive.
pub const MAX_LAYOUT_STATES: usize = 64;

/// The index of a piece in [`RenderedBody::pieces`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PieceId(pub usize);

/// Which execution of the source a piece belongs to: the helper calls that
/// reached its body (outermost first) and its `range` iteration, if any. A
/// helper called twice renders the same source action as two occurrences.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Occurrence {
    /// Helper-call actions from the rendered document down to this body.
    pub calls: Vec<ActionId>,
    /// The iteration of the enclosing `range` body.
    pub iteration: Option<usize>,
}

/// What a hole's output can do to the YAML around it, as the expression
/// layer proves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleShape {
    /// Text whose alphabet and escaping keep the surrounding scalar token and
    /// style intact (no line break, `: `, ` #`, quote or flow punctuation that
    /// could end the token). `nonempty` records a proof that it is never empty.
    ScalarText {
        /// The output is never the empty string.
        nonempty: bool,
    },
    /// Nothing is known about the output's text.
    Unknown,
}

/// The expression layer's proofs about hole outputs.
pub trait HoleShapes {
    /// The proved shape of `action`'s output.
    fn shape(&self, action: ActionId) -> HoleShape;
}

/// The oracle that proves nothing: every hole is [`HoleShape::Unknown`].
pub struct UnknownShapes;

impl HoleShapes for UnknownShapes {
    fn shape(&self, _action: ActionId) -> HoleShape {
        HoleShape::Unknown
    }
}

/// One rendered piece. Literal text carries no action; action output carries
/// its action and no source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderedPiece {
    /// Template literal text after delimiter trims; `span` indexes
    /// [`RenderedBody::source`].
    Text {
        /// The trimmed literal's byte range.
        span: Span,
        /// The execution the literal belongs to.
        occurrence: Occurrence,
    },
    /// The output of one action.
    Hole {
        /// The action that renders the output.
        action: ActionId,
        /// What the output can do to the surrounding YAML.
        shape: HoleShape,
        /// The execution the output belongs to.
        occurrence: Occurrence,
    },
}

/// The branch an arm takes in one control region: the region's index in
/// source order, and the branch in written order. An `if`/`with` without a
/// bare `else` has one more, empty branch after its written ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BranchChoice {
    /// The control region's index in source order.
    pub region: usize,
    /// The branch taken.
    pub branch: usize,
}

/// One distinct rendered output of a body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedArm {
    /// Every choice path that renders exactly these pieces. A region whose
    /// branches all render alike contributes no choice.
    pub paths: Vec<Vec<BranchChoice>>,
    /// The pieces the arm renders, in output order.
    pub pieces: Vec<PieceId>,
}

/// Why a body's or an arm's layout is not known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutUncertainty {
    /// A `range` body renders a cardinality-dependent repetition.
    Range,
    /// More than [`MAX_LAYOUT_STATES`] execution alternatives.
    Overflow,
    /// The Go-template parse recovered from an error.
    Recovery,
    /// A hole of unknown shape.
    RawHole,
    /// The YAML parse of the skeleton has an error or missing node.
    Parse,
    /// A permitted substitution of a hole changes the YAML structure.
    Substitution,
    /// A tag or alias whose interpretation the parse does not resolve.
    TagOrAlias,
    /// A substitution can form the merge key `<<` over a collection.
    MergeKey,
    /// A piece does not resolve in the body's source.
    Provenance,
}

/// A body's rendered alternatives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyLayout {
    /// Every distinct rendered output of the body.
    Arms(Vec<RenderedArm>),
    /// The alternatives are not enumerable; nothing is claimed about them.
    Uncertain(LayoutUncertainty),
}

/// The rendered pieces of a template body and its arms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedBody<'src> {
    /// The source every [`RenderedPiece::Text`] span indexes.
    pub source: &'src str,
    /// Every literal and action of the body, indexed once in source order.
    pub pieces: Vec<RenderedPiece>,
    /// The body's arms over those pieces.
    pub layout: BodyLayout,
}

/// Render the top-level body of `document`, whose Go-template tree is `root`.
#[must_use]
pub fn render_body<'src>(
    document: &TemplatedDocument<'src>,
    root: tree_sitter::Node<'_>,
    shapes: &dyn HoleShapes,
) -> RenderedBody<'src> {
    let builder = Builder::new(document.source(), document.actions(), root, shapes);
    let mut next = 0;
    let layout = match builder.body(&mut next) {
        Ok(_) if next < builder.tokens.len() => BodyLayout::Uncertain(LayoutUncertainty::Recovery),
        Ok(alternatives) => BodyLayout::Arms(alternatives),
        Err(uncertainty) => BodyLayout::Uncertain(uncertainty),
    };
    RenderedBody {
        source: document.source(),
        pieces: builder.pieces,
        layout,
    }
}

/// Every alternative of a partial render.
type Alternatives = Vec<RenderedArm>;

struct Builder<'a, 'tree> {
    tokens: &'a [TemplateAction],
    root: tree_sitter::Node<'tree>,
    pieces: Vec<RenderedPiece>,
    /// The piece of the literal before token `i` (index `tokens.len()` is the
    /// trailing literal); `None` when the trimmed literal is empty.
    gaps: Vec<Option<PieceId>>,
    /// The hole piece of each output token and `block` call.
    holes: Vec<Option<PieceId>>,
}

impl<'a, 'tree> Builder<'a, 'tree> {
    fn new(
        source: &str,
        tokens: &'a [TemplateAction],
        root: tree_sitter::Node<'tree>,
        shapes: &dyn HoleShapes,
    ) -> Self {
        let mut pieces = Vec::new();
        let mut gaps = Vec::with_capacity(tokens.len() + 1);
        let mut holes = Vec::with_capacity(tokens.len());
        let mut start = 0;
        for (index, token) in tokens.iter().enumerate() {
            let before = index
                .checked_sub(1)
                .and_then(|previous| tokens.get(previous));
            let trim_start = before.is_some_and(|previous| previous.trim_right);
            gaps.push(literal_piece(
                source,
                Span::new(start, token.span.start),
                trim_start,
                token.trim_left,
                &mut pieces,
            ));
            let renders = matches!(
                token.kind,
                ActionKind::Output { .. }
                    | ActionKind::RegionOpen {
                        kind: ControlKind::Block,
                        ..
                    }
            );
            holes.push(renders.then(|| {
                let action = ActionId(index);
                pieces.push(RenderedPiece::Hole {
                    action,
                    shape: shapes.shape(action),
                    occurrence: Occurrence::default(),
                });
                PieceId(pieces.len() - 1)
            }));
            start = token.span.end;
        }
        let trim_start = tokens.last().is_some_and(|last| last.trim_right);
        gaps.push(literal_piece(
            source,
            Span::new(start, source.len()),
            trim_start,
            false,
            &mut pieces,
        ));
        Self {
            tokens,
            root,
            pieces,
            gaps,
            holes,
        }
    }

    /// Render from token `next` up to the branch boundary or end that closes
    /// the enclosing region (left unconsumed), or the end of the source.
    fn body(&self, next: &mut usize) -> Result<Alternatives, LayoutUncertainty> {
        let mut alternatives = vec![RenderedArm {
            paths: vec![Vec::new()],
            pieces: Vec::new(),
        }];
        loop {
            if let Some(Some(gap)) = self.gaps.get(*next) {
                append_piece(&mut alternatives, *gap);
            }
            let Some(token) = self.tokens.get(*next) else {
                return Ok(alternatives);
            };
            match token.kind {
                ActionKind::RegionBranch { .. } | ActionKind::RegionEnd { .. } => {
                    return Ok(alternatives);
                }
                ActionKind::Output { .. } => {
                    if let Some(Some(hole)) = self.holes.get(*next) {
                        append_piece(&mut alternatives, *hole);
                    }
                    *next += 1;
                }
                ActionKind::Assign | ActionKind::TemplateComment => *next += 1,
                // Loop control only occurs inside a `range` body.
                ActionKind::Break | ActionKind::Continue => return Err(LayoutUncertainty::Range),
                ActionKind::Error => return Err(LayoutUncertainty::Recovery),
                ActionKind::RegionOpen {
                    region,
                    kind,
                    region_end,
                } => {
                    *next += 1;
                    let rendered = self.region(region, kind, region_end, next)?;
                    alternatives = product(&alternatives, &rendered)?;
                }
            }
        }
    }

    /// Render one control region whose opener was consumed; leaves `next`
    /// after its `{{ end }}`.
    fn region(
        &self,
        region: usize,
        kind: ControlKind,
        region_end: usize,
        next: &mut usize,
    ) -> Result<Alternatives, LayoutUncertainty> {
        match kind {
            // A definition renders nothing where it is written. `block` also
            // calls the template it defines, and another definition of the
            // name can win, so the call's output is the opener's hole until
            // named-template resolution proves the target.
            ControlKind::Define | ControlKind::Block => {
                let call = next
                    .checked_sub(1)
                    .and_then(|opener| self.holes.get(opener))
                    .copied()
                    .flatten();
                self.skip_definition(region_end, next);
                return Ok(vec![RenderedArm {
                    paths: vec![Vec::new()],
                    pieces: call.into_iter().collect(),
                }]);
            }
            ControlKind::Range => return Err(LayoutUncertainty::Range),
            ControlKind::If | ControlKind::With => {}
        }
        let mut branches = Vec::new();
        let mut bare_else = false;
        loop {
            branches.push(self.body(next)?);
            let token = self.tokens.get(*next).ok_or(LayoutUncertainty::Recovery)?;
            *next += 1;
            match token.kind {
                ActionKind::RegionBranch { region: owner } if owner == region => {
                    bare_else |= !self.is_conditional(token.span);
                }
                ActionKind::RegionEnd { region: owner } if owner == region => break,
                _ => return Err(LayoutUncertainty::Recovery),
            }
        }
        if !bare_else {
            branches.push(vec![RenderedArm {
                paths: vec![Vec::new()],
                pieces: Vec::new(),
            }]);
        }
        let mut alternatives: Alternatives = Vec::new();
        for (branch, arms) in branches.into_iter().enumerate() {
            for arm in arms {
                let paths = arm
                    .paths
                    .into_iter()
                    .map(|mut path| {
                        path.insert(0, BranchChoice { region, branch });
                        path
                    })
                    .collect();
                merge_arm(
                    &mut alternatives,
                    RenderedArm {
                        paths,
                        pieces: arm.pieces,
                    },
                );
            }
        }
        // Every branch renders the same pieces: the choice cannot change the
        // layout, so it is not part of any arm.
        if let [only] = alternatives.as_mut_slice() {
            only.paths = vec![Vec::new()];
        }
        check_bound(&alternatives)?;
        Ok(alternatives)
    }

    /// Move `next` past the definition tokens that start before byte
    /// `region_end`.
    fn skip_definition(&self, region_end: usize, next: &mut usize) {
        while self
            .tokens
            .get(*next)
            .is_some_and(|token| token.span.start < region_end)
        {
            *next += 1;
        }
    }

    /// Whether the branch boundary at `span` carries a condition
    /// (`{{ else if … }}` / `{{ else with … }}`), read from the named nodes
    /// the Go-template tree places inside it.
    fn is_conditional(&self, span: Span) -> bool {
        let Some(control) = self
            .root
            .descendant_for_byte_range(span.start, span.end)
            .and_then(|node| enclosing_control(node, span))
        else {
            return false;
        };
        let mut cursor = control.walk();
        control.children(&mut cursor).any(|child| {
            child.is_named() && span.start <= child.start_byte() && child.end_byte() <= span.end
        })
    }
}

/// The control node whose own bracket covers `span`.
fn enclosing_control(mut node: tree_sitter::Node<'_>, span: Span) -> Option<tree_sitter::Node<'_>> {
    loop {
        if matches!(node.kind(), "if_action" | "with_action")
            && node.start_byte() <= span.start
            && span.end <= node.end_byte()
        {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// The trimmed literal of `span` as a piece, or `None` when it is empty.
/// `{{-` trims the whitespace before the action and `-}}` the whitespace
/// after it, including line breaks, as the Go lexer does.
fn literal_piece(
    source: &str,
    span: Span,
    trim_start: bool,
    trim_end: bool,
    pieces: &mut Vec<RenderedPiece>,
) -> Option<PieceId> {
    let text = source.get(span.start..span.end)?;
    let is_space = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n');
    let start = if trim_start {
        span.end - text.trim_start_matches(is_space).len()
    } else {
        span.start
    };
    let end = if trim_end {
        span.start + text.trim_end_matches(is_space).len()
    } else {
        span.end
    };
    if start >= end {
        return None;
    }
    pieces.push(RenderedPiece::Text {
        span: Span::new(start, end),
        occurrence: Occurrence::default(),
    });
    Some(PieceId(pieces.len() - 1))
}

fn append_piece(alternatives: &mut Alternatives, piece: PieceId) {
    for arm in alternatives {
        arm.pieces.push(piece);
    }
}

/// Every alternative of `left` followed by every alternative of `right`.
fn product(left: &Alternatives, right: &Alternatives) -> Result<Alternatives, LayoutUncertainty> {
    let mut out: Alternatives = Vec::new();
    for first in left {
        for second in right {
            let mut paths = Vec::new();
            for head in &first.paths {
                for tail in &second.paths {
                    let mut path = head.clone();
                    path.extend(tail.iter().copied());
                    paths.push(path);
                }
            }
            let mut pieces = first.pieces.clone();
            pieces.extend(second.pieces.iter().copied());
            merge_arm(&mut out, RenderedArm { paths, pieces });
            check_bound(&out)?;
        }
    }
    Ok(out)
}

/// Add `arm`, merging its paths into an arm that renders the same pieces.
fn merge_arm(alternatives: &mut Alternatives, arm: RenderedArm) {
    match alternatives
        .iter_mut()
        .find(|existing| existing.pieces == arm.pieces)
    {
        Some(existing) => existing.paths.extend(arm.paths),
        None => alternatives.push(arm),
    }
}

/// The alternatives stay enumerable only while their execution paths fit
/// [`MAX_LAYOUT_STATES`].
fn check_bound(alternatives: &Alternatives) -> Result<(), LayoutUncertainty> {
    let paths: usize = alternatives.iter().map(|arm| arm.paths.len()).sum();
    if paths > MAX_LAYOUT_STATES {
        return Err(LayoutUncertainty::Overflow);
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/rendered.rs"]
mod tests;
