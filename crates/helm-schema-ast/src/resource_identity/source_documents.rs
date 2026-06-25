use crate::parse_helm_template;

pub(super) fn document_spans(source: &str) -> Vec<(usize, usize)> {
    let Some(tree) = parse_helm_template(source) else {
        return whole_source_span(source);
    };

    let root = tree.root_node();
    let mut docs = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.is_named() && child.kind() == "document" {
            docs.push((child.start_byte(), child.end_byte()));
        }
    }
    if docs.is_empty() {
        return whole_source_span(source);
    }
    docs.sort_by_key(|(start, _)| *start);
    for index in 0..docs.len() {
        let end = docs
            .get(index + 1)
            .map(|(next_start, _)| *next_start)
            .unwrap_or(source.len());
        docs[index].1 = end;
    }
    docs
}

fn whole_source_span(source: &str) -> Vec<(usize, usize)> {
    if source.is_empty() {
        Vec::new()
    } else {
        vec![(0, source.len())]
    }
}
