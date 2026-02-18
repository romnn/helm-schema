use tree_sitter::{Parser, Tree};

// Build sanitized YAML by concatenating all `text` nodes from the gotmpl tree.
pub fn sanitize_yaml_from_gotmpl_text_nodes(gotmpl_tree: &tree_sitter::Tree, src: &str) -> String {
    let mut out = String::new();
    let root = gotmpl_tree.root_node();

    // DFS in document order; append every `text` node verbatim
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "text" {
            let r = node.byte_range();
            out.push_str(&src[r]);
        }
        let mut c = node.walk();
        // push children in reverse so we pop in order
        let kids: Vec<_> = node.children(&mut c).collect();
        for ch in kids.into_iter().rev() {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }
    out
}

pub fn sanitize_yaml_for_parse_from_gotmpl(
    text: &str,
    ranges: &[std::ops::Range<usize>],
) -> String {
    // Replace each template range with a YAML-safe placeholder of the same length to keep offsets.
    // We’ll use spaces/newlines to preserve indentation/structure as much as possible.
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for r in ranges {
        if r.start > last {
            out.push_str(&text[last..r.start]);
        }
        // Keep newlines, blank out other chars so YAML indentation stays similar.
        for b in text.as_bytes()[r.start..r.end].iter() {
            match *b {
                b'\n' => out.push('\n'),
                _ => out.push(' '),
            }
        }
        last = r.end;
    }
    if last < text.len() {
        out.push_str(&text[last..]);
    }
    out
}

pub struct YamlDoc {
    pub tree: Tree,
    pub sanitized: String,
}

pub fn parse_yaml_sanitized(src: &str) -> Option<YamlDoc> {
    let mut parser = Parser::new();
    let language = tree_sitter::Language::new(helm_schema_template_grammar::yaml::language());
    parser.set_language(&language).ok()?;
    let tree = parser.parse(src, None)?;
    Some(YamlDoc {
        tree,
        sanitized: src.to_string(),
    })
}
