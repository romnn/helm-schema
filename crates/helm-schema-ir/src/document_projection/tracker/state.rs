use crate::YamlPath;
use crate::yaml_syntax::first_mapping_colon_offset;

use super::yaml_tree::{is_scalar_like, parse_yaml_tree, scalar_text, unwrap_yaml_node};

#[derive(Clone, Debug)]
pub(super) struct DocumentState {
    stack: Vec<StackEntry>,
    at_line_start: bool,
    clear_pending_on_newline_at_indent: Option<usize>,
    block_scalar_parent_indent: Option<usize>,
}

impl Default for DocumentState {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            at_line_start: true,
            clear_pending_on_newline_at_indent: None,
            block_scalar_parent_indent: None,
        }
    }
}

#[derive(Clone, Debug)]
struct StackEntry {
    indent: usize,
    container: Container,
    pending_key: Option<String>,
}

impl StackEntry {
    fn mapping(indent: usize, pending_key: Option<String>) -> Self {
        Self {
            indent,
            container: Container::Mapping,
            pending_key,
        }
    }

    fn sequence(indent: usize) -> Self {
        Self {
            indent,
            container: Container::Sequence,
            pending_key: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Container {
    Mapping,
    Sequence,
}

impl DocumentState {
    pub(super) fn current_path(&self) -> YamlPath {
        let mut out: Vec<String> = Vec::new();
        for entry in &self.stack {
            match (entry.container, entry.pending_key.as_ref()) {
                (Container::Mapping, Some(key)) => {
                    if !key.is_empty() {
                        out.push(key.clone());
                    }
                }
                (Container::Mapping, None) => {}
                (Container::Sequence, _) => {
                    if let Some(last) = out.last_mut() {
                        if !last.ends_with("[*]") {
                            last.push_str("[*]");
                        }
                    } else {
                        out.push("[*]".to_string());
                    }
                }
            }
        }
        YamlPath(out)
    }

    pub(super) fn path_at_mapping_entry_indent(&self, logical_indent: usize) -> YamlPath {
        let mut path = self.current_path();
        for _ in 0..self.trailing_pending_mapping_segments_at_or_above(logical_indent) {
            path.0.pop();
        }
        path
    }

    pub(super) fn sync_action_position(
        &mut self,
        indent: usize,
        col: usize,
        allow_clear_pending: bool,
    ) {
        let effective = std::cmp::max(indent, col);
        if self.is_inside_block_scalar_line(indent) {
            return;
        }
        self.block_scalar_parent_indent = None;

        while self
            .stack
            .last()
            .is_some_and(|entry| entry.indent > effective)
        {
            self.stack.pop();
        }

        if allow_clear_pending && col > indent && self.clear_pending_on_newline_at_indent.is_none()
        {
            let mut candidate: Option<usize> = None;
            for entry in self.stack.iter().rev() {
                if entry.container != Container::Mapping {
                    continue;
                }
                if entry.pending_key.is_none() {
                    continue;
                }
                if entry.indent < indent {
                    break;
                }
                if entry.indent <= col {
                    candidate = Some(entry.indent);
                    break;
                }
            }
            if let Some(indent) = candidate {
                self.clear_pending_on_newline_at_indent = Some(indent);
            }
        }
    }

    pub(super) fn is_inside_block_scalar_line(&self, indent: usize) -> bool {
        self.block_scalar_parent_indent
            .is_some_and(|parent_indent| indent > parent_indent)
    }

    pub(super) fn ingest(&mut self, text: &str) {
        for raw in text.split_inclusive('\n') {
            let is_newline_terminated = raw.ends_with('\n');
            if !self.at_line_start {
                if is_newline_terminated {
                    self.finish_line();
                }
                self.at_line_start = is_newline_terminated;
                continue;
            }

            let line = raw.trim_end_matches('\n');
            let indent = line.chars().take_while(|&ch| ch == ' ').count();
            if self.is_inside_block_scalar_line(indent) {
                self.at_line_start = is_newline_terminated;
                continue;
            }

            let Some(event) = parse_line_event(line) else {
                self.at_line_start = is_newline_terminated;
                continue;
            };

            self.apply_event(event);
            if is_newline_terminated {
                self.finish_line();
            }
            self.at_line_start = is_newline_terminated;
        }
    }

    fn apply_event(&mut self, event: LineEvent) {
        match event {
            LineEvent::DocumentBoundary => {
                self.stack.clear();
                self.block_scalar_parent_indent = None;
                self.clear_pending_on_newline_at_indent = None;
            }
            LineEvent::Mapping(event) => self.apply_mapping(event),
            LineEvent::Sequence(event) => self.apply_sequence(event),
        }
    }

    fn apply_mapping(&mut self, event: MappingEvent) {
        if self.is_inside_block_scalar_line(event.indent) {
            return;
        }
        self.prepare_for_indent(event.indent);

        if let Some(entry) = self.stack.last()
            && entry.indent == event.indent
            && entry.container == Container::Sequence
        {
            self.stack.pop();
        }

        match self.stack.last_mut() {
            Some(entry)
                if entry.indent == event.indent && entry.container == Container::Mapping =>
            {
                entry.pending_key = Some(event.key);
            }
            Some(entry) if entry.indent < event.indent => {
                self.stack
                    .push(StackEntry::mapping(event.indent, Some(event.key)));
            }
            None => {
                self.stack
                    .push(StackEntry::mapping(event.indent, Some(event.key)));
            }
            _ => {}
        }

        if event.starts_block_scalar {
            self.block_scalar_parent_indent = Some(event.indent);
        }

        if event.scalar_value_present {
            self.clear_pending_on_newline_at_indent = Some(event.indent);
        }
    }

    fn apply_sequence(&mut self, event: SequenceEvent) {
        if self.is_inside_block_scalar_line(event.indent) {
            return;
        }
        self.prepare_for_indent(event.indent);

        match self.stack.last_mut() {
            Some(entry)
                if entry.indent == event.indent && entry.container == Container::Sequence =>
            {
                entry.pending_key = None;
            }
            Some(entry)
                if entry.indent == event.indent && entry.container == Container::Mapping =>
            {
                self.stack.push(StackEntry::sequence(event.indent));
            }
            Some(entry) if entry.indent < event.indent => {
                self.stack.push(StackEntry::sequence(event.indent));
            }
            None => {
                self.stack.push(StackEntry::sequence(event.indent));
            }
            _ => {}
        }

        match event.payload {
            SequencePayload::None | SequencePayload::Scalar => {}
            SequencePayload::BlockScalar => {
                self.block_scalar_parent_indent = Some(event.indent);
            }
            SequencePayload::InlineMapping(mapping) => {
                let indent = mapping.indent;
                match self.stack.last_mut() {
                    Some(entry)
                        if entry.indent == indent && entry.container == Container::Mapping =>
                    {
                        entry.pending_key = Some(mapping.key);
                    }
                    Some(entry) if entry.indent < indent => {
                        self.stack
                            .push(StackEntry::mapping(indent, Some(mapping.key)));
                    }
                    None => {
                        self.stack
                            .push(StackEntry::mapping(indent, Some(mapping.key)));
                    }
                    _ => {}
                }

                if mapping.starts_block_scalar {
                    self.block_scalar_parent_indent = Some(indent);
                }

                if mapping.scalar_value_present {
                    self.clear_pending_on_newline_at_indent = Some(indent);
                }
            }
        }
    }

    fn prepare_for_indent(&mut self, indent: usize) {
        if !self.is_inside_block_scalar_line(indent) {
            self.block_scalar_parent_indent = None;
        }

        while self.stack.last().is_some_and(|entry| entry.indent > indent) {
            self.stack.pop();
        }
    }

    fn finish_line(&mut self) {
        let Some(indent) = self.clear_pending_on_newline_at_indent.take() else {
            return;
        };
        for entry in self.stack.iter_mut().rev() {
            if entry.indent == indent {
                if entry.container == Container::Mapping {
                    entry.pending_key = None;
                }
                break;
            }
        }
    }

    fn trailing_pending_mapping_segments_at_or_above(&self, logical_indent: usize) -> usize {
        self.stack
            .iter()
            .rev()
            .take_while(|entry| {
                entry.indent >= logical_indent
                    && entry.container == Container::Mapping
                    && entry.pending_key.is_some()
            })
            .count()
    }
}

enum LineEvent {
    DocumentBoundary,
    Mapping(MappingEvent),
    Sequence(SequenceEvent),
}

struct MappingEvent {
    indent: usize,
    key: String,
    scalar_value_present: bool,
    starts_block_scalar: bool,
}

struct SequenceEvent {
    indent: usize,
    payload: SequencePayload,
}

enum SequencePayload {
    None,
    Scalar,
    BlockScalar,
    InlineMapping(MappingEvent),
}

fn parse_line_event(line: &str) -> Option<LineEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if matches!(trimmed, "---" | "...") {
        return Some(LineEvent::DocumentBoundary);
    }

    let mut source = line.to_string();
    source.push('\n');
    let tree = parse_yaml_tree(&source)?;
    event_from_node(tree.root_node(), &source).or_else(|| repaired_mapping_value_event(line))
}

fn repaired_mapping_value_event(line: &str) -> Option<LineEvent> {
    let indent = line.chars().take_while(|&ch| ch == ' ').count();
    let after_indent = &line[indent..];
    let colon_offset = first_mapping_colon_offset(after_indent)?;
    let key_prefix = &after_indent[..=colon_offset];
    let repaired = format!("{}{} __HS_VALUE__\n", " ".repeat(indent), key_prefix);
    let tree = parse_yaml_tree(&repaired)?;
    event_from_node(tree.root_node(), &repaired)
}

fn event_from_node(node: tree_sitter::Node<'_>, source: &str) -> Option<LineEvent> {
    match node.kind() {
        "block_mapping_pair" | "flow_pair" => mapping_event(node, source).map(LineEvent::Mapping),
        "block_sequence_item" => Some(LineEvent::Sequence(sequence_event(node, source))),
        "block_mapping" | "flow_mapping" => first_child_event(node, source, "block_mapping_pair")
            .or_else(|| first_child_event(node, source, "flow_pair")),
        "block_sequence" | "flow_sequence" => {
            first_child_event(node, source, "block_sequence_item")
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                if let Some(event) = event_from_node(child, source) {
                    return Some(event);
                }
            }
            None
        }
    }
}

fn first_child_event(node: tree_sitter::Node<'_>, source: &str, kind: &str) -> Option<LineEvent> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() && child.kind() == kind {
            return event_from_node(child, source);
        }
    }
    None
}

fn mapping_event(node: tree_sitter::Node<'_>, source: &str) -> Option<MappingEvent> {
    let key = node.child_by_field_name("key")?;
    let value = node.child_by_field_name("value");
    let key_text = scalar_text(key, source)?;
    let indent = key.start_position().column;
    let (scalar_value_present, starts_block_scalar) =
        value.map(classify_value_shape).unwrap_or((false, false));

    Some(MappingEvent {
        indent,
        key: key_text,
        scalar_value_present,
        starts_block_scalar,
    })
}

fn sequence_event(node: tree_sitter::Node<'_>, source: &str) -> SequenceEvent {
    let indent = node.start_position().column;
    let payload = node
        .named_child(0)
        .map(|child| sequence_payload(node, child, source))
        .unwrap_or(SequencePayload::None);
    SequenceEvent { indent, payload }
}

fn sequence_payload(
    item: tree_sitter::Node<'_>,
    child: tree_sitter::Node<'_>,
    source: &str,
) -> SequencePayload {
    let item_row = item.start_position().row;
    let child = unwrap_yaml_node(child);

    if let Some(mapping) = sequence_mapping_payload(child, item_row, source) {
        return SequencePayload::InlineMapping(mapping);
    }

    match child.kind() {
        "block_scalar" => SequencePayload::BlockScalar,
        kind if is_scalar_like(kind) => SequencePayload::Scalar,
        _ if child.start_position().row == item_row => SequencePayload::Scalar,
        _ => SequencePayload::None,
    }
}

fn sequence_mapping_payload(
    child: tree_sitter::Node<'_>,
    item_row: usize,
    source: &str,
) -> Option<MappingEvent> {
    match child.kind() {
        "block_mapping_pair" | "flow_pair" => {
            let key = child.child_by_field_name("key")?;
            (key.start_position().row == item_row).then(|| mapping_event(child, source))?
        }
        "block_mapping" | "flow_mapping" => {
            let pair_kind = if child.kind() == "block_mapping" {
                "block_mapping_pair"
            } else {
                "flow_pair"
            };
            let mut cursor = child.walk();
            for pair in child.children(&mut cursor) {
                if !pair.is_named() || pair.kind() != pair_kind {
                    continue;
                }
                let key = pair.child_by_field_name("key")?;
                if key.start_position().row == item_row {
                    return mapping_event(pair, source);
                }
            }
            None
        }
        _ => None,
    }
}

fn classify_value_shape(node: tree_sitter::Node<'_>) -> (bool, bool) {
    let node = unwrap_yaml_node(node);
    match node.kind() {
        "block_scalar" => (false, true),
        "block_mapping" | "flow_mapping" | "block_sequence" | "flow_sequence" => (false, false),
        kind if is_scalar_like(kind) => (true, false),
        _ => (node.start_position().row == node.end_position().row, false),
    }
}

#[cfg(test)]
mod tests {
    use super::DocumentState;

    #[test]
    fn sequence_block_scalar_keeps_deeper_lines_out_of_document_state() {
        let mut state = DocumentState::default();

        state.ingest(
            r#"args:
  - -ec
  - |
    chown -R "#,
        );

        assert_eq!(state.current_path().0, vec!["args[*]"]);
        assert!(state.is_inside_block_scalar_line(4));
    }

    #[test]
    fn block_scalar_state_survives_action_sync_inside_body() {
        let mut state = DocumentState::default();

        state.ingest(
            r#"args:
  - -ec
  - |
    chown -R "#,
        );
        state.sync_action_position(4, 13, true);

        assert!(state.is_inside_block_scalar_line(4));
    }

    #[test]
    fn flow_sequence_value_tracks_mapping_key() {
        let mut state = DocumentState::default();

        state.ingest(r#"command: ['/bin/bash', '-c', 'timeout "#);

        assert_eq!(state.current_path().0, vec!["command"]);
    }
}
