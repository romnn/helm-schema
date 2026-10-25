//! Typed AST for Go template *expressions* — the inside of a `{{ ... }}`
//! action. Structural consumers parse Helm/YAML with the fused tree-sitter
//! grammar and use this module to pattern-match on structured `Call` /
//! `Pipeline` / `Literal` nodes instead of re-implementing a
//! string-literal-aware tokenizer over raw bytes. Bytes inside a Go string
//! literal can no longer
//! masquerade as helper calls or `default …` patterns by accident.

use std::rc::Rc;

use helm_schema_syntax::{ActionId, Span, TemplateAction};
use tree_sitter::Node;

/// A Go-template literal value.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// `"..."` — escapes already decoded. e.g. `"a\"b"` → `String("a\"b")`.
    String(String),
    /// Interpreted string whose runtime bytes cannot be represented exactly as UTF-8.
    ///
    /// The original quoted lexeme is retained for diagnostics, not as runtime content.
    UnresolvedString(String),
    /// `` `...` `` — backtick raw string, escapes preserved and source carriage returns discarded.
    RawString(String),
    /// Signed integer literal. Decimal, hex (`0x`), octal (`0o`/`0`),
    /// binary (`0b`) are all decoded; digit underscores `_` are stripped.
    Int(i64),
    /// Floating-point literal.
    Float(f64),
    /// Boolean literal.
    Bool(bool),
    /// Nil literal.
    Nil,
}

impl Literal {
    /// Returns the literal's decoded string content if it's a string
    /// (interpreted or raw). Unresolved strings and non-string literals return `None`.
    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Literal::String(s) | Literal::RawString(s) => Some(s),
            Literal::UnresolvedString(_)
            | Literal::Int(_)
            | Literal::Float(_)
            | Literal::Bool(_)
            | Literal::Nil => None,
        }
    }
}

/// A parsed Go-template expression — the inside of a single `{{ ... }}`.
///
/// `Unknown` is the safety net for grammar nodes we don't model
/// (rare literals, `ERROR` nodes from malformed input); the raw text
/// is preserved so callers can still inspect it.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateExpr {
    /// Typed scalar literal.
    Literal(Literal),
    /// Dotted field-access chain rooted at the current context.
    /// `.Values.foo.bar` → `Field(["Values","foo","bar"])`; bare `.`
    /// is `Field(vec![])`.
    Field(Vec<String>),
    /// Selector on a non-context operand: `$root.Values.X` becomes
    /// `Selector { operand: Variable("root"), path: ["Values","X"] }`.
    Selector {
        /// Expression on which selector fields are resolved.
        operand: Box<TemplateExpr>,
        /// Field segments resolved from the operand.
        path: Vec<String>,
    },
    /// `$varname`; bare `$` is `Variable(String::new())`.
    Variable(String),
    /// `funcname arg1 arg2 ...`. Bare identifiers come through as
    /// `Call { args: vec![] }` because the grammar models them as
    /// zero-arg calls.
    Call {
        /// Function identifier as written in the action.
        function: String,
        /// Positional arguments in source order.
        args: Vec<TemplateExpr>,
    },
    /// `a | b | c`. The piped value is passed as the LAST positional
    /// argument to the next stage at render time.
    Pipeline(Vec<TemplateExpr>),
    /// Expression wrapped in grouping parentheses.
    Parenthesized(Box<TemplateExpr>),
    /// `$x := expr`.
    VariableDefinition {
        /// Variable name as written, INCLUDING the leading dollar sign
        /// (`"$x"`). Note the asymmetry with [`TemplateExpr::Variable`],
        /// which carries the bare name; consumers normalize with
        /// `trim_start_matches('$')`.
        name: String,
        /// Expression assigned by the definition.
        value: Box<TemplateExpr>,
    },
    /// `$x = expr`.
    Assignment {
        /// Variable name as written, INCLUDING the leading dollar sign
        /// (`"$x"`), like [`TemplateExpr::VariableDefinition`].
        name: String,
        /// Expression assigned to the existing variable.
        value: Box<TemplateExpr>,
    },
    /// Source text for an expression shape not represented by this AST.
    Unknown(String),
}

impl TemplateExpr {
    /// Walk this expression and every nested sub-expression in preorder,
    /// invoking `visit` on each. Used by extractors to scan for patterns.
    pub fn walk<F: FnMut(&TemplateExpr)>(&self, mut visit: F) {
        self.walk_inner(&mut visit);
    }

    /// Strip every surrounding `Parenthesized` wrapper and return the
    /// inner-most non-parens node. Parens are syntactic grouping in Go
    /// templates — same rule as arithmetic: depth and ordering don't
    /// change *what* the expression is, only its associativity. Use
    /// this helper at the top of any pattern-match that wants to treat
    /// `(.X)`, `((.X))`, `((((.X))))` as identical to `.X`. Convert-site
    /// path collapsing (`convert_selector`) handles the `(prefix).suffix`
    /// case structurally; this helper covers standalone uses.
    ///
    /// **When NOT to use:** inside a [`Self::walk`] visitor. `walk`
    /// already recurses through `Parenthesized`, so the visitor sees
    /// the inner node on its own. Adding `deparen()` to the visitor's
    /// match would fire the same pattern twice — once at the parens
    /// parent (deparened down to the inner), once at the inner itself.
    /// `walk` callbacks should match on `expr` directly; only use
    /// `deparen()` on argument slots / pipeline stages that the walk
    /// doesn't independently traverse.
    ///
    /// **Semantic caveat:** at runtime, `.A.B.C` errors out when an
    /// intermediate is nil, but `(.A).B.C` (or `(.A.B).C`) returns nil
    /// instead — the parens are a Helm idiom for nil-tolerant access.
    /// `deparen` only resolves the *structural* parens; nullability
    /// inference is the caller's concern (see
    /// `crate::required_inference` for the default-fallback pass).
    #[must_use]
    pub fn deparen(&self) -> &TemplateExpr {
        let mut current = self;
        while let TemplateExpr::Parenthesized(inner) = current {
            current = inner;
        }
        current
    }

    /// Reports whether rendering this expression can produce structural YAML.
    #[must_use]
    pub fn renders_yaml_fragment(&self) -> bool {
        match self.deparen() {
            TemplateExpr::Call { function, args } => {
                matches!(function.as_str(), "toYaml" | "nindent" | "indent")
                    || (function == "tpl"
                        && args
                            .first()
                            .is_some_and(TemplateExpr::renders_yaml_fragment))
                    || args.iter().any(TemplateExpr::renders_yaml_fragment)
            }
            TemplateExpr::Pipeline(stages) => {
                stages.iter().any(TemplateExpr::renders_yaml_fragment)
            }
            _ => false,
        }
    }

    /// The rendered indent width of a fragment expression
    /// (`… | nindent N` / `… | indent N`), when statically known.
    #[must_use]
    pub fn fragment_indent_width(&self) -> Option<usize> {
        match self.deparen() {
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "indent" | "nindent") =>
            {
                indent_width_from_call_args(args)
            }
            // `tpl` is the identity on text that carries no template action,
            // so an indent applied to its SUBJECT survives the render:
            // prometheus writes `{{ tpl (toYaml .Values.server.ingress.tls |
            // indent 4) $ }}` at column 0 and the rows still land at column 4.
            TemplateExpr::Call { function, args } if function == "tpl" => {
                args.first().and_then(TemplateExpr::fragment_indent_width)
            }
            TemplateExpr::Pipeline(stages) => stages
                .iter()
                .rev()
                .find_map(TemplateExpr::fragment_indent_width),
            _ => None,
        }
    }

    fn walk_inner<F: FnMut(&TemplateExpr)>(&self, visit: &mut F) {
        visit(self);
        match self {
            TemplateExpr::Selector { operand, .. } => operand.walk_inner(visit),
            TemplateExpr::Call { args, .. } => {
                for a in args {
                    a.walk_inner(visit);
                }
            }
            TemplateExpr::Pipeline(stages) => {
                for s in stages {
                    s.walk_inner(visit);
                }
            }
            TemplateExpr::Parenthesized(inner) => inner.walk_inner(visit),
            TemplateExpr::VariableDefinition { value, .. }
            | TemplateExpr::Assignment { value, .. } => value.walk_inner(visit),
            TemplateExpr::Literal(_)
            | TemplateExpr::Field(_)
            | TemplateExpr::Variable(_)
            | TemplateExpr::Unknown(_) => {}
        }
    }
}

fn indent_width_from_call_args(args: &[TemplateExpr]) -> Option<usize> {
    match args.first()?.deparen() {
        TemplateExpr::Literal(Literal::Int(width)) => usize::try_from(*width).ok(),
        _ => None,
    }
}

/// Parse a body of Helm/Go template text (zero or more `{{ ... }}`
/// actions interleaved with YAML/text) and return a flat list of every
/// expression found across every action, recursing through control-flow
/// bodies. Comment actions are skipped; `{{ template "name" . }}` is
/// normalised into `Call { function: "template", args: [Literal::String,
/// argument?] }` so `include` and `template` keyword forms look
/// identical to extractors. Returns an empty list on empty input or
/// tree-sitter failure (never panics).
#[must_use]
pub fn parse_action_expressions(body_text: &str) -> Vec<TemplateExpr> {
    if body_text.is_empty() {
        return Vec::new();
    }

    let Some(tree) = helm_schema_syntax::parse_go_template(body_text) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    collect_from_node(tree.root_node(), body_text, &mut out);
    out
}

/// The typed expressions of one node of an already-parsed Go-template
/// tree, converted exactly as [`parse_action_expressions`] converts the
/// same node after parsing its text on its own.
#[must_use]
pub fn node_expressions(node: Node<'_>, src: &str) -> Vec<TemplateExpr> {
    let mut out = Vec::new();
    collect_from_node(node, src, &mut out);
    out
}

/// One template action with the typed expressions inside its delimiters,
/// lowered from the retained Go-template tree. A control bracket carries
/// the condition or range clause it holds.
#[derive(Debug)]
pub struct ParsedAction {
    /// The action as the syntax layer delimits it.
    pub action: TemplateAction,
    /// The action's expressions in source order.
    pub expressions: Rc<[TemplateExpr]>,
}

/// Every action of one parsed template, lowered once from its retained
/// Go-template tree and indexed by [`ActionId`], so evaluating an action
/// any number of times never parses its text again.
#[derive(Debug, Default)]
pub struct ParsedActions {
    actions: Vec<ParsedAction>,
}

impl ParsedActions {
    /// Lowers `actions`, the byte-ordered action list of the template whose
    /// tree `root` is.
    #[must_use]
    pub fn lower(root: Node<'_>, src: &str, actions: &[TemplateAction]) -> Self {
        let actions = actions
            .iter()
            .map(|action| {
                let mut expressions = Vec::new();
                collect_span_expressions(root, src, action.span, &mut expressions);
                ParsedAction {
                    action: *action,
                    expressions: Rc::from(expressions),
                }
            })
            .collect();
        Self { actions }
    }

    /// The action with this identity.
    #[must_use]
    pub fn get(&self, id: ActionId) -> Option<&ParsedAction> {
        self.actions.get(id.0)
    }

    /// The actions that start inside `span`, in source order.
    #[must_use]
    pub fn starting_in(&self, span: Span) -> &[ParsedAction] {
        let first = self
            .actions
            .partition_point(|parsed| parsed.action.span.start < span.start);
        let end = self
            .actions
            .partition_point(|parsed| parsed.action.span.start < span.end);
        self.actions.get(first..end).unwrap_or_default()
    }

    /// The action whose span ends exactly at `byte`.
    #[must_use]
    pub fn ending_at(&self, byte: usize) -> Option<&ParsedAction> {
        let before = self
            .actions
            .partition_point(|parsed| parsed.action.span.start < byte);
        self.actions
            .get(before.checked_sub(1)?)
            .filter(|parsed| parsed.action.span.end == byte)
    }

    /// The action whose span starts where `span` starts.
    #[must_use]
    pub fn at(&self, span: Span) -> Option<&ParsedAction> {
        let index = self
            .actions
            .binary_search_by_key(&span.start, |parsed| parsed.action.span.start)
            .ok()?;
        self.get(ActionId(index))
    }
}

/// The typed expressions of every node inside `span` of an already-parsed
/// Go-template tree, in source order, without parsing the span's text again.
#[must_use]
pub fn span_expressions(root: Node<'_>, src: &str, span: Span) -> Vec<TemplateExpr> {
    let mut out = Vec::new();
    collect_span_expressions(root, src, span, &mut out);
    out
}

/// Collect the expression nodes inside `span`. A node only partly inside it
/// (the control action whose header bracket `span` is) contributes the
/// children that lie inside.
fn collect_span_expressions(node: Node<'_>, src: &str, span: Span, out: &mut Vec<TemplateExpr>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.end_byte() <= span.start || child.start_byte() >= span.end {
            continue;
        }
        if span.start <= child.start_byte() && child.end_byte() <= span.end {
            collect_from_node(child, src, out);
        } else {
            collect_span_expressions(child, src, span, out);
        }
    }
}

/// Recursively flatten every action / expression in `node` into `out`.
///
/// Walks the tree-sitter AST; each visit either appends a typed
/// expression (for action / pipeline nodes) or recurses into children
/// (for `template`, control-flow bodies, etc.). Define/block names
/// (string literals) are intentionally skipped because they're never
/// interesting to expression extractors — only the body and any
/// non-name fields contribute.
fn collect_from_node(node: Node<'_>, src: &str, out: &mut Vec<TemplateExpr>) {
    match node.kind() {
        "text" | "yaml_no_injection_text" | "comment" => {}

        // Control-flow and definition bodies: recurse into every named
        // child. For `{{ define "name" }}` / `{{ block "name" arg }}` the
        // `name` field (an opaque string literal that adds noise) is
        // skipped; the other action kinds have no `name` field.
        "template" | "if_action" | "range_action" | "with_action" | "define_action"
        | "block_action" => {
            let name_node = node.child_by_field_name("name");
            let mut cursor = node.walk();
            for ch in node.named_children(&mut cursor) {
                if Some(ch.id()) == name_node.map(|n| n.id()) {
                    continue;
                }
                collect_from_node(ch, src, out);
            }
        }

        // `{{ range $i, $v := EXPR }}` — only the range expression
        // contributes a meaningful sub-expression; the destructured
        // index/element variables are noise. The plain
        // `{{ range EXPR }}` form arrives via the surrounding
        // `range_action`'s named children directly.
        "range_variable_definition" => {
            if let Some(range_node) = node.child_by_field_name("range") {
                collect_from_node(range_node, src, out);
            }
        }

        // `{{ template "name" . }}` — synthesise a Call so extractors
        // looking for include-or-template keyword usage see the same
        // shape as a real `{{ include "name" . }}` call.
        "template_action" => {
            let name_node = node.child_by_field_name("name");
            let arg_node = node.child_by_field_name("argument");
            let mut args: Vec<TemplateExpr> = Vec::new();
            if let Some(n) = name_node {
                args.push(convert_pipeline(n, src));
            }
            if let Some(a) = arg_node {
                args.push(convert_pipeline(a, src));
            }
            out.push(TemplateExpr::Call {
                function: "template".to_string(),
                args,
            });
        }

        // Single-action pipelines arrive here because `_pipeline_action`
        // and `_action` / `_pipeline` are anonymous in the grammar —
        // their pipeline child is what surfaces as a named child of
        // `template`. So for `{{ include "X" . }}`, this case receives
        // the `function_call` node directly.
        _ => {
            let expr = convert_pipeline(node, src);
            // Push only top-level expressions — not `text`, not
            // `Unknown(<delimiter>)`. We've already filtered above.
            if !matches!(expr, TemplateExpr::Unknown(ref s) if s.is_empty()) {
                out.push(expr);
            }
        }
    }
}

/// Convert one tree-sitter expression node into [`TemplateExpr`].
///
/// Anything we don't recognise becomes [`TemplateExpr::Unknown`] with
/// the node's source text — never panics, never drops information.
fn convert_pipeline(node: Node<'_>, src: &str) -> TemplateExpr {
    // A recovery tree inside an expression means the node's shape does not
    // faithfully describe the source (tree-sitter may have skipped bytes),
    // and converting the survivors can fabricate a WRONG values path —
    // `.Values.١key` (a Go-valid Unicode-digit-initial field this grammar
    // does not lex) recovered into `Field(["Values","key"])`. Abstain with
    // the raw text instead: no signal beats a wrong one. String literals
    // are exempt: their decoder already preserves undecodable escapes
    // verbatim, which keeps the value typed as a string.
    if node.has_error()
        && !matches!(
            node.kind(),
            "interpreted_string_literal" | "raw_string_literal"
        )
    {
        return TemplateExpr::Unknown(node_text(node, src).to_string());
    }
    match node.kind() {
        "function_call" => TemplateExpr::Call {
            function: field_text(node, "function", src).to_string(),
            args: convert_args(node, src),
        },
        "method_call" => {
            // `(.x.y).Method arg1 ...` — model as a Call where the
            // function "name" is the selector text (with leading dots).
            // Extractors that care about specific functions check bare
            // identifiers like `"include"` / `"default"`, so a method
            // call name like `".x.y.Method"` never collides.
            TemplateExpr::Call {
                function: field_text(node, "method", src).to_string(),
                args: convert_args(node, src),
            }
        }
        "chained_pipeline" => {
            // The grammar nests `chained_pipeline` left-associatively:
            // `a | b | c` → `chained_pipeline(chained_pipeline(a, b), c)`.
            // Flatten back into a linear list for easier matching. A
            // trailing pipe (`{{ x | }}`, tolerated by Go) contributes no
            // stage, so a single survivor collapses to the stage itself —
            // exactly what the pipe-less spelling parses to.
            let mut stages: Vec<TemplateExpr> = Vec::new();
            collect_pipeline_stages(node, src, &mut stages);
            if stages.len() == 1 {
                return stages.remove(0);
            }
            TemplateExpr::Pipeline(stages)
        }
        "parenthesized_pipeline" => {
            // The parenthesised sub-expression is the (only) named child.
            // Empty parens (`{{ () }}`) — vanishingly rare in Helm —
            // surface as an `Unknown` carrying the raw text so callers
            // can still inspect what was there.
            let inner = node.named_child(0).map_or_else(
                || TemplateExpr::Unknown(node_text(node, src).to_string()),
                |n| convert_pipeline(n, src),
            );
            TemplateExpr::Parenthesized(Box::new(inner))
        }
        "selector_expression" => convert_selector(node, src),
        // A bare identifier only reaches this converter as a selector
        // operand (`now.Year`): Go evaluates it as a niladic function whose
        // result the selector fields index into.
        "identifier" => TemplateExpr::Call {
            function: node_text(node, src).to_string(),
            args: Vec::new(),
        },
        "field" => TemplateExpr::Field(vec![field_text(node, "name", src).to_string()]),
        "dot" => TemplateExpr::Field(Vec::new()),
        "variable" => TemplateExpr::Variable(field_text(node, "name", src).to_string()),
        "variable_definition" => TemplateExpr::VariableDefinition {
            name: field_text(node, "variable", src).to_string(),
            value: Box::new(convert_value_field(node, src)),
        },
        "assignment" => TemplateExpr::Assignment {
            name: field_text(node, "variable", src).to_string(),
            value: Box::new(convert_value_field(node, src)),
        },
        "interpreted_string_literal" => {
            let raw = node_text(node, src);
            TemplateExpr::Literal(decode_interpreted_string(raw).map_or_else(
                || Literal::UnresolvedString(raw.to_string()),
                Literal::String,
            ))
        }
        "raw_string_literal" => {
            let raw = node_text(node, src);
            let content = raw.strip_prefix('`').unwrap_or(raw);
            let content = content.strip_suffix('`').unwrap_or(content);
            TemplateExpr::Literal(Literal::RawString(content.replace('\r', "")))
        }
        "int_literal" => {
            let raw = node_text(node, src);
            parse_int_literal(raw).map_or_else(
                || TemplateExpr::Unknown(raw.to_string()),
                |n| TemplateExpr::Literal(Literal::Int(n)),
            )
        }
        "float_literal" => {
            let raw = node_text(node, src);
            parse_float_literal(raw).map_or_else(
                || TemplateExpr::Unknown(raw.to_string()),
                |f| TemplateExpr::Literal(Literal::Float(f)),
            )
        }
        "true" => TemplateExpr::Literal(Literal::Bool(true)),
        "false" => TemplateExpr::Literal(Literal::Bool(false)),
        "nil" => TemplateExpr::Literal(Literal::Nil),
        _ => {
            // Anything else: keep the text. Includes ERROR/MISSING/
            // UNEXPECTED nodes from malformed input, `imaginary_literal`,
            // `rune_literal`, and `pipeline_stub` — all rare in Helm
            // template content but kept verbatim so callers can still
            // pattern-match on the raw text if they care.
            TemplateExpr::Unknown(node_text(node, src).to_string())
        }
    }
}

/// Convert the `operand.field` chain rooted at a `selector_expression`.
///
/// Adjacent selectors merge: `.Values.foo.bar` becomes
/// `Field(["Values","foo","bar"])` instead of three nested nodes.
/// A parenthesized receiver remains a [`TemplateExpr::Parenthesized`]
/// operand because grouping changes nil handling at runtime. For example,
/// `(.Values.resources.limits).memory` tolerates an absent `limits`, while
/// the ungrouped chain does not. Downstream analysis still derives the full
/// values identity through the selector without losing that boundary.
fn convert_selector(node: Node<'_>, src: &str) -> TemplateExpr {
    let suffix = field_text(node, "field", src).to_string();
    let operand = node
        .child_by_field_name("operand")
        .map(|n| convert_pipeline(n, src));
    match operand {
        Some(TemplateExpr::Field(mut path)) => {
            path.push(suffix);
            TemplateExpr::Field(path)
        }
        Some(TemplateExpr::Selector {
            operand: inner,
            mut path,
        }) => {
            path.push(suffix);
            TemplateExpr::Selector {
                operand: inner,
                path,
            }
        }
        Some(other) => TemplateExpr::Selector {
            operand: Box::new(other),
            path: vec![suffix],
        },
        None => TemplateExpr::Unknown(node_text(node, src).to_string()),
    }
}

/// Convert the `value` field of a `variable_definition` / `assignment`
/// node, defaulting to an empty `Unknown` if the field is absent
/// (only happens for grammar-error inputs).
fn convert_value_field(node: Node<'_>, src: &str) -> TemplateExpr {
    node.child_by_field_name("value").map_or_else(
        || TemplateExpr::Unknown(String::new()),
        |n| convert_pipeline(n, src),
    )
}

fn convert_args(node: Node<'_>, src: &str) -> Vec<TemplateExpr> {
    let Some(list) = node.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .map(|ch| convert_pipeline(ch, src))
        .collect()
}

/// Source text of `node`'s named-field child, or `""` if absent.
fn field_text<'a>(node: Node<'_>, name: &str, src: &'a str) -> &'a str {
    node.child_by_field_name(name)
        .and_then(|n| n.utf8_text(src.as_bytes()).ok())
        .unwrap_or("")
}

/// Read a tree-sitter node's source text, returning `""` if the byte
/// range is unreadable (which `tree_sitter::Node::utf8_text` only
/// returns on internal corruption — practically never).
fn node_text<'a>(node: Node<'_>, src: &'a str) -> &'a str {
    node.utf8_text(src.as_bytes()).unwrap_or("")
}

fn collect_pipeline_stages(node: Node<'_>, src: &str, out: &mut Vec<TemplateExpr>) {
    let mut cursor = node.walk();
    for ch in node.named_children(&mut cursor) {
        if ch.kind() == "chained_pipeline" {
            collect_pipeline_stages(ch, src, out);
        } else {
            out.push(convert_pipeline(ch, src));
        }
    }
}

/// Decodes byte escapes before validating the complete runtime string as UTF-8.
///
/// Go's octal and hexadecimal byte escapes can jointly encode one Unicode character.
/// Unicode escapes name scalar values instead. Invalid escapes and valid non-UTF-8
/// Go strings have no exact string representation in this AST.
fn decode_interpreted_string(raw: &str) -> Option<String> {
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut output = Vec::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            if matches!(character, '\n' | '"') {
                return None;
            }
            let mut encoded = [0; 4];
            output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            continue;
        }
        match chars.next()? {
            'n' => output.push(b'\n'),
            'r' => output.push(b'\r'),
            't' => output.push(b'\t'),
            '\\' => output.push(b'\\'),
            '"' => output.push(b'"'),
            'a' => output.push(0x07),
            'b' => output.push(0x08),
            'f' => output.push(0x0c),
            'v' => output.push(0x0b),
            'x' => output.push(u8::try_from(decode_escape_digits(&mut chars, 2, 16)?).ok()?),
            marker @ ('u' | 'U') => {
                let width = if marker == 'u' { 4 } else { 8 };
                let character = char::from_u32(decode_escape_digits(&mut chars, width, 16)?)?;
                let mut encoded = [0; 4];
                output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            }
            first @ '0'..='7' => {
                let value = first.to_digit(8)? * 64 + decode_escape_digits(&mut chars, 2, 8)?;
                output.push(u8::try_from(value).ok()?);
            }
            _ => return None,
        }
    }
    String::from_utf8(output).ok()
}

fn decode_escape_digits(chars: &mut std::str::Chars<'_>, width: usize, radix: u32) -> Option<u32> {
    let mut value = 0;
    for _ in 0..width {
        value = value * radix + chars.next()?.to_digit(radix)?;
    }
    Some(value)
}

/// Parse a Go integer literal, including underscores and base prefixes
/// (`0x` / `0X`, `0o` / `0O`, `0b` / `0B`, leading-zero octal).
fn parse_int_literal(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    let (sign, rest) = match raw.as_bytes().first() {
        Some(b'+') => (1, &raw[1..]),
        Some(b'-') => (-1, &raw[1..]),
        _ => (1, raw),
    };
    let cleaned: String = rest.chars().filter(|c| *c != '_').collect();
    let (radix, digits) = if cleaned.starts_with("0x") || cleaned.starts_with("0X") {
        (16, &cleaned[2..])
    } else if cleaned.starts_with("0b") || cleaned.starts_with("0B") {
        (2, &cleaned[2..])
    } else if cleaned.starts_with("0o") || cleaned.starts_with("0O") {
        (8, &cleaned[2..])
    } else if cleaned.starts_with('0')
        && cleaned.len() > 1
        && cleaned.chars().all(|c| c.is_ascii_digit())
    {
        (8, &cleaned[1..])
    } else {
        (10, cleaned.as_str())
    };
    // Parse the magnitude wider than i64 so i64::MIN, whose magnitude
    // does not fit a positive i64, still decodes.
    let value = i128::from_str_radix(digits, radix).ok()?;
    i64::try_from(i128::from(sign) * value).ok()
}

fn parse_float_literal(raw: &str) -> Option<f64> {
    let cleaned: String = raw.chars().filter(|c| *c != '_').collect();
    let (sign, rest) = match cleaned.as_bytes().first() {
        Some(b'+') => (1.0, &cleaned[1..]),
        Some(b'-') => (-1.0, &cleaned[1..]),
        _ => (1.0, cleaned.as_str()),
    };
    let value = if let Some(digits) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        parse_hex_float(digits).map(|value| sign * value)
    } else {
        cleaned.parse::<f64>().ok()
    }?;
    // Go rejects out-of-range float literals in BOTH spellings
    // (`strconv.ParseFloat` reports ErrRange for `1e400` exactly as for
    // `0x1p10000`), and a non-finite `Literal::Float` would break
    // expression equality (NaN != NaN under the derived `PartialEq`).
    value.is_finite().then_some(value)
}

/// Decode a Go hex float mantissa-and-exponent (the part after `0x`):
/// `hexdigits[.hexdigits]p[±]decimal`, e.g. `1p60` or `1.8p-1`. Bitnami's
/// memory-size helpers spell binary unit factors this way (`0x1p20` for
/// one MiB). Returns `None` for malformed input; the caller
/// ([`parse_float_literal`]) rejects non-finite results for both float
/// spellings. Rounding on very long mantissas may differ from Go's
/// correctly-rounded `strconv.ParseFloat` in the last ULP, and an
/// exponent below the subnormal range underflows to `0` where Go
/// reports a range error — both far outside any precision a chart
/// value needs.
fn parse_hex_float(digits: &str) -> Option<f64> {
    let (mantissa, exponent) = digits.split_once('p').or_else(|| digits.split_once('P'))?;
    let exponent = exponent.parse::<i32>().ok()?;
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((int_part, frac_part)) => (int_part, frac_part),
        None => (mantissa, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    // Accumulate the whole mantissa as an integer and fold the fraction
    // into the binary exponent (each hex fraction digit is 2^-4), then
    // scale stepwise so intermediate powers never overflow or underflow
    // on the way to a representable (possibly subnormal) result.
    let mut value = 0.0f64;
    for c in int_part.chars().chain(frac_part.chars()) {
        value = value * 16.0 + f64::from(c.to_digit(16)?);
    }
    let frac_digits = i32::try_from(frac_part.chars().count()).ok()?;
    let scaled = scale_by_pow2(value, exponent.checked_sub(frac_digits.checked_mul(4)?)?);
    scaled.is_finite().then_some(scaled)
}

/// Multiply `value` by 2^`exp` without letting an intermediate power of
/// two overflow to infinity or vanish to zero before the mantissa gets a
/// say (`0x1p-1024` is a representable subnormal, `0x.1p1024` is finite).
fn scale_by_pow2(mut value: f64, mut exp: i32) -> f64 {
    if value == 0.0 {
        // Zero scales to zero for any exponent (Go accepts `0x0p10000`
        // as plain zero); skipping the loops below would otherwise reach
        // `0.0 * inf` = NaN.
        return value;
    }
    while exp > 1023 && value.is_finite() {
        value *= (2.0f64).powi(1023);
        exp -= 1023;
    }
    while exp < -1022 && value != 0.0 {
        value *= (2.0f64).powi(-1022);
        exp += 1022;
    }
    value * (2.0f64).powi(exp.clamp(-1022, 1023))
}

#[cfg(test)]
#[path = "tests/expr.rs"]
mod tests;

/// Helper names `include`d or `template`d OUTSIDE every control region of
/// the template. Helm parses and renders each manifest template whole, so
/// reaching the document at all executes these calls; `if`/`range`/`with`
/// bodies render conditionally and `define`/`block` bodies run only when
/// included, so both are excluded.
#[must_use]
pub fn unconditional_include_names(source: &str) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    let Some(tree) = crate::parse_go_template(source) else {
        return names;
    };
    let root = tree.root_node();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "if_action"
            | "range_action"
            | "with_action"
            | "define_action"
            | "block_action"
            | "text"
            | "yaml_no_injection_text"
            | "comment" => {}
            _ => {
                let mut exprs = Vec::new();
                collect_from_node(child, source, &mut exprs);
                for expr in exprs {
                    expr.walk(|inner| {
                        if let TemplateExpr::Call { function, args } = inner
                            && matches!(function.as_str(), "include" | "template")
                            && let Some(TemplateExpr::Literal(
                                Literal::String(name) | Literal::RawString(name),
                            )) = args.first().map(TemplateExpr::deparen)
                        {
                            names.insert(name.clone());
                        }
                    });
                }
            }
        }
    }
    names
}
