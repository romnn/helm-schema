//! Typed AST for Go template *expressions* — the inside of a `{{ ... }}`
//! action. Structural consumers parse Helm/YAML with the fused tree-sitter
//! grammar and use this module to pattern-match on structured `Call` /
//! `Pipeline` / `Literal` nodes instead of re-implementing a
//! string-literal-aware tokenizer over raw bytes. Bytes inside a Go string
//! literal can no longer
//! masquerade as helper calls or `default …` patterns by accident.

use tree_sitter::Node;

/// A Go-template literal value.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// `"..."` — escapes already decoded. e.g. `"a\"b"` → `String("a\"b")`.
    String(String),
    /// `` `...` `` — backtick raw string, escapes preserved verbatim.
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
    /// (interpreted or raw). For non-string literals returns `None`.
    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Literal::String(s) | Literal::RawString(s) => Some(s),
            _ => None,
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
        /// Variable name without the leading dollar sign.
        name: String,
        /// Expression assigned by the definition.
        value: Box<TemplateExpr>,
    },
    /// `$x = expr`.
    Assignment {
        /// Variable name without the leading dollar sign.
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
    match node.kind() {
        "function_call" => call_with_unfolded_pipe(
            field_text(node, "function", src).to_string(),
            convert_args(node, src),
        ),
        "method_call" => {
            // `(.x.y).Method arg1 ...` — model as a Call where the
            // function "name" is the selector text (with leading dots).
            // Extractors that care about specific functions check bare
            // identifiers like `"include"` / `"default"`, so a method
            // call name like `".x.y.Method"` never collides.
            call_with_unfolded_pipe(
                field_text(node, "method", src).to_string(),
                convert_args(node, src),
            )
        }
        "chained_pipeline" => {
            // The grammar nests `chained_pipeline` left-associatively:
            // `a | b | c` → `chained_pipeline(chained_pipeline(a, b), c)`.
            // Flatten back into a linear list for easier matching.
            let mut stages: Vec<TemplateExpr> = Vec::new();
            collect_pipeline_stages(node, src, &mut stages);
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
        "interpreted_string_literal" => TemplateExpr::Literal(Literal::String(
            decode_interpreted_string(node_text(node, src)),
        )),
        "raw_string_literal" => {
            let raw = node_text(node, src);
            let content = raw.strip_prefix('`').unwrap_or(raw);
            let content = content.strip_suffix('`').unwrap_or(content);
            TemplateExpr::Literal(Literal::RawString(content.to_string()))
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

/// Rebuild a call whose LAST argument absorbed an un-spaced pipe.
///
/// Go's tokenizer emits `)` and `|` as separate tokens regardless of
/// spacing, and an argument-position pipeline is only legal when
/// parenthesized — a pipe always splits the ENCLOSING command. The
/// tree-sitter grammar instead attaches `(include "x" .)| sha256sum`'s
/// pipe to the last argument, so a BARE pipeline argument is always that
/// parsing error: its first stage is the real argument and the remaining
/// stages pipe the whole call (redis-ha's checksum annotation spells its
/// digest exactly this way).
fn call_with_unfolded_pipe(function: String, mut args: Vec<TemplateExpr>) -> TemplateExpr {
    if matches!(args.last(), Some(TemplateExpr::Pipeline(stages)) if stages.len() >= 2)
        && let Some(TemplateExpr::Pipeline(mut stages)) = args.pop()
    {
        let first = stages.remove(0);
        args.push(first);
        let mut out = Vec::with_capacity(stages.len() + 1);
        out.push(TemplateExpr::Call { function, args });
        out.append(&mut stages);
        return TemplateExpr::Pipeline(out);
    }
    TemplateExpr::Call { function, args }
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
            match convert_pipeline(ch, src) {
                // A stage that unfolded an un-spaced argument pipe (see
                // `call_with_unfolded_pipe`) is itself a pipeline; Go
                // reads the whole chain as one flat stage list.
                TemplateExpr::Pipeline(stages) => out.extend(stages),
                stage => out.push(stage),
            }
        }
    }
}

/// Decode a Go `interpreted_string_literal` (text including the
/// surrounding `"`s) into its runtime string value. Handles the
/// escape forms tree-sitter-go-template's grammar models:
///   - single-char: `\n`, `\r`, `\t`, `\\`, `\"`, `\'`, `\0`, `\a`,
///     `\b`, `\f`, `\v`
///   - `\xHH` (exactly two hex digits, byte value)
///   - `\uHHHH` (exactly four hex digits, BMP code point)
///   - `\UHHHHHHHH` (exactly eight hex digits, any code point)
///
/// For any malformed escape (wrong digit count, non-hex char, surrogate
/// code point) the original `\X…` bytes are preserved verbatim — we
/// never produce silently-wrong output. Octal escapes (`\NNN`) and
/// other unknown one-char escapes are also preserved as-is. That's
/// not technically Go's behaviour but it's the safe choice for a
/// static-analysis tool: produce *no* signal rather than a wrong one.
fn decode_interpreted_string(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);

    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(next) = chars.next() else {
            // Trailing backslash — preserve verbatim.
            out.push('\\');
            break;
        };
        match next {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '\'' => out.push('\''),
            '0' => out.push('\0'),
            'a' => out.push('\x07'),
            'b' => out.push('\x08'),
            'f' => out.push('\x0c'),
            'v' => out.push('\x0b'),
            'x' => decode_hex_escape(&mut chars, 2, 'x', &mut out),
            'u' => decode_hex_escape(&mut chars, 4, 'u', &mut out),
            'U' => decode_hex_escape(&mut chars, 8, 'U', &mut out),
            other => {
                // Unknown escape — preserve the backslash and the char.
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

/// Consume exactly `width` hex digits from `chars` and append the
/// decoded character to `out`. On any failure (fewer chars available,
/// non-hex digit, code point not representable as a `char`) preserve
/// the original `\<marker><consumed>` bytes verbatim.
fn decode_hex_escape(
    chars: &mut std::str::Chars<'_>,
    width: usize,
    marker: char,
    out: &mut String,
) {
    let mut buf = String::with_capacity(width);
    for _ in 0..width {
        if let Some(ch) = chars.next() {
            buf.push(ch);
        } else {
            break;
        }
    }
    let valid = buf.len() == width && buf.chars().all(|c| c.is_ascii_hexdigit());
    if valid
        && let Ok(code) = u32::from_str_radix(&buf, 16)
        && let Some(ch) = char::from_u32(code)
    {
        out.push(ch);
        return;
    }
    out.push('\\');
    out.push(marker);
    out.push_str(&buf);
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
    let value = i64::from_str_radix(digits, radix).ok()?;
    Some(sign * value)
}

fn parse_float_literal(raw: &str) -> Option<f64> {
    let cleaned: String = raw.chars().filter(|c| *c != '_').collect();
    cleaned.parse::<f64>().ok()
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
