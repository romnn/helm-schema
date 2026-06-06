//! Typed AST for Go template *expressions* — the inside of a `{{ ... }}`
//! action. Sits alongside [`crate::HelmAst`], which models template
//! *structure* (define blocks, control flow, action boundaries) but
//! stores each action's interior as opaque text. This module
//! re-parses that text with `tree-sitter-go-template` so callers can
//! pattern-match on structured `Call` / `Pipeline` / `Literal` nodes
//! instead of re-implementing a string-literal-aware tokenizer over
//! raw bytes. Bytes inside a Go string literal can no longer
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
    Bool(bool),
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
    Literal(Literal),
    /// Dotted field-access chain rooted at the current context.
    /// `.Values.foo.bar` → `Field(["Values","foo","bar"])`; bare `.`
    /// is `Field(vec![])`.
    Field(Vec<String>),
    /// Selector on a non-context operand: `$root.Values.X` becomes
    /// `Selector { operand: Variable("root"), path: ["Values","X"] }`.
    Selector {
        operand: Box<TemplateExpr>,
        path: Vec<String>,
    },
    /// `$varname`; bare `$` is `Variable(String::new())`.
    Variable(String),
    /// `funcname arg1 arg2 ...`. Bare identifiers come through as
    /// `Call { args: vec![] }` because the grammar models them as
    /// zero-arg calls.
    Call {
        function: String,
        args: Vec<TemplateExpr>,
    },
    /// `a | b | c`. The piped value is passed as the LAST positional
    /// argument to the next stage at render time.
    Pipeline(Vec<TemplateExpr>),
    Parenthesized(Box<TemplateExpr>),
    /// `$x := expr`.
    VariableDefinition {
        name: String,
        value: Box<TemplateExpr>,
    },
    /// `$x = expr`.
    Assignment {
        name: String,
        value: Box<TemplateExpr>,
    },
    Unknown(String),
}

impl TemplateExpr {
    /// Walk this expression and every nested sub-expression in preorder,
    /// invoking `visit` on each. Used by extractors to scan for patterns.
    pub fn walk<F: FnMut(&TemplateExpr)>(&self, mut visit: F) {
        self.walk_inner(&mut visit);
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

    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(body_text, None) else {
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

        "template" | "if_action" | "range_action" | "with_action" => {
            let mut cursor = node.walk();
            for ch in node.named_children(&mut cursor) {
                collect_from_node(ch, src, out);
            }
        }

        // `{{ define "name" }} BODY {{ end }}` / `{{ block "name" arg }}
        // BODY {{ end }}` — recurse into every named child *except* the
        // `name` field (an opaque string literal that adds noise).
        "define_action" | "block_action" => {
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
/// Collapses adjacent selectors into a single `Field` or `Selector`
/// with a long path, so `.Values.foo.bar` becomes
/// `Field(["Values","foo","bar"])` rather than three nested nodes.
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
    node.child_by_field_name("arguments")
        .map(|n| collect_argument_list(n, src))
        .unwrap_or_default()
}

/// Source text of `node`'s named-field child, or `""` if absent.
fn field_text<'a>(node: Node<'_>, name: &str, src: &'a str) -> &'a str {
    node.child_by_field_name(name)
        .and_then(|n| n.utf8_text(src.as_bytes()).ok())
        .unwrap_or("")
}

fn collect_argument_list(node: Node<'_>, src: &str) -> Vec<TemplateExpr> {
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for ch in node.named_children(&mut cursor) {
        args.push(convert_pipeline(ch, src));
    }
    args
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
mod tests {
    use super::*;

    fn first(exprs: &[TemplateExpr]) -> &TemplateExpr {
        exprs.first().expect("at least one expression")
    }

    #[test]
    fn parses_real_include_call() {
        let exprs = parse_action_expressions(r#"{{ include "common.labels" . }}"#);
        match first(&exprs) {
            TemplateExpr::Call { function, args } => {
                assert_eq!(function, "include");
                assert_eq!(args.len(), 2);
                assert_eq!(
                    args[0],
                    TemplateExpr::Literal(Literal::String("common.labels".into()))
                );
                assert_eq!(args[1], TemplateExpr::Field(Vec::new()));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn parses_template_action_as_call() {
        let exprs = parse_action_expressions(r#"{{ template "common.labels" . }}"#);
        match first(&exprs) {
            TemplateExpr::Call { function, args } => {
                assert_eq!(function, "template");
                assert_eq!(
                    args[0],
                    TemplateExpr::Literal(Literal::String("common.labels".into()))
                );
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn quoted_string_payload_is_a_literal_not_a_call() {
        // The bug class: `"include \"X\""` is a payload string, NOT a
        // call. Walking the parsed tree, we should find ONE Call node
        // (the outer `quote` call) whose first arg is a Literal::String
        // containing the literal text — never a Call to `include`.
        let exprs = parse_action_expressions(r#"{{ "include \"X\"" | quote }}"#);
        let pipeline = first(&exprs);
        let TemplateExpr::Pipeline(stages) = pipeline else {
            panic!("expected Pipeline, got {pipeline:?}");
        };
        let TemplateExpr::Literal(Literal::String(s)) = &stages[0] else {
            panic!("expected String literal as stage 0, got {:?}", stages[0]);
        };
        assert_eq!(s, r#"include "X""#);

        // Confirm no Call to include exists anywhere in the tree.
        let mut saw_include = false;
        pipeline.walk(|e| {
            if let TemplateExpr::Call { function, .. } = e
                && function == "include"
            {
                saw_include = true;
            }
        });
        assert!(!saw_include, "phantom Call to include leaked through");
    }

    #[test]
    fn parses_default_literal_dotvalues_prefix_form() {
        let exprs = parse_action_expressions(r#"{{ default 5 .Values.replicas }}"#);
        match first(&exprs) {
            TemplateExpr::Call { function, args } => {
                assert_eq!(function, "default");
                assert_eq!(args[0], TemplateExpr::Literal(Literal::Int(5)));
                assert_eq!(
                    args[1],
                    TemplateExpr::Field(vec!["Values".into(), "replicas".into()])
                );
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn parses_pipeline_form_dotvalues_default() {
        let exprs = parse_action_expressions(r#"{{ .Values.replicas | default 5 }}"#);
        match first(&exprs) {
            TemplateExpr::Pipeline(stages) => {
                assert_eq!(stages.len(), 2);
                assert_eq!(
                    stages[0],
                    TemplateExpr::Field(vec!["Values".into(), "replicas".into()])
                );
                let TemplateExpr::Call { function, args } = &stages[1] else {
                    panic!("expected default call in stage 1");
                };
                assert_eq!(function, "default");
                assert_eq!(args, &vec![TemplateExpr::Literal(Literal::Int(5))]);
            }
            other => panic!("expected Pipeline, got {other:?}"),
        }
    }

    #[test]
    fn skips_comment_action() {
        let exprs = parse_action_expressions(r#"{{/* include "fake" */}}{{ include "real" . }}"#);
        // We collect comment actions as either nothing (skipped) or
        // they don't produce an extracted call. The only Call we should
        // see is "include" with arg "real".
        let mut include_args: Vec<String> = Vec::new();
        for e in &exprs {
            e.walk(|child| {
                if let TemplateExpr::Call { function, args } = child
                    && function == "include"
                    && let Some(TemplateExpr::Literal(Literal::String(name))) = args.first()
                {
                    include_args.push(name.clone());
                }
            });
        }
        assert_eq!(include_args, vec!["real".to_string()]);
    }

    #[test]
    fn flattens_nested_control_flow_bodies() {
        let body = r#"{{ if .X }}{{ include "a" . }}{{ end }}{{ include "b" . }}"#;
        let exprs = parse_action_expressions(body);
        let mut include_names: Vec<String> = Vec::new();
        for e in &exprs {
            e.walk(|child| {
                if let TemplateExpr::Call { function, args } = child
                    && function == "include"
                    && let Some(TemplateExpr::Literal(Literal::String(name))) = args.first()
                {
                    include_names.push(name.clone());
                }
            });
        }
        assert_eq!(include_names, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn raw_string_literal_decoded_verbatim() {
        let exprs = parse_action_expressions("{{ `a\\nb` }}");
        // Raw string contents are NOT escape-decoded.
        match first(&exprs) {
            TemplateExpr::Literal(Literal::RawString(s)) => {
                assert_eq!(s, "a\\nb");
            }
            other => panic!("expected RawString, got {other:?}"),
        }
    }

    #[test]
    fn variable_selector_chain() {
        // `$root.Values.foo` — Selector with a Variable operand.
        let exprs = parse_action_expressions(r#"{{ $root.Values.foo }}"#);
        match first(&exprs) {
            TemplateExpr::Selector { operand, path } => {
                assert_eq!(**operand, TemplateExpr::Variable("root".into()));
                assert_eq!(path, &vec!["Values".to_string(), "foo".to_string()]);
            }
            other => panic!("expected Selector, got {other:?}"),
        }
    }

    #[test]
    fn deep_field_chain_collapses_to_single_field() {
        // `.A.B.C.D.E` — nested `selector_expression`s. Should collapse
        // into a single `Field` with the full path, NOT a five-deep
        // Selector chain.
        let exprs = parse_action_expressions(r#"{{ .A.B.C.D.E }}"#);
        assert_eq!(
            first(&exprs),
            &TemplateExpr::Field(vec![
                "A".into(),
                "B".into(),
                "C".into(),
                "D".into(),
                "E".into()
            ]),
        );
    }

    #[test]
    fn bare_dot_parses_as_empty_field_path() {
        let exprs = parse_action_expressions(r#"{{ . }}"#);
        assert_eq!(exprs, vec![TemplateExpr::Field(Vec::new())]);
    }

    #[test]
    fn pipeline_with_intervening_call_no_default_match() {
        // `.Values.X | upper | default 5` — the windows pattern matcher
        // should NOT pair `.Values.X` with `default` because `upper`
        // intervenes (so `default` fires on `upper(.Values.X)`, not on
        // `.Values.X`). Only direct `.Values.X | default` pairings are
        // legal type-hint sites.
        let exprs = parse_action_expressions(r#"{{ .Values.X | upper | default 5 }}"#);
        let TemplateExpr::Pipeline(stages) = first(&exprs) else {
            panic!("expected pipeline");
        };
        assert_eq!(stages.len(), 3);
        // First adjacent pair (Field, Call("upper")) — not a default.
        assert!(matches!(&stages[1], TemplateExpr::Call { function, .. } if function == "upper"));
        // Second adjacent pair (Call("upper"), Call("default")) — first
        // half is not a Field, so consumers should reject.
        assert!(matches!(&stages[2], TemplateExpr::Call { function, .. } if function == "default"),);
    }

    #[test]
    fn range_destructure_walks_range_expression_not_variables() {
        // `{{ range $i, $v := include "common.items" . }}{{ end }}`
        // — the range_variable_definition's `range` field carries an
        // include call. Walking the parsed actions should surface the
        // include Call so helper-call extraction works inside range
        // headers.
        let body = r#"{{ range $i, $v := include "common.items" . }}{{ end }}"#;
        let exprs = parse_action_expressions(body);
        let mut found = false;
        for e in &exprs {
            e.walk(|child| {
                if let TemplateExpr::Call { function, args } = child
                    && function == "include"
                    && let Some(TemplateExpr::Literal(Literal::String(name))) = args.first()
                    && name == "common.items"
                {
                    found = true;
                }
            });
        }
        assert!(
            found,
            "include inside range destructure not extracted: {exprs:?}"
        );
    }

    #[test]
    fn define_name_is_not_surfaced_as_top_level_literal() {
        // `{{ define "common.name" }}{{ include "X" . }}{{ end }}` —
        // the define's name `"common.name"` must NOT appear in the
        // top-level expression list as a stray Literal. Only the body's
        // `include` call should surface.
        let body = r#"{{ define "common.name" }}{{ include "X" . }}{{ end }}"#;
        let exprs = parse_action_expressions(body);
        // Filter: name literals are forbidden at top level.
        let strays: Vec<_> = exprs
            .iter()
            .filter(
                |e| matches!(e, TemplateExpr::Literal(Literal::String(s)) if s == "common.name"),
            )
            .collect();
        assert!(
            strays.is_empty(),
            "define name leaked as top-level Literal: {strays:?}",
        );
        // Sanity: the include call IS surfaced.
        let mut saw_include = false;
        for e in &exprs {
            e.walk(|c| {
                if let TemplateExpr::Call { function, .. } = c
                    && function == "include"
                {
                    saw_include = true;
                }
            });
        }
        assert!(saw_include, "include inside define body not extracted");
    }

    #[test]
    fn block_name_is_not_surfaced_but_argument_is() {
        // `{{ block "name" .Values.X }}…{{ end }}` — the block's `name`
        // string is noise; the `.Values.X` argument and body content
        // must still be reachable.
        let body = r#"{{ block "thing" .Values.X }}body{{ end }}"#;
        let exprs = parse_action_expressions(body);
        let strays: Vec<_> = exprs
            .iter()
            .filter(|e| matches!(e, TemplateExpr::Literal(Literal::String(s)) if s == "thing"))
            .collect();
        assert!(strays.is_empty(), "block name leaked: {strays:?}");
        let mut saw_arg = false;
        for e in &exprs {
            e.walk(|c| {
                if let TemplateExpr::Field(path) = c
                    && path == &vec!["Values".to_string(), "X".to_string()]
                {
                    saw_arg = true;
                }
            });
        }
        assert!(saw_arg, "block argument .Values.X not extracted: {exprs:?}");
    }

    #[test]
    fn malformed_unicode_escape_is_preserved_verbatim() {
        // `\u12` is only two hex digits — Go's grammar requires four.
        // The decoder must not silently produce a wrong char; the input
        // bytes are preserved verbatim so callers can detect the issue.
        let exprs = parse_action_expressions(r#"{{ "\u12" }}"#);
        let TemplateExpr::Literal(Literal::String(s)) = first(&exprs) else {
            panic!("expected string literal");
        };
        assert_eq!(s, r"\u12", "got {s:?}");
    }

    #[test]
    fn well_formed_unicode_escapes_decode_correctly() {
        // `é` → 'é'. `\U0001F600` → '😀' (supplementary plane).
        let exprs = parse_action_expressions(r#"{{ "café \U0001F600" }}"#);
        let TemplateExpr::Literal(Literal::String(s)) = first(&exprs) else {
            panic!("expected string literal");
        };
        assert_eq!(s, "café 😀");
    }

    #[test]
    fn surrogate_code_point_is_not_silently_decoded() {
        // `\uD800` is the leading half of a UTF-16 surrogate pair —
        // not a valid Unicode scalar value. `char::from_u32` returns
        // None, so the decoder must preserve the raw escape bytes.
        let exprs = parse_action_expressions(r#"{{ "\uD800" }}"#);
        let TemplateExpr::Literal(Literal::String(s)) = first(&exprs) else {
            panic!("expected string literal");
        };
        assert_eq!(s, r"\uD800");
    }

    #[test]
    fn empty_body_returns_empty_list() {
        assert!(parse_action_expressions("").is_empty());
    }

    #[test]
    fn body_with_no_actions_returns_empty() {
        assert!(parse_action_expressions("just plain yaml text\nkey: value\n").is_empty());
    }

    #[test]
    fn negative_int_literal() {
        let exprs = parse_action_expressions(r#"{{ default -42 .Values.X }}"#);
        let TemplateExpr::Call { args, .. } = first(&exprs) else {
            panic!("expected Call");
        };
        assert_eq!(args[0], TemplateExpr::Literal(Literal::Int(-42)));
    }

    #[test]
    fn hex_int_literal() {
        let exprs = parse_action_expressions(r#"{{ default 0xFF .Values.X }}"#);
        let TemplateExpr::Call { args, .. } = first(&exprs) else {
            panic!("expected Call");
        };
        assert_eq!(args[0], TemplateExpr::Literal(Literal::Int(0xFF)));
    }
}
