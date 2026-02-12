use helm_schema_ast::{DefineIndex, HelmAst};

use crate::walker::{extract_values_paths, is_fragment_expr, parse_condition};
use crate::{Guard, IrGenerator, ResourceRef, ValueKind, ValueUse, YamlPath};

pub struct SymbolicIrGenerator;

impl IrGenerator for SymbolicIrGenerator {
    fn generate(&self, src: &str, _ast: &HelmAst, defines: &DefineIndex) -> Vec<ValueUse> {
        let language =
            tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&language).is_err() {
            return Vec::new();
        }
        let Some(tree) = parser.parse(src, None) else {
            return Vec::new();
        };

        let mut w = SymbolicWalker::new(src, defines);
        w.run(&tree)
    }
}

struct SymbolicWalker<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    uses: Vec<ValueUse>,
    guards: Vec<Guard>,
    no_output_depth: usize,
    dot_stack: Vec<Option<String>>,
    shape: Shape,
    resource_detector: ResourceDetector,
    text_spans: Vec<(usize, usize)>,
    text_span_idx: usize,
    text_pos: usize,
}

#[derive(Clone, Debug)]
struct Shape {
    stack: Vec<(usize, Container, Option<String>)>,
    at_line_start: bool,
    clear_pending_on_newline_at_indent: Option<usize>,
    prefix: Vec<String>,
    stack_floor: usize,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            at_line_start: true,
            clear_pending_on_newline_at_indent: None,
            prefix: Vec::new(),
            stack_floor: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Container {
    Mapping,
    Sequence,
}

fn rewrite_dot_expr_to_values(text: &str, dot_prefix: &str) -> String {
    fn is_ident_start(b: u8) -> bool {
        b.is_ascii_uppercase() || b.is_ascii_lowercase() || b == b'_'
    }

    fn is_ident_continue(b: u8) -> bool {
        is_ident_start(b) || b.is_ascii_digit()
    }

    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + dot_prefix.len() * 2);
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'.' {
            let prev = if i == 0 { None } else { Some(bytes[i - 1]) };
            let next = bytes.get(i + 1).copied();

            let prev_ok = !matches!(
                prev,
                Some(b'$' | b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'_')
            );

            // Bare dot: `.` not followed by an identifier.
            if prev_ok && next.is_some_and(|b| !is_ident_start(b)) {
                out.push_str(".Values.");
                out.push_str(dot_prefix);
                i += 1;
                continue;
            }

            // Dot selector: `.foo` or `.foo.bar`
            if prev_ok && next.is_some_and(is_ident_start) {
                let mut j = i + 1;
                while j < bytes.len() {
                    let b = bytes[j];
                    if is_ident_continue(b) || b == b'.' {
                        j += 1;
                        continue;
                    }
                    break;
                }
                let sel = &text[i + 1..j];
                if sel == "Values" {
                    out.push('.');
                    out.push_str(sel);
                } else {
                    out.push_str(".Values.");
                    out.push_str(dot_prefix);
                    out.push('.');
                    out.push_str(sel);
                }
                i = j;
                continue;
            }
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

impl Shape {
    fn current_path(&self) -> YamlPath {
        let mut out: Vec<String> = self.prefix.clone();
        for (_, kind, pending_key) in &self.stack {
            match (kind, pending_key) {
                (Container::Mapping, Some(k)) => {
                    if !k.is_empty() {
                        out.push(k.clone());
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

    fn sync_action_position(&mut self, indent: usize, col: usize) {
        let effective = std::cmp::max(indent, col);
        while let Some((top_indent, _, _)) = self.stack.last().cloned() {
            if top_indent > effective {
                if self.stack.len() <= self.stack_floor {
                    break;
                }
                self.stack.pop();
            } else {
                break;
            }
        }

        if col > indent && self.clear_pending_on_newline_at_indent.is_none() {
            let mut candidate: Option<usize> = None;
            for (top_indent, kind, pending) in self.stack.iter().rev() {
                if *kind != Container::Mapping {
                    continue;
                }
                if pending.is_none() {
                    continue;
                }
                if *top_indent < indent {
                    break;
                }
                if *top_indent <= col {
                    candidate = Some(*top_indent);
                    break;
                }
            }
            if let Some(i) = candidate {
                self.clear_pending_on_newline_at_indent = Some(i);
            }
        }
    }

    fn ingest(&mut self, text: &str) {
        fn parse_yaml_key(after: &str) -> Option<(String, bool)> {
            let after = after.trim_end();
            if after.starts_with("{{") {
                return None;
            }
            let mut chars = after.chars();
            let mut key = String::new();
            while let Some(c) = chars.next() {
                if c == ':' {
                    if key.is_empty() {
                        return None;
                    }
                    let rest = chars.as_str();
                    let rest = rest.trim_start();
                    let is_block =
                        rest.is_empty() || rest.starts_with('|') || rest.starts_with('>');
                    return Some((key.trim().to_string(), !is_block));
                }
                if c.is_whitespace() {
                    return None;
                }
                if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/') {
                    key.push(c);
                    continue;
                }
                return None;
            }
            None
        }

        fn clear_pending_at_indent(
            stack: &mut Vec<(usize, Container, Option<String>)>,
            indent: usize,
        ) {
            for (top_indent, kind, pending) in stack.iter_mut().rev() {
                if *top_indent == indent {
                    if *kind == Container::Mapping {
                        *pending = None;
                    }
                    break;
                }
            }
        }

        for raw in text.split_inclusive('\n') {
            let is_newline_terminated = raw.ends_with('\n');
            if !self.at_line_start {
                if is_newline_terminated
                    && let Some(indent) = self.clear_pending_on_newline_at_indent.take()
                {
                    clear_pending_at_indent(&mut self.stack, indent);
                }
                self.at_line_start = is_newline_terminated;
                continue;
            }

            let line = raw.trim_end_matches('\n');
            let indent = line.chars().take_while(|&c| c == ' ').count();
            let after = &line[indent..];
            let trimmed = after.trim_end();

            if trimmed.is_empty() {
                self.at_line_start = is_newline_terminated;
                continue;
            }

            if trimmed == "---" || trimmed == "..." {
                self.stack.truncate(self.stack_floor);
                self.at_line_start = is_newline_terminated;
                continue;
            }

            if trimmed.trim_start().starts_with("{{") {
                self.at_line_start = is_newline_terminated;
                continue;
            }

            while let Some((top_indent, _, _)) = self.stack.last().cloned() {
                if top_indent > indent {
                    if self.stack.len() <= self.stack_floor {
                        break;
                    }
                    self.stack.pop();
                } else {
                    break;
                }
            }

            if after.starts_with("- ") {
                match self.stack.last_mut() {
                    Some((top_indent, Container::Sequence, pending)) if *top_indent == indent => {
                        *pending = None;
                    }
                    Some((top_indent, Container::Mapping, _)) if *top_indent == indent => {
                        self.stack.push((indent, Container::Sequence, None));
                    }
                    Some((top_indent, _, _)) if *top_indent < indent => {
                        self.stack.push((indent, Container::Sequence, None));
                    }
                    None => {
                        self.stack.push((indent, Container::Sequence, None));
                    }
                    _ => {}
                }

                if let Some(colon) = after.find(':') {
                    let key = after["- ".len()..colon].trim().to_string();
                    let child_indent = indent + 2;
                    self.stack
                        .push((child_indent, Container::Mapping, Some(key)));

                    let rest = after[colon + 1..].trim_start();
                    let scalar_value_present =
                        !rest.is_empty() && !rest.starts_with('|') && !rest.starts_with('>');
                    if scalar_value_present {
                        if is_newline_terminated {
                            clear_pending_at_indent(&mut self.stack, child_indent);
                        } else {
                            self.clear_pending_on_newline_at_indent = Some(child_indent);
                        }
                    }
                }

                self.at_line_start = is_newline_terminated;
                continue;
            }

            if let Some((key, scalar_value_present)) = parse_yaml_key(after) {
                match self.stack.last_mut() {
                    Some((top_indent, Container::Mapping, pending)) if *top_indent == indent => {
                        *pending = Some(key);
                    }
                    Some((top_indent, _, _)) if *top_indent < indent => {
                        self.stack.push((indent, Container::Mapping, Some(key)));
                    }
                    None => {
                        self.stack.push((indent, Container::Mapping, Some(key)));
                    }
                    _ => {}
                }

                if scalar_value_present {
                    if is_newline_terminated {
                        clear_pending_at_indent(&mut self.stack, indent);
                    } else {
                        self.clear_pending_on_newline_at_indent = Some(indent);
                    }
                }

                self.at_line_start = is_newline_terminated;
                continue;
            }

            if self.stack.is_empty() {
                self.stack.push((indent, Container::Mapping, None));
            }

            self.at_line_start = is_newline_terminated;
        }
    }
}

impl<'a> SymbolicWalker<'a> {
    fn new(source: &'a str, defines: &'a DefineIndex) -> Self {
        Self {
            source,
            defines,
            uses: Vec::new(),
            guards: Vec::new(),
            no_output_depth: 0,
            dot_stack: Vec::new(),
            shape: Shape::default(),
            resource_detector: ResourceDetector::default(),
            text_spans: Vec::new(),
            text_span_idx: 0,
            text_pos: 0,
        }
    }

    fn run(&mut self, tree: &tree_sitter::Tree) -> Vec<ValueUse> {
        self.reset_text_spans(tree);
        self.walk(tree.root_node());
        self.uses.sort_by(|a, b| {
            a.source_expr
                .cmp(&b.source_expr)
                .then_with(|| a.path.0.cmp(&b.path.0))
                .then_with(|| (a.kind as u8).cmp(&(b.kind as u8)))
                .then_with(|| a.resource.cmp(&b.resource))
                .then_with(|| a.guards.cmp(&b.guards))
        });
        self.uses.dedup();
        std::mem::take(&mut self.uses)
    }

    fn reset_text_spans(&mut self, tree: &tree_sitter::Tree) {
        let mut spans = Vec::<(usize, usize)>::new();
        let mut stack = vec![tree.root_node()];
        while let Some(n) = stack.pop() {
            if n.is_named() {
                if matches!(n.kind(), "text" | "yaml_no_injection_text") {
                    let r = n.byte_range();
                    spans.push((r.start, r.end));
                }
                let mut w = n.walk();
                for ch in n.children(&mut w) {
                    if ch.is_named() {
                        stack.push(ch);
                    }
                }
            }
        }
        spans.sort_by_key(|(s, _)| *s);

        // Merge adjacent spans so we don't split YAML line prefixes (notably `- `)
        // across multiple `ingest()` calls.
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (s, e) in spans {
            match merged.last_mut() {
                Some((ms, me)) if s <= *me => {
                    let _ = ms;
                    *me = (*me).max(e);
                }
                Some((_ms, me)) if s == *me => {
                    *me = e;
                }
                _ => merged.push((s, e)),
            }
        }

        self.text_spans = merged;
        self.text_span_idx = 0;
        self.text_pos = 0;
        self.resource_detector = ResourceDetector::default();
        self.shape = Shape::default();
        self.guards.clear();
        self.dot_stack.clear();
        self.no_output_depth = 0;
    }

    fn ingest_text_up_to(&mut self, target: usize) {
        let target = target.min(self.source.len());
        if target <= self.text_pos {
            return;
        }

        let mut buf = String::new();

        while self.text_span_idx < self.text_spans.len() {
            let (s, e) = self.text_spans[self.text_span_idx];

            if e <= self.text_pos {
                self.text_span_idx += 1;
                continue;
            }
            if s >= target {
                break;
            }

            let start = s.max(self.text_pos);
            let end = e.min(target);
            if start < end {
                let txt = &self.source[start..end];
                buf.push_str(txt);
                self.text_pos = end;
            }

            if self.text_pos >= e {
                self.text_span_idx += 1;
            }

            if self.text_pos >= target {
                break;
            }

            if self.text_span_idx < self.text_spans.len() {
                let (ns, _) = self.text_spans[self.text_span_idx];
                if self.text_pos < ns {
                    let end = ns.min(target);
                    if self.text_pos < end {
                        let gap = &self.source[self.text_pos..end];
                        let mut sanitized = String::with_capacity(gap.len());
                        for ch in gap.chars() {
                            if ch == '\n' || ch == ' ' || ch == '\t' {
                                sanitized.push(ch);
                            }
                        }
                        if !sanitized.is_empty() {
                            buf.push_str(&sanitized);
                        }
                        self.text_pos = end;
                    }
                }
            } else {
                self.text_pos = target;
            }

            if buf.len() > 4096 {
                self.shape.ingest(&buf);
                self.resource_detector.ingest(&buf);
                buf.clear();
            }
        }

        if self.text_pos < target {
            self.text_pos = target;
        }

        if !buf.is_empty() {
            self.shape.ingest(&buf);
            self.resource_detector.ingest(&buf);
        }
    }

    fn line_indent_and_col(&self, byte_pos: usize) -> (usize, usize) {
        let bytes = self.source.as_bytes();
        let mut line_start = byte_pos.min(bytes.len());
        while line_start > 0 {
            if bytes[line_start - 1] == b'\n' {
                break;
            }
            line_start -= 1;
        }

        let col = byte_pos.saturating_sub(line_start);
        let mut indent = 0usize;
        while line_start + indent < bytes.len() {
            if bytes[line_start + indent] == b' ' {
                indent += 1;
            } else {
                break;
            }
        }
        (indent, col)
    }

    fn sync_action_for_node(&mut self, node: tree_sitter::Node<'_>) {
        if matches!(node.kind(), "text" | "yaml_no_injection_text") {
            return;
        }

        // Only sync placement for nodes that correspond to template actions or whole pipelines.
        // Syncing on every leaf (e.g. inside selector chains) can shift the shape incorrectly
        // and cause spurious path emissions.
        // Sync *only* on template-level action nodes.
        // Syncing on inner expression nodes (e.g. `selector_expression`) can set
        // `clear_pending_on_newline_at_indent` at the wrong indent and clear parent keys.
        if !matches!(
            node.kind(),
            "if_action"
                | "with_action"
                | "range_action"
                | "template_action"
                | "define_action"
                | "block_action"
                | "{{"
                | "{{-"
        ) {
            return;
        }

        let mut pos = node.start_byte().min(self.source.len());
        let end = node.end_byte().min(self.source.len());
        while pos < end {
            match self.source.as_bytes()[pos] {
                b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
                _ => break,
            }
        }

        if pos > node.start_byte() {
            let leading = &self.source[node.start_byte()..pos];
            let mut sanitized = String::with_capacity(leading.len());
            for ch in leading.chars() {
                if ch == '\n' || ch == ' ' || ch == '\t' {
                    sanitized.push(ch);
                }
            }
            if !sanitized.is_empty() {
                self.shape.ingest(&sanitized);
                self.resource_detector.ingest(&sanitized);
                self.text_pos = pos;
            }
        }

        let (indent, col) = self.line_indent_and_col(pos);
        self.shape.sync_action_position(indent, col);
    }

    fn children_with_field<'n>(
        &self,
        node: tree_sitter::Node<'n>,
        field: &str,
    ) -> Vec<tree_sitter::Node<'n>> {
        let mut out = Vec::new();
        for i in 0..node.child_count() {
            let Some(ch) = node.child(i) else {
                continue;
            };
            if !ch.is_named() {
                continue;
            }
            let Ok(i) = u32::try_from(i) else {
                continue;
            };
            if node.field_name_for_child(i) == Some(field) {
                out.push(ch);
            }
        }
        out
    }

    fn emit_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind) {
        let path = if self.no_output_depth > 0 {
            YamlPath(Vec::new())
        } else {
            path
        };
        self.uses.push(ValueUse {
            source_expr,
            path,
            kind,
            guards: self.guards.clone(),
            resource: self.resource_detector.current(),
        });
    }

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        let Ok(text) = node.utf8_text(self.source.as_bytes()) else {
            return;
        };

        if std::env::var("SYMBOLIC_DEBUG").is_ok()
            && (text.contains("containerPorts")
                || text.contains("commonAnnotations")
                || text.contains("extraIngress")
                || text.contains("extraEgress"))
        {
            eprintln!(
                "symbolic: output_node kind={} text={}",
                node.kind(),
                text.replace('\n', "\\n")
            );
        }
        let kind = if is_fragment_expr(text) {
            ValueKind::Fragment
        } else {
            ValueKind::Scalar
        };
        let mut values = extract_values_paths(text);
        if values.is_empty()
            && let Some(Some(dot_prefix)) = self.dot_stack.last()
        {
            let rewritten = rewrite_dot_expr_to_values(text, dot_prefix);
            values = extract_values_paths(&rewritten);
        }
        if values.is_empty() {
            return;
        }
        let mut path = self.shape.current_path();
        if kind == ValueKind::Fragment {
            if let Some(last) = path.0.last_mut()
                && let Some(stripped) = last.strip_suffix("[*]")
            {
                *last = stripped.to_string();
            }
            if matches!(path.0.last().map(std::string::String::as_str), Some("")) {
                path.0.pop();
            }
        }
        for v in values {
            if std::env::var("SYMBOLIC_DEBUG").is_ok()
                && matches!(
                    v.as_str(),
                    "metrics.containerPorts.http"
                        | "master.containerPorts.redis"
                        | "networkPolicy.extraIngress"
                        | "commonAnnotations"
                        | "commonLabels"
                )
            {
                eprintln!("symbolic: use={v} kind={kind:?} path={:?}", path.0);
            }
            self.emit_use(v, path.clone(), kind);
        }
    }

    fn collect_if_with_guards(&mut self, cond_text: &str) {
        let mut cond_guards = parse_condition(cond_text);
        if cond_guards.is_empty()
            && let Some(Some(dot_prefix)) = self.dot_stack.last()
        {
            let rewritten = rewrite_dot_expr_to_values(cond_text, dot_prefix);
            cond_guards = parse_condition(&rewritten);
        }
        for g in &cond_guards {
            for path in g.value_paths() {
                self.emit_use(path.to_string(), YamlPath(Vec::new()), ValueKind::Scalar);
            }
            if !self.guards.contains(g) {
                self.guards.push(g.clone());
            }
        }
    }

    fn push_with_dot_binding(&mut self, header_text: &str) {
        let values = extract_values_paths(header_text);
        if values.len() == 1 {
            self.dot_stack.push(Some(values[0].clone()));
        } else {
            self.dot_stack.push(None);
        }
    }

    fn collect_range_guards(&mut self, header_text: &str) {
        let values = extract_values_paths(header_text);
        let path = self.shape.current_path();
        for v in values {
            self.emit_use(v.clone(), path.clone(), ValueKind::Scalar);
            let g = Guard::Truthy { path: v };
            if !self.guards.contains(&g) {
                self.guards.push(g);
            }
        }
    }

    fn range_header_text(&self, node: tree_sitter::Node<'_>) -> Option<String> {
        if let Some(p) = node.child_by_field_name("range") {
            return p
                .utf8_text(self.source.as_bytes())
                .ok()
                .map(|s| s.trim().to_string());
        }
        let mut w = node.walk();
        for ch in node.named_children(&mut w) {
            if ch.kind() == "range_variable_definition"
                && let Some(p) = ch.child_by_field_name("range")
            {
                return p
                    .utf8_text(self.source.as_bytes())
                    .ok()
                    .map(|s| s.trim().to_string());
            }
        }
        None
    }

    fn walk(&mut self, node: tree_sitter::Node<'_>) {
        self.ingest_text_up_to(node.start_byte());
        self.sync_action_for_node(node);

        match node.kind() {
            "text" | "yaml_no_injection_text" => {
                self.ingest_text_up_to(node.end_byte());
                return;
            }

            "variable_definition" | "assignment" => {
                self.no_output_depth += 1;
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    self.walk(ch);
                }
                self.no_output_depth = self.no_output_depth.saturating_sub(1);
                return;
            }

            "if_action" => {
                let saved = self.guards.len();
                if let Some(cond) = node.child_by_field_name("condition")
                    && let Ok(txt) = cond.utf8_text(self.source.as_bytes())
                {
                    self.collect_if_with_guards(txt);
                }

                let consequence = self.children_with_field(node, "consequence");
                for ch in consequence {
                    self.walk(ch);
                }

                self.guards.truncate(saved);

                // Note: else-if chains are represented as repeated condition/option fields.
                // For now, we only handle the plain else branch.
                let alternative = self.children_with_field(node, "alternative");
                for ch in alternative {
                    self.walk(ch);
                }
                return;
            }

            "with_action" => {
                let saved = self.guards.len();
                let saved_dot = self.dot_stack.len();
                if let Some(cond) = node.child_by_field_name("condition")
                    && let Ok(txt) = cond.utf8_text(self.source.as_bytes())
                {
                    self.push_with_dot_binding(txt);
                    self.collect_if_with_guards(txt);
                }

                let consequence = self.children_with_field(node, "consequence");
                for ch in consequence {
                    self.walk(ch);
                }

                self.guards.truncate(saved);
                self.dot_stack.truncate(saved_dot);

                let alternative = self.children_with_field(node, "alternative");
                for ch in alternative {
                    self.walk(ch);
                }
                return;
            }

            "range_action" => {
                let saved = self.guards.len();
                let saved_dot = self.dot_stack.len();
                self.dot_stack.push(None);
                if let Some(txt) = self.range_header_text(node) {
                    self.collect_range_guards(&txt);
                }

                let body = self.children_with_field(node, "body");
                for ch in body {
                    self.walk(ch);
                }

                self.guards.truncate(saved);
                self.dot_stack.truncate(saved_dot);

                let alternative = self.children_with_field(node, "alternative");
                for ch in alternative {
                    self.walk(ch);
                }
                return;
            }

            "define_action" | "block_action" => {
                return;
            }

            "template_action"
            | "chained_pipeline"
            | "parenthesized_pipeline"
            | "selector_expression"
            | "function_call"
            | "method_call" => {
                self.handle_output_node(node);
                return;
            }

            _ => {}
        }

        let mut c = node.walk();
        for ch in node.children(&mut c) {
            self.walk(ch);
        }
    }
}

#[derive(Default, Clone, Debug)]
struct ResourceDetector {
    api_version: Option<String>,
    kind: Option<String>,
    current: Option<ResourceRef>,
    buf: String,
}

impl ResourceDetector {
    fn current(&self) -> Option<ResourceRef> {
        self.current.clone()
    }

    fn ingest(&mut self, text: &str) {
        fn parse_literal_value(line: &str, key: &str) -> Option<String> {
            let rest = line.strip_prefix(key)?;
            let rest = rest.trim_start();
            let rest = rest.strip_prefix(':')?.trim_start();
            if rest.is_empty() {
                return None;
            }
            if rest.contains("{{") {
                return None;
            }
            let val = rest
                .trim_matches('"')
                .trim_matches('\'')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            if val.is_empty() { None } else { Some(val) }
        }

        fn process_line(det: &mut ResourceDetector, line: &str) {
            let line = line.trim_end_matches('\r');
            let indent = line.chars().take_while(|&c| c == ' ').count();
            let after = &line[indent..];
            let trimmed = after.trim_end();

            if trimmed.is_empty() {
                return;
            }
            if trimmed == "---" || trimmed == "..." {
                det.api_version = None;
                det.kind = None;
                det.current = None;
                return;
            }

            if let Some(v) = parse_literal_value(trimmed, "apiVersion") {
                det.api_version = Some(v);
            }
            if let Some(v) = parse_literal_value(trimmed, "kind") {
                det.kind = Some(v);
            }

            if let Some(kind) = &det.kind {
                det.current = Some(ResourceRef {
                    api_version: det.api_version.clone().unwrap_or_default(),
                    kind: kind.clone(),
                });
            }
        }

        self.buf.push_str(text);
        while let Some(nl) = self.buf.find('\n') {
            let line = self.buf[..nl].to_string();
            self.buf.drain(..=nl);
            process_line(self, &line);
        }
    }
}
