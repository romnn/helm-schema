//! Direct tests of the open-slot query API: these pin the slot semantics
//! (paths, mapping-key detection, whole-vs-partial scalars, block-scalar
//! suppression, fragment insertion slots) independently of the downstream
//! attribution and IR layers.

use helm_schema_syntax::{PathSegment, TemplatedDocument};
use test_util::prelude::sim_assert_eq;

/// Fold segments with the consumer convention: `[*]` appends to the
/// enclosing key (collapsing repeats), or stands alone at the root.
fn keys(segments: &[PathSegment]) -> Vec<String> {
    let mut folded: Vec<String> = Vec::new();
    for segment in segments {
        match segment {
            PathSegment::Key(key) => folded.push(key.clone()),
            PathSegment::Item => match folded.last_mut() {
                Some(last) if !last.ends_with("[*]") => last.push_str("[*]"),
                Some(_) => {}
                None => folded.push("[*]".to_string()),
            },
        }
    }
    folded
}

fn action_span(source: &str, occurrence: usize) -> (usize, usize) {
    let mut from = 0;
    for _ in 0..=occurrence {
        let start = source[from..]
            .find("{{")
            .map(|offset| from + offset)
            .unwrap_or_else(|| panic!("missing action occurrence in source"));
        from = start + 2;
    }
    let start = from - 2;
    let end = source[start..]
        .find("}}")
        .map(|offset| start + offset + 2)
        .unwrap_or_else(|| panic!("unterminated action in source"));
    (start, end)
}

#[test]
fn whole_and_partial_scalar_slots() {
    let source = "metadata:\n  name: {{ .Values.name }}\n  addr: {{ .Values.a }}:{{ .Values.b }}\n";
    let document = TemplatedDocument::parse(source);

    let whole = action_span(source, 0);
    let context = document.slot_context_at(whole.0, Some(whole));
    sim_assert_eq!(have: keys(&context.path), want: vec!["metadata".to_string(), "name".to_string()]);
    assert!(context.entire_scalar_value);
    assert!(!context.in_mapping_key);

    let partial = action_span(source, 1);
    let context = document.slot_context_at(partial.0, Some(partial));
    sim_assert_eq!(have: keys(&context.path), want: vec!["metadata".to_string(), "addr".to_string()]);
    assert!(!context.entire_scalar_value);
}

#[test]
fn mapping_key_slot() {
    let source = "{{ .Values.key }}: value\n";
    let document = TemplatedDocument::parse(source);
    let span = action_span(source, 0);
    let context = document.slot_context_at(span.0, Some(span));
    assert!(context.in_mapping_key);
    assert!(keys(&context.path).is_empty());
}

#[test]
fn block_scalar_suppression_and_comment_position() {
    let source = "conf: |\n  a={{ .Values.a }}\n# note {{ .Values.b }}\n";
    let document = TemplatedDocument::parse(source);

    let suppressed = action_span(source, 0);
    let context = document.slot_context_at(suppressed.0, Some(suppressed));
    assert!(context.inside_block_scalar);
    sim_assert_eq!(have: keys(&context.path), want: vec!["conf".to_string()]);

    let comment = action_span(source, 1);
    let context = document.slot_context_at(comment.0, Some(comment));
    assert!(context.on_comment_line);
    assert!(!context.inside_block_scalar);
    assert!(keys(&context.path).is_empty());
}

#[test]
fn fragment_slot_at_rendered_indent() {
    let source = "metadata:\n  labels:\n    {{- include \"labels\" . | nindent 4 }}\n";
    let document = TemplatedDocument::parse(source);
    let span = action_span(source, 0);

    let slot = document
        .open_slot_path_before(span.0, 4)
        .unwrap_or_else(|| panic!("expected an open slot"));
    sim_assert_eq!(have: keys(&slot), want: vec!["metadata".to_string(), "labels".to_string()]);

    // At indent 2 the labels slot (same indent, opened empty) still accepts
    // the output; at indent 0 the path is the document root.
    let slot = document
        .open_slot_path_before(span.0, 2)
        .unwrap_or_else(|| panic!("expected an open slot"));
    sim_assert_eq!(have: keys(&slot), want: vec!["metadata".to_string(), "labels".to_string()]);
    sim_assert_eq!(have: document.open_slot_path_before(span.0, 0), want: Some(Vec::new()));
}

#[test]
fn fragment_slot_in_sequence_item() {
    let source = "containers:\n  - name: app\n    {{- toYaml .Values.extra | nindent 4 }}\n";
    let document = TemplatedDocument::parse(source);
    let span = action_span(source, 0);
    let slot = document
        .open_slot_path_before(span.0, 4)
        .unwrap_or_else(|| panic!("expected an open slot"));
    sim_assert_eq!(have: keys(&slot), want: vec!["containers[*]".to_string()]);
}

#[test]
fn inline_fragment_slot_uses_partial_line_prefix() {
    let source = "metadata:\n  labels: {{- include \"labels\" . | nindent 4 }}\n";
    let document = TemplatedDocument::parse(source);
    let span = action_span(source, 0);
    // The prefix `  labels: ` opens the labels slot before the action.
    let slot = document
        .open_slot_path_before(span.0, 4)
        .unwrap_or_else(|| panic!("expected an open slot"));
    sim_assert_eq!(have: keys(&slot), want: vec!["metadata".to_string(), "labels".to_string()]);
}

#[test]
fn sequence_item_context_without_action_stays_on_container() {
    let source = "args:\n  - {{ .Values.a }}\n";
    let document = TemplatedDocument::parse(source);
    let span = action_span(source, 0);

    // Without an action span the item line resolves to the sequence
    // container; with one, to the item slot.
    let container = document.slot_context_at(span.0, None);
    sim_assert_eq!(have: keys(&container.path), want: vec!["args".to_string()]);

    let item = document.slot_context_at(span.0, Some(span));
    sim_assert_eq!(have: keys(&item.path), want: vec!["args[*]".to_string()]);
    assert!(item.entire_scalar_value);
}

#[test]
fn document_spans_split_at_markers() {
    let source = "kind: A\n---\nkind: B\n";
    let document = TemplatedDocument::parse(source);
    let spans: Vec<(usize, usize)> = document
        .document_spans()
        .iter()
        .map(|span| (span.start, span.end))
        .collect();
    sim_assert_eq!(have: spans, want: vec![(0, 8), (12, 20)]);
}
