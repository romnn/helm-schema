//! Template action tokens: a linear, byte-ordered stream of `{{ … }}`
//! action spans extracted from the tree-sitter Go-template parse. The layout
//! parser overlays these tokens on the line structure; they classify holes
//! and control regions but never decide YAML container structure.

use crate::cst::{ControlKind, Span};

thread_local! {
    /// Whole-chart analysis parses tens of thousands of short action strings,
    /// and a `Parser` is far more expensive to build than to reuse. Trees own
    /// their data, so a parser can be handed the next source immediately.
    static GO_TEMPLATE_PARSER: std::cell::RefCell<Option<tree_sitter::Parser>> =
        std::cell::RefCell::new(new_go_template_parser());
}

fn new_go_template_parser() -> Option<tree_sitter::Parser> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    Some(parser)
}

/// Parse `source` with the tree-sitter Go-template grammar. This crate is
/// the single owner of the raw Go-template tree parse; `helm-schema-ast`
/// re-exports this function and layers the typed expression AST on top.
#[tracing::instrument(skip_all, fields(bytes = source.len()))]
#[must_use]
pub fn parse_go_template(source: &str) -> Option<tree_sitter::Tree> {
    GO_TEMPLATE_PARSER.with_borrow_mut(|parser| parser.as_mut()?.parse(source, None))
}

/// One `{{ … }}` action of the source, as the Go-template tree delimits it.
#[derive(Clone, Copy, Debug)]
pub struct TemplateAction {
    /// The action's byte range, delimiters included.
    pub span: Span,
    /// What the action does.
    pub kind: ActionKind,
    /// The left delimiter is `{{-`, which trims the whitespace before the
    /// action. `{{-3}}` is a plain `{{` before the literal `-3`.
    pub trim_left: bool,
    /// The right delimiter is `-}}`, which trims the whitespace after the
    /// action.
    pub trim_right: bool,
}

/// The position of an action in its document's byte-ordered action list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionId(pub usize);

/// The role of one template action, read off its Go-template tree node.
#[derive(Clone, Copy, Debug)]
pub enum ActionKind {
    /// `{{ pipeline }}` — renders output at this position.
    Output {
        /// The expression between the delimiters.
        expr_span: Span,
        /// The width of a terminal literal `indent`/`nindent` call.
        render_indent: Option<usize>,
    },
    /// `{{ $x := … }}` / `{{ $x = … }}` — renders nothing.
    Assign,
    /// `{{/* … */}}`.
    TemplateComment,
    /// `{{ break }}`.
    Break,
    /// `{{ continue }}`.
    Continue,
    /// Header of a control region; `region_end` is the byte end of the whole
    /// `{{ … }}…{{ end }}` construct.
    RegionOpen {
        /// The region's index in source order.
        region: usize,
        /// The control action that opens the region.
        kind: ControlKind,
        /// The byte end of the whole region.
        region_end: usize,
    },
    /// An `{{ else }}` / `{{ else if … }}` / `{{ else with … }}` boundary.
    RegionBranch {
        /// The index of the region the boundary belongs to.
        region: usize,
    },
    /// The `{{ end }}` closer.
    RegionEnd {
        /// The index of the region the closer ends.
        region: usize,
    },
    /// Unparsable action content.
    Error,
}

pub(crate) fn collect_action_tokens(
    root: tree_sitter::Node<'_>,
    source: &str,
) -> Vec<TemplateAction> {
    let mut out = Vec::new();
    let mut next_region = 0usize;
    walk_body(root, &mut next_region, &mut out);
    for token in &mut out {
        let ActionKind::Output {
            expr_span,
            render_indent,
        } = &mut token.kind
        else {
            continue;
        };
        *render_indent = root
            .descendant_for_byte_range(expr_span.start, expr_span.end)
            .and_then(|node| terminal_indent_width(node, source));
    }
    out.sort_by_key(|token| (token.span.start, token.span.end));
    out
}

/// Reads a terminal parsed `indent`/`nindent` call without interpreting source text.
fn terminal_indent_width(node: tree_sitter::Node<'_>, source: &str) -> Option<usize> {
    let terminal = match node.kind() {
        "function_call" => node,
        "chained_pipeline" | "pipeline" | "template_action" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).last()?
        }
        _ => return None,
    };
    if terminal.kind() != "function_call" {
        return None;
    }
    let function = terminal.child_by_field_name("function")?;
    let name = function.utf8_text(source.as_bytes()).ok()?;
    if !matches!(name, "indent" | "nindent") {
        return None;
    }
    let arguments = terminal.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let width = arguments.named_children(&mut cursor).next()?;
    if width.kind() != "int_literal" {
        return None;
    }
    width.utf8_text(source.as_bytes()).ok()?.parse().ok()
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
fn walk_body(node: tree_sitter::Node<'_>, next_region: &mut usize, out: &mut Vec<TemplateAction>) {
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
    out: &mut Vec<TemplateAction>,
) {
    let kind = child.kind();
    if !child.is_named() {
        if is_left_delimiter(kind) {
            group.open(child, out);
        } else if is_right_delimiter(kind) {
            group.close(child, out);
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
        "template_action" => out.push(node_action(
            child,
            ActionKind::Output {
                expr_span: node_span(child),
                render_indent: None,
            },
        )),
        "break_action" => out.push(node_action(child, ActionKind::Break)),
        "continue_action" => out.push(node_action(child, ActionKind::Continue)),
        "ERROR" => walk_body(child, next_region, out),
        _ => {
            if let Some(control) = control_kind(kind) {
                walk_control(child, control, next_region, out);
            } else {
                out.push(untrimmed(node_span(child), ActionKind::Error));
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
    out: &mut Vec<TemplateAction>,
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
                    trim_right: child.kind() == "-}}",
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
                trim_left: child.kind() == "{{-",
                trim_right: false,
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
    trim_left: bool,
    trim_right: bool,
    has_else: bool,
    has_end: bool,
}

fn emit_bracket(
    bracket: Bracket,
    region: usize,
    kind: ControlKind,
    region_end: usize,
    opened: &mut bool,
    out: &mut Vec<TemplateAction>,
) {
    let span = Span::new(bracket.start, bracket.end);
    let token_kind = if !*opened {
        *opened = true;
        ActionKind::RegionOpen {
            region,
            kind,
            region_end,
        }
    } else if bracket.has_end {
        ActionKind::RegionEnd { region }
    } else if bracket.has_else {
        ActionKind::RegionBranch { region }
    } else {
        ActionKind::Error
    };
    out.push(TemplateAction {
        span,
        kind: token_kind,
        trim_left: bracket.trim_left,
        trim_right: bracket.trim_right,
    });
}

/// State machine grouping a flat `{{`, content…, `}}` sibling run into one
/// action token. Defensive against recovery trees: unbalanced delimiters
/// surface as [`ActionKind::Error`] tokens instead of being dropped.
#[derive(Default)]
struct GroupState<'tree> {
    start: Option<usize>,
    trim_left: bool,
    named: Vec<tree_sitter::Node<'tree>>,
}

impl<'tree> GroupState<'tree> {
    fn open(&mut self, child: tree_sitter::Node<'tree>, out: &mut Vec<TemplateAction>) {
        if let Some(start) = self.start.take() {
            out.push(untrimmed(
                Span::new(start, child.start_byte()),
                ActionKind::Error,
            ));
            self.named.clear();
        }
        self.start = Some(child.start_byte());
        self.trim_left = child.kind() == "{{-";
    }

    fn close(&mut self, delimiter: tree_sitter::Node<'tree>, out: &mut Vec<TemplateAction>) {
        let Some(start) = self.start.take() else {
            return;
        };
        let kind = classify_group(&self.named);
        self.named.clear();
        out.push(TemplateAction {
            span: Span::new(start, delimiter.end_byte()),
            kind,
            trim_left: self.trim_left,
            trim_right: delimiter.kind() == "-}}",
        });
    }

    /// Returns `true` when the child was consumed as group content.
    fn collect_named(&mut self, child: tree_sitter::Node<'tree>) -> bool {
        if self.start.is_some() {
            self.named.push(child);
            return true;
        }
        false
    }

    fn finish(&mut self, node_end: usize, out: &mut Vec<TemplateAction>) {
        if let Some(start) = self.start.take() {
            out.push(untrimmed(Span::new(start, node_end), ActionKind::Error));
            self.named.clear();
        }
    }
}

fn classify_group(named: &[tree_sitter::Node<'_>]) -> ActionKind {
    let Some(first) = named.first() else {
        return ActionKind::Error;
    };
    match first.kind() {
        "comment" => ActionKind::TemplateComment,
        "variable_definition" | "assignment" => ActionKind::Assign,
        "ERROR" => ActionKind::Error,
        _ => {
            let last = named.last().unwrap_or(first);
            ActionKind::Output {
                expr_span: Span::new(first.start_byte(), last.end_byte()),
                render_indent: None,
            }
        }
    }
}

fn node_span(node: tree_sitter::Node<'_>) -> Span {
    Span::new(node.start_byte(), node.end_byte())
}

/// A single-node action (`{{ template … }}`, `{{ break }}`), whose delimiter
/// tokens are its own first and last children.
fn node_action(node: tree_sitter::Node<'_>, kind: ActionKind) -> TemplateAction {
    let mut cursor = node.walk();
    let last = node.children(&mut cursor).last();
    TemplateAction {
        span: node_span(node),
        kind,
        trim_left: node.child(0).is_some_and(|child| child.kind() == "{{-"),
        trim_right: last.is_some_and(|child| child.kind() == "-}}"),
    }
}

/// An action recovered from an unbalanced delimiter run.
fn untrimmed(span: Span, kind: ActionKind) -> TemplateAction {
    TemplateAction {
        span,
        kind,
        trim_left: false,
        trim_right: false,
    }
}

#[cfg(test)]
#[path = "tests/actions.rs"]
mod tests;
