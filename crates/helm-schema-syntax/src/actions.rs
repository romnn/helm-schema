//! Template action tokens: a linear, byte-ordered stream of `{{ … }}`
//! action spans extracted from the tree-sitter Go-template parse. The layout
//! parser overlays these tokens on the line structure; they classify holes
//! and control regions but never decide YAML container structure.

use crate::cst::{ControlKind, Span};

/// Parse `source` with the tree-sitter Go-template grammar. This crate is
/// the single owner of the raw Go-template tree parse; `helm-schema-ast`
/// re-exports this function and layers the typed expression AST on top.
#[tracing::instrument(skip_all, fields(bytes = source.len()))]
#[must_use]
pub fn parse_go_template(source: &str) -> Option<tree_sitter::Tree> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(source, None)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActionToken {
    pub(crate) span: Span,
    pub(crate) kind: TokenKind,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TokenKind {
    /// `{{ pipeline }}` — renders output at this position.
    Output { expr_span: Span },
    /// `{{ $x := … }}` / `{{ $x = … }}` — renders nothing.
    Assign,
    /// `{{/* … */}}`.
    TemplateComment,
    /// `{{ break }}` / `{{ continue }}`.
    ControlAtom,
    /// Header of a control region; `region_end` is the byte end of the whole
    /// `{{ … }}…{{ end }}` construct.
    RegionOpen {
        region: usize,
        kind: ControlKind,
        region_end: usize,
    },
    /// An `{{ else }}` / `{{ else if … }}` / `{{ else with … }}` boundary.
    RegionBranch { region: usize },
    /// The `{{ end }}` closer.
    RegionEnd { region: usize },
    /// Unparsable action content.
    Error,
}

pub(crate) fn collect_action_tokens(root: tree_sitter::Node<'_>) -> Vec<ActionToken> {
    let mut out = Vec::new();
    let mut next_region = 0usize;
    walk_body(root, &mut next_region, &mut out);
    out.sort_by_key(|token| (token.span.start, token.span.end));
    out
}

fn is_left_delimiter(kind: &str) -> bool {
    matches!(kind, "{{" | "{{-")
}

fn is_right_delimiter(kind: &str) -> bool {
    matches!(kind, "}}" | "-}}")
}

fn control_kind(node_kind: &str) -> Option<ControlKind> {
    match node_kind {
        "if_action" => Some(ControlKind::If),
        "with_action" => Some(ControlKind::With),
        "range_action" => Some(ControlKind::Range),
        "define_action" => Some(ControlKind::Define),
        "block_action" => Some(ControlKind::Block),
        _ => None,
    }
}

/// Walk a body-level node (the template root, an `ERROR` recovery node):
/// inline `{{ expr }}` actions arrive as flat delimiter/content sibling runs
/// because `_pipeline_action` is inlined in the grammar.
fn walk_body(node: tree_sitter::Node<'_>, next_region: &mut usize, out: &mut Vec<ActionToken>) {
    let mut group = GroupState::default();
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        dispatch_body_child(child, next_region, &mut group, out);
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    group.finish(node.end_byte(), out);
}

fn dispatch_body_child<'tree>(
    child: tree_sitter::Node<'tree>,
    next_region: &mut usize,
    group: &mut GroupState<'tree>,
    out: &mut Vec<ActionToken>,
) {
    let kind = child.kind();
    if !child.is_named() {
        if is_left_delimiter(kind) {
            group.open(child, out);
        } else if is_right_delimiter(kind) {
            group.close(child.end_byte(), out);
        }
        return;
    }
    if group.collect_named(child) {
        return;
    }
    match kind {
        "text" | "yaml_no_injection_text" | "comment" => {
            // A comment outside a delimiter group only occurs in recovery
            // trees; the grouped path handles the normal case.
        }
        "template_action" => out.push(ActionToken {
            span: node_span(child),
            kind: TokenKind::Output {
                expr_span: node_span(child),
            },
        }),
        "break_action" | "continue_action" => out.push(ActionToken {
            span: node_span(child),
            kind: TokenKind::ControlAtom,
        }),
        "ERROR" => walk_body(child, next_region, out),
        _ => {
            if let Some(control) = control_kind(kind) {
                walk_control(child, control, next_region, out);
            } else {
                out.push(ActionToken {
                    span: node_span(child),
                    kind: TokenKind::Error,
                });
            }
        }
    }
}

/// Walk a control node. Its own bracket actions (`{{ if … }}`, `{{ else }}`,
/// `{{ end }}`) are the delimiter runs WITHOUT a field name; branch-body
/// children (and inner action delimiters) carry the branch field.
fn walk_control(
    node: tree_sitter::Node<'_>,
    kind: ControlKind,
    next_region: &mut usize,
    out: &mut Vec<ActionToken>,
) {
    let region = *next_region;
    *next_region += 1;
    let region_end = node.end_byte();

    let mut group = GroupState::default();
    let mut bracket: Option<Bracket> = None;
    let mut opened = false;

    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        let field = cursor.field_name();
        if let Some(state) = bracket.as_mut() {
            if !child.is_named() && field.is_none() && is_right_delimiter(child.kind()) {
                let closed = Bracket {
                    end: child.end_byte(),
                    ..*state
                };
                bracket = None;
                emit_bracket(closed, region, kind, region_end, &mut opened, out);
            } else if !child.is_named() {
                match child.kind() {
                    "else" => state.has_else = true,
                    "end" => state.has_end = true,
                    _ => {}
                }
            }
        } else if field.is_none() && !child.is_named() && is_left_delimiter(child.kind()) {
            bracket = Some(Bracket {
                start: child.start_byte(),
                end: child.end_byte(),
                has_else: false,
                has_end: false,
            });
        } else {
            dispatch_body_child(child, next_region, &mut group, out);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    group.finish(node.end_byte(), out);
}

#[derive(Clone, Copy)]
struct Bracket {
    start: usize,
    end: usize,
    has_else: bool,
    has_end: bool,
}

fn emit_bracket(
    bracket: Bracket,
    region: usize,
    kind: ControlKind,
    region_end: usize,
    opened: &mut bool,
    out: &mut Vec<ActionToken>,
) {
    let span = Span::new(bracket.start, bracket.end);
    let token_kind = if !*opened {
        *opened = true;
        TokenKind::RegionOpen {
            region,
            kind,
            region_end,
        }
    } else if bracket.has_end {
        TokenKind::RegionEnd { region }
    } else if bracket.has_else {
        TokenKind::RegionBranch { region }
    } else {
        TokenKind::Error
    };
    out.push(ActionToken {
        span,
        kind: token_kind,
    });
}

/// State machine grouping a flat `{{`, content…, `}}` sibling run into one
/// action token. Defensive against recovery trees: unbalanced delimiters
/// surface as [`TokenKind::Error`] tokens instead of being dropped.
#[derive(Default)]
struct GroupState<'tree> {
    start: Option<usize>,
    named: Vec<tree_sitter::Node<'tree>>,
}

impl<'tree> GroupState<'tree> {
    fn open(&mut self, child: tree_sitter::Node<'tree>, out: &mut Vec<ActionToken>) {
        if let Some(start) = self.start.take() {
            out.push(ActionToken {
                span: Span::new(start, child.start_byte()),
                kind: TokenKind::Error,
            });
            self.named.clear();
        }
        self.start = Some(child.start_byte());
    }

    fn close(&mut self, end: usize, out: &mut Vec<ActionToken>) {
        let Some(start) = self.start.take() else {
            return;
        };
        let span = Span::new(start, end);
        let kind = classify_group(&self.named);
        self.named.clear();
        out.push(ActionToken { span, kind });
    }

    /// Returns `true` when the child was consumed as group content.
    fn collect_named(&mut self, child: tree_sitter::Node<'tree>) -> bool {
        if self.start.is_some() {
            self.named.push(child);
            return true;
        }
        false
    }

    fn finish(&mut self, node_end: usize, out: &mut Vec<ActionToken>) {
        if let Some(start) = self.start.take() {
            out.push(ActionToken {
                span: Span::new(start, node_end),
                kind: TokenKind::Error,
            });
            self.named.clear();
        }
    }
}

fn classify_group(named: &[tree_sitter::Node<'_>]) -> TokenKind {
    let Some(first) = named.first() else {
        return TokenKind::Error;
    };
    match first.kind() {
        "comment" => TokenKind::TemplateComment,
        "variable_definition" | "assignment" => TokenKind::Assign,
        "ERROR" => TokenKind::Error,
        _ => {
            let last = named.last().unwrap_or(first);
            TokenKind::Output {
                expr_span: Span::new(first.start_byte(), last.end_byte()),
            }
        }
    }
}

fn node_span(node: tree_sitter::Node<'_>) -> Span {
    Span::new(node.start_byte(), node.end_byte())
}
