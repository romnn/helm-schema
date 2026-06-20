use crate::{HelmAst, HelmParser, ParseError, TemplateAction, TemplateExpr, TemplateHeader};

/// Parser implementation backed by the tree-sitter fused Helm+YAML grammar.
///
/// Uses the `go_template` grammar for top-level structure and the
/// `helm_template` grammar for re-parsing YAML fragments within text nodes.
pub struct TreeSitterParser;

/// Fused template parse result plus syntax-level metadata.
pub struct ParsedTemplate {
    pub ast: HelmAst,
    pub contains_template_action: bool,
}

/// Return whether the source contains any Helm/Go-template action.
///
/// This is a syntax-level check over the template grammar. Callers that only
/// accept literal YAML can use it to abstain before handing source text to a
/// YAML parser.
#[tracing::instrument(skip_all, fields(bytes = src.len()))]
pub fn contains_template_action(src: &str) -> Result<bool, ParseError> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .map_err(|_| ParseError::TreeSitterParseFailed)?;

    let tree = parser
        .parse(src, None)
        .ok_or(ParseError::TreeSitterParseFailed)?;

    Ok(node_contains_template_action(tree.root_node()))
}

fn node_contains_template_action(node: tree_sitter::Node<'_>) -> bool {
    if is_template_action_node(node.kind()) {
        return true;
    }

    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(node_contains_template_action)
}

fn is_template_action_node(kind: &str) -> bool {
    is_template_delim_start(kind)
        || is_template_delim_end(kind)
        || matches!(
            kind,
            "template_action"
                | "if_action"
                | "else_action"
                | "range_action"
                | "with_action"
                | "define_action"
                | "block_action"
                | "end_action"
        )
}

fn is_template_delim_start(kind: &str) -> bool {
    kind == "{{" || kind == "{{-"
}

fn is_fragment_injector_action(action: &TemplateAction) -> bool {
    action
        .exprs()
        .iter()
        .any(template_expr_is_fragment_injector)
}

fn template_expr_is_fragment_injector(expr: &TemplateExpr) -> bool {
    match expr.deparen() {
        TemplateExpr::Call { function, .. } => matches!(
            function.as_str(),
            "include" | "template" | "tpl" | "toYaml" | "fromYaml" | "indent" | "nindent"
        ),
        TemplateExpr::Pipeline(stages) => stages.iter().any(template_expr_is_fragment_injector),
        TemplateExpr::Parenthesized(inner) => template_expr_is_fragment_injector(inner),
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Selector { .. }
        | TemplateExpr::Variable(_)
        | TemplateExpr::VariableDefinition { .. }
        | TemplateExpr::Assignment { .. }
        | TemplateExpr::Unknown(_) => false,
    }
}

fn is_template_delim_end(kind: &str) -> bool {
    kind == "}}" || kind == "-}}"
}

fn is_standalone_span(start: usize, end: usize, src: &str) -> bool {
    let bytes = src.as_bytes();
    let start = start.min(src.len());
    let end = end.min(src.len());

    let mut line_start = start;
    while line_start > 0 {
        if bytes[line_start - 1] == b'\n' {
            break;
        }
        line_start -= 1;
    }

    let mut line_end = end;
    while line_end < bytes.len() {
        if bytes[line_end] == b'\n' {
            break;
        }
        line_end += 1;
    }

    let prefix = &src[line_start..start];
    let suffix = &src[end..line_end];

    prefix.chars().all(|c| c == ' ' || c == '\t' || c == '\r')
        && suffix.chars().all(|c| c == ' ' || c == '\t' || c == '\r')
}

#[derive(Clone, Debug)]
struct DeindentedYamlFragment {
    text: String,
    base_indent: usize,
}

impl HelmParser for TreeSitterParser {
    #[tracing::instrument(skip_all, fields(bytes = src.len()))]
    fn parse(&self, src: &str) -> Result<HelmAst, ParseError> {
        self.parse_with_metadata(src).map(|parsed| parsed.ast)
    }
}

impl TreeSitterParser {
    #[tracing::instrument(skip_all, fields(bytes = src.len()))]
    pub fn parse_with_metadata(&self, src: &str) -> Result<ParsedTemplate, ParseError> {
        let language =
            tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .map_err(|_| ParseError::TreeSitterParseFailed)?;

        let tree = parser
            .parse(src, None)
            .ok_or(ParseError::TreeSitterParseFailed)?;

        let root = tree.root_node();
        let contains_template_action = node_contains_template_action(root);
        let mut blocks = Vec::new();
        let mut c = root.walk();
        for ch in root.children(&mut c) {
            blocks.push(ch);
        }
        let items = fuse_blocks(&blocks, src, false);
        Ok(ParsedTemplate {
            ast: HelmAst::Document { items },
            contains_template_action,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal: fuse tree-sitter nodes into HelmAst
// ---------------------------------------------------------------------------

fn is_control_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_action" | "range_action" | "with_action" | "define_action" | "block_action"
    )
}

fn is_standalone_template_action(node: tree_sitter::Node<'_>, src: &str) -> bool {
    if node.kind() != "template_action" {
        return false;
    }
    is_standalone_span(node.start_byte(), node.end_byte(), src)
}

fn children_with_field<'a>(node: tree_sitter::Node<'a>, field: &str) -> Vec<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor).collect()
}

fn normalize_helm_template_text(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(rest) = s.strip_prefix("{{") {
        s = rest;
    }
    s = s.strip_prefix('-').unwrap_or(s);
    s = s.trim_start();
    if let Some(rest) = s.strip_suffix("}}") {
        s = rest;
    }
    s = s.strip_suffix('-').unwrap_or(s);
    s.trim().to_string()
}

fn deindent_yaml_fragment(fragment: &str) -> String {
    deindent_yaml_fragment_with_base(fragment).text
}

fn deindent_yaml_fragment_with_base(fragment: &str) -> DeindentedYamlFragment {
    let mut min_indent: Option<usize> = None;
    for line in fragment.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            continue;
        }
        if content.trim_start().starts_with("{{") {
            continue;
        }

        let indent = content
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        min_indent = Some(match min_indent {
            None => indent,
            Some(prev) => prev.min(indent),
        });
    }

    let Some(min_indent) = min_indent else {
        return DeindentedYamlFragment {
            text: fragment.to_string(),
            base_indent: 0,
        };
    };

    let mut out = String::with_capacity(fragment.len());
    for line in fragment.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            out.push_str(line);
            continue;
        }

        let mut removed = 0usize;
        let mut idx = 0usize;
        for ch in line.chars() {
            if removed >= min_indent {
                break;
            }
            if ch == ' ' || ch == '\t' {
                removed += 1;
                idx += ch.len_utf8();
                continue;
            }
            break;
        }
        out.push_str(&line[idx..]);
    }
    DeindentedYamlFragment {
        text: out,
        base_indent: min_indent,
    }
}

fn parse_helm_template_tree(src: &str) -> Option<tree_sitter::Tree> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::helm_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(src, None)
}

fn last_relevant_named_child(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| {
            child.is_named()
                && !matches!(
                    child.kind(),
                    "reserved_directive" | "tag_directive" | "yaml_directive" | "comment"
                )
        })
        .last()
}

fn trailing_open_mapping_value_indent(node: tree_sitter::Node<'_>) -> Option<usize> {
    match node.kind() {
        "document"
        | "block_node"
        | "flow_node"
        | "block_mapping"
        | "flow_mapping"
        | "block_sequence"
        | "flow_sequence"
        | "block_sequence_item" => {
            last_relevant_named_child(node).and_then(trailing_open_mapping_value_indent)
        }
        "block_mapping_pair" | "flow_pair" => {
            let value = node.child_by_field_name("value");
            match value.and_then(trailing_open_mapping_value_indent) {
                Some(indent) => Some(indent),
                None if value.is_none() => node
                    .child_by_field_name("key")
                    .map(|key| key.start_position().column),
                None => None,
            }
        }
        _ => last_relevant_named_child(node).and_then(trailing_open_mapping_value_indent),
    }
}

fn action_continues_pending_yaml_value(pending: &str, action_indent: usize) -> bool {
    let pending = deindent_yaml_fragment_with_base(pending);
    if pending.text.trim().is_empty() {
        return false;
    }

    let normalized_indent = action_indent.saturating_sub(pending.base_indent);
    let Some(tree) = parse_helm_template_tree(&pending.text) else {
        return false;
    };
    let Some(indent) = trailing_open_mapping_value_indent(tree.root_node()) else {
        return false;
    };
    normalized_indent > indent
}

/// Re-parse a YAML fragment using the fused `helm_template` grammar and convert to `HelmAst` nodes.
fn parse_yaml_items(src: &str) -> Vec<HelmAst> {
    let src = deindent_yaml_fragment(src);
    if src.trim().is_empty() {
        return vec![];
    }

    let Some(tree) = parse_helm_template_tree(&src) else {
        return vec![];
    };

    let root = tree.root_node();
    let mut out = Vec::new();

    let mut cursor = root.walk();
    for doc in root.children(&mut cursor) {
        if !doc.is_named() || doc.kind() != "document" {
            continue;
        }

        let mut dc = doc.walk();
        for ch in doc.children(&mut dc) {
            if !ch.is_named() {
                continue;
            }
            if matches!(
                ch.kind(),
                "reserved_directive" | "tag_directive" | "yaml_directive"
            ) {
                continue;
            }
            out.push(yaml_node_to_ast(ch, &src));
        }
    }

    out
}

/// Convert a single tree-sitter YAML/Helm node into `HelmAst`.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
fn yaml_node_to_ast(node: tree_sitter::Node<'_>, src: &str) -> HelmAst {
    match node.kind() {
        "block_node" | "flow_node" => {
            if let Some(ch) = node.named_child(0) {
                yaml_node_to_ast(ch, src)
            } else {
                HelmAst::Scalar {
                    text: String::new(),
                }
            }
        }
        "document" => {
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if !ch.is_named() {
                    continue;
                }
                if matches!(
                    ch.kind(),
                    "reserved_directive" | "tag_directive" | "yaml_directive"
                ) {
                    continue;
                }
                kids.push(yaml_node_to_ast(ch, src));
            }
            HelmAst::Document { items: kids }
        }
        "block_mapping" | "flow_mapping" => {
            let pair_kind = if node.kind() == "block_mapping" {
                "block_mapping_pair"
            } else {
                "flow_pair"
            };
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() && ch.kind() == pair_kind {
                    kids.push(yaml_node_to_ast(ch, src));
                }
            }
            HelmAst::Mapping { items: kids }
        }
        "block_mapping_pair" | "flow_pair" => {
            let key = node.child_by_field_name("key").map_or_else(
                || {
                    Box::new(HelmAst::Scalar {
                        text: String::new(),
                    })
                },
                |n| Box::new(yaml_node_to_ast(n, src)),
            );
            let value = node
                .child_by_field_name("value")
                .map(|n| Box::new(yaml_node_to_ast(n, src)));

            HelmAst::Pair { key, value }
        }
        "block_sequence" | "flow_sequence" => {
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if !ch.is_named() {
                    continue;
                }
                if matches!(ch.kind(), "block_sequence_item" | "flow_node" | "flow_pair") {
                    kids.push(yaml_node_to_ast(ch, src));
                }
            }
            HelmAst::Sequence { items: kids }
        }
        "block_sequence_item" => {
            if let Some(ch) = node.named_child(0) {
                yaml_node_to_ast(ch, src)
            } else {
                HelmAst::Scalar {
                    text: String::new(),
                }
            }
        }
        "plain_scalar" => {
            if let Some(ch) = node.named_child(0) {
                yaml_node_to_ast(ch, src)
            } else {
                HelmAst::Scalar {
                    text: node
                        .utf8_text(src.as_bytes())
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                }
            }
        }
        "string_scalar" | "block_scalar" | "integer_scalar" | "float_scalar" | "boolean_scalar" => {
            HelmAst::Scalar {
                text: node
                    .utf8_text(src.as_bytes())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            }
        }
        "double_quote_scalar" | "single_quote_scalar" => {
            let raw = node.utf8_text(src.as_bytes()).unwrap_or("").trim();
            let text = if (raw.starts_with('"') && raw.ends_with('"'))
                || (raw.starts_with('\'') && raw.ends_with('\''))
            {
                raw[1..raw.len() - 1].to_string()
            } else {
                raw.to_string()
            };
            HelmAst::Scalar { text }
        }
        "null_scalar" => HelmAst::Scalar {
            text: String::new(),
        },
        "helm_template" => {
            let text = node.utf8_text(src.as_bytes()).unwrap_or("");
            HelmAst::HelmExpr {
                action: TemplateAction::parse(normalize_helm_template_text(text)),
            }
        }
        _ => {
            // Generic fallback: recurse into named children.
            let mut kids = Vec::new();
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() {
                    kids.push(yaml_node_to_ast(ch, src));
                }
            }
            if kids.is_empty() {
                HelmAst::Scalar {
                    text: node
                        .utf8_text(src.as_bytes())
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                }
            } else if kids.len() == 1 {
                kids.into_iter().next().unwrap()
            } else {
                HelmAst::Document { items: kids }
            }
        }
    }
}

/// Fuse a sequence of tree-sitter top-level nodes (text + control flow) into `HelmAst` nodes.
#[allow(clippy::too_many_lines)]
fn fuse_blocks(blocks: &[tree_sitter::Node<'_>], src: &str, in_control_flow: bool) -> Vec<HelmAst> {
    let mut out: Vec<HelmAst> = Vec::new();
    let mut pending = String::new();

    let flush_pending = |pending: &mut String, out: &mut Vec<HelmAst>| {
        if pending.trim().is_empty() {
            pending.clear();
            return;
        }
        let fragment = std::mem::take(pending);
        out.extend(parse_yaml_items(&fragment));
    };

    let mut i = 0usize;
    while i < blocks.len() {
        let b = blocks[i];

        if is_template_delim_start(b.kind()) {
            let mut j = i + 1;
            while j < blocks.len() {
                if is_template_delim_end(blocks[j].kind()) {
                    break;
                }
                j += 1;
            }

            if j < blocks.len() {
                let start = blocks[i].start_byte();
                let end = blocks[j].end_byte();
                if is_standalone_span(start, end, src) {
                    let comment_node = (i + 1..j)
                        .filter_map(|k| blocks.get(k).copied())
                        .find(|n| n.is_named() && n.kind() == "comment");
                    if let Some(comment_node) = comment_node {
                        flush_pending(&mut pending, &mut out);
                        let comment_text = comment_node.utf8_text(src.as_bytes()).unwrap_or("");
                        out.push(HelmAst::HelmComment {
                            text: comment_text.to_string(),
                        });
                        i = j + 1;
                        continue;
                    }

                    let action_indent = blocks[i].start_position().column;
                    let span_text = &src[start.min(src.len())..end.min(src.len())];
                    let normalized = normalize_helm_template_text(span_text);
                    let action = TemplateAction::parse(normalized);
                    let is_yaml_value_continuation =
                        action_continues_pending_yaml_value(&pending, action_indent);

                    if !is_yaml_value_continuation
                        && !in_control_flow
                        && action_indent > 0
                        && is_fragment_injector_action(&action)
                    {
                        i = j + 1;
                        continue;
                    }

                    if !is_yaml_value_continuation {
                        flush_pending(&mut pending, &mut out);
                        out.push(HelmAst::HelmExpr { action });
                        i = j + 1;
                        continue;
                    }
                }
            }
        }

        // Detect comment actions: `{{` + `comment` node + `}}`
        if (b.kind() == "{{" || b.kind() == "{{-")
            && blocks
                .get(i + 1)
                .is_some_and(|n| n.is_named() && n.kind() == "comment")
        {
            flush_pending(&mut pending, &mut out);

            let comment_node = blocks[i + 1];
            let comment_text = comment_node.utf8_text(src.as_bytes()).unwrap_or("");
            out.push(HelmAst::HelmComment {
                text: comment_text.to_string(),
            });

            i += 2;

            // Skip whitespace tokens before closing delimiter.
            while i < blocks.len()
                && !blocks[i].is_named()
                && blocks[i].kind().chars().all(char::is_whitespace)
            {
                i += 1;
            }

            // Skip closing delimiter.
            if i < blocks.len() && (blocks[i].kind() == "}}" || blocks[i].kind() == "-}}") {
                i += 1;
            }
            continue;
        }

        if is_control_flow(b.kind()) {
            flush_pending(&mut pending, &mut out);
            out.push(fuse_control_flow(b, src));
        } else if is_standalone_template_action(b, src) {
            let action_indent = b.start_position().column;
            let is_yaml_value_continuation =
                action_continues_pending_yaml_value(&pending, action_indent);
            let text = b.utf8_text(src.as_bytes()).unwrap_or("");
            let normalized = normalize_helm_template_text(text);
            let action = TemplateAction::parse(normalized);

            if !is_yaml_value_continuation
                && !in_control_flow
                && action_indent > 0
                && is_fragment_injector_action(&action)
            {
                // Skip top-level fragment injectors; they typically expand to YAML.
            } else if is_yaml_value_continuation {
                let r = b.byte_range();
                pending.push_str(&src[r]);
            } else {
                flush_pending(&mut pending, &mut out);
                out.push(HelmAst::HelmExpr { action });
            }
        } else {
            let r = b.byte_range();
            pending.push_str(&src[r]);
        }

        i += 1;
    }

    flush_pending(&mut pending, &mut out);
    out
}

/// Convert a tree-sitter control-flow node into `HelmAst`.
#[allow(clippy::too_many_lines)]
fn fuse_control_flow(node: tree_sitter::Node<'_>, src: &str) -> HelmAst {
    match node.kind() {
        "if_action" => {
            let cond_text = node
                .child_by_field_name("condition")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .to_string();
            let condition = TemplateHeader::parse_control(cond_text);

            let then_blocks = children_with_field(node, "consequence");
            let else_blocks = children_with_field(node, "alternative");

            let then_items = fuse_blocks(&then_blocks, src, true);
            let base_else_items = fuse_blocks(&else_blocks, src, true);

            // Handle `else if` chains: tree-sitter inlines them as repeated
            // condition/option fields. We lower them into nested If nodes.
            let mut else_if_pairs: Vec<(TemplateHeader, Vec<tree_sitter::Node<'_>>)> = Vec::new();
            let mut seen_main_condition = false;
            let mut walker = node.walk();
            if walker.goto_first_child() {
                loop {
                    let ch = walker.node();
                    match walker.field_name() {
                        Some("condition") => {
                            if seen_main_condition {
                                let cnd = ch
                                    .utf8_text(src.as_bytes())
                                    .unwrap_or("")
                                    .trim()
                                    .to_string();
                                else_if_pairs
                                    .push((TemplateHeader::parse_control(cnd), Vec::new()));
                            } else {
                                seen_main_condition = true;
                            }
                        }
                        Some("option") => {
                            if let Some((_, blocks)) = else_if_pairs.last_mut() {
                                blocks.push(ch);
                            }
                        }
                        _ => {}
                    }
                    if !walker.goto_next_sibling() {
                        break;
                    }
                }
            }

            let else_items = if else_if_pairs.is_empty() {
                base_else_items
            } else {
                let mut tail = base_else_items;
                for (condition, blocks) in else_if_pairs.into_iter().rev() {
                    let opt_items = fuse_blocks(&blocks, src, true);
                    tail = vec![HelmAst::If {
                        condition,
                        then_branch: opt_items,
                        else_branch: tail,
                    }];
                }
                tail
            };

            HelmAst::If {
                condition,
                then_branch: then_items,
                else_branch: else_items,
            }
        }
        "range_action" => {
            // For `range .Values.X`, extract the pipeline directly.
            // For `range $key, $value := .Values.X`, extract the full
            // variable definition text to preserve binding information.
            let header = {
                let mut c = node.walk();
                let rvd = node
                    .children(&mut c)
                    .find(|ch| ch.kind() == "range_variable_definition");
                if let Some(rvd) = rvd {
                    rvd.utf8_text(src.as_bytes())
                        .unwrap_or("")
                        .trim()
                        .to_string()
                } else {
                    node.child_by_field_name("range")
                        .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                        .unwrap_or("")
                        .trim()
                        .to_string()
                }
            };
            let header = TemplateHeader::parse_range(header);

            let body = fuse_blocks(&children_with_field(node, "body"), src, true);
            let else_branch = fuse_blocks(&children_with_field(node, "alternative"), src, true);

            HelmAst::Range {
                header,
                body,
                else_branch,
            }
        }
        "with_action" => {
            let header = node
                .child_by_field_name("condition")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .to_string();
            let header = TemplateHeader::parse_control(header);

            let body = fuse_blocks(&children_with_field(node, "consequence"), src, true);
            let else_branch = fuse_blocks(&children_with_field(node, "alternative"), src, true);

            HelmAst::With {
                header,
                body,
                else_branch,
            }
        }
        "define_action" => {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .to_string();

            let body = fuse_blocks(&children_with_field(node, "body"), src, true);

            HelmAst::Define { name, body }
        }
        "block_action" => {
            let name_part = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim();
            let arg_part = node
                .child_by_field_name("argument")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .unwrap_or("")
                .trim();

            let name = if arg_part.is_empty() {
                name_part.trim_matches('"').to_string()
            } else {
                format!("{} {}", name_part.trim_matches('"'), arg_part)
            };

            let body = fuse_blocks(&children_with_field(node, "body"), src, true);

            HelmAst::Block { name, body }
        }
        _ => HelmAst::Scalar {
            text: node
                .utf8_text(src.as_bytes())
                .unwrap_or("")
                .trim()
                .to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::action_continues_pending_yaml_value;

    #[test]
    fn open_mapping_key_continues_with_structural_fragment_indent() {
        let pending = "metadata:\n  labels:\n";
        assert!(action_continues_pending_yaml_value(pending, 4));
        assert!(!action_continues_pending_yaml_value(pending, 2));
    }

    #[test]
    fn open_mapping_key_continues_past_comment_line() {
        let pending = "metadata:\n  labels:\n  # chart adds labels here\n";
        assert!(action_continues_pending_yaml_value(pending, 4));
    }
}
