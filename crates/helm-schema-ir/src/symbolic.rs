use std::collections::{BTreeSet, HashMap, HashSet};

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
    seed_guards: Vec<Guard>,
    no_output_depth: usize,
    dot_stack: Vec<Option<String>>,
    shape: Shape,
    resource_detector: ResourceDetector,
    text_spans: Vec<(usize, usize)>,
    text_span_idx: usize,
    text_pos: usize,

    define_value_cache: HashMap<String, DefineValues>,

    inline_stack: Vec<String>,

    range_domains: HashMap<String, Vec<String>>,
    get_bindings: HashMap<String, GetBinding>,

    inline_helpers_in_fragments: bool,
}

#[derive(Clone, Debug)]
struct GetBinding {
    base: String,
    key_var: String,
}

#[derive(Clone, Debug, Default)]
struct DefineValues {
    output: Vec<String>,
    guards: Vec<String>,
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

    fn sync_action_position(&mut self, indent: usize, col: usize, allow_clear_pending: bool) {
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

        if allow_clear_pending && col > indent && self.clear_pending_on_newline_at_indent.is_none()
        {
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

    #[allow(clippy::too_many_lines)]
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
                    let is_template = rest.starts_with("{{");
                    let is_template_fragment = is_template
                        && (rest.contains("toYaml")
                            || rest.contains("nindent")
                            || rest.contains("indent")
                            || rest.contains("tpl"));
                    let is_block = rest.is_empty()
                        || rest.starts_with('|')
                        || rest.starts_with('>')
                        || is_template_fragment;
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
            stack: &mut [(usize, Container, Option<String>)],
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
                // If we were in a sequence at this indent and now see a mapping key,
                // treat it as ending the sequence and starting a sibling mapping entry.
                //
                // This matters for some Helm templates where list items appear at the same
                // indentation level as the parent key due to whitespace trimming.
                if let Some((top_indent, Container::Sequence, _)) = self.stack.last()
                    && *top_indent == indent
                {
                    self.stack.pop();
                }
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
            seed_guards: Vec::new(),
            no_output_depth: 0,
            dot_stack: Vec::new(),
            shape: Shape::default(),
            resource_detector: ResourceDetector::default(),
            text_spans: Vec::new(),
            text_span_idx: 0,
            text_pos: 0,

            define_value_cache: HashMap::new(),

            inline_stack: Vec::new(),

            range_domains: HashMap::new(),
            get_bindings: HashMap::new(),

            inline_helpers_in_fragments: false,
        }
    }

    fn with_initial_guards(mut self, guards: Vec<Guard>) -> Self {
        self.seed_guards = guards;
        self.guards = self.seed_guards.clone();
        self
    }

    fn with_inline_stack(mut self, stack: Vec<String>) -> Self {
        self.inline_stack = stack;
        self
    }

    fn with_inline_helpers_in_fragments(mut self, enabled: bool) -> Self {
        self.inline_helpers_in_fragments = enabled;
        self
    }

    fn parse_go_template(src: &str) -> Option<tree_sitter::Tree> {
        let language =
            tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&language).is_err() {
            return None;
        }
        parser.parse(src, None)
    }

    fn parse_literal_list_range(header: &str) -> Option<(String, Vec<String>)> {
        if !header.contains("list") {
            return None;
        }

        let toks: Vec<&str> = header.split_whitespace().collect();
        let list_pos = toks.iter().position(|t| *t == "list")?;

        // `range $k := list ...` or `$k := list ...` or `list ...` (in some tree-sitter nodes).
        // We only care about the bound variable name and the literal domain.
        let var = toks
            .iter()
            .take(list_pos)
            .find_map(|t| t.strip_prefix('$'))
            .filter(|v| !v.is_empty() && !v.contains('.') && !v.contains('/') && !v.contains('('))
            .map(std::string::ToString::to_string)?;

        let mut out = Vec::new();
        for t in toks.iter().skip(list_pos + 1) {
            if let Some(s) = t.strip_prefix('"').and_then(|x| x.strip_suffix('"'))
                && !s.is_empty()
            {
                out.push(s.to_string());
            }
        }
        if out.is_empty() {
            None
        } else {
            Some((var, out))
        }
    }

    fn parse_get_binding(text: &str) -> Option<(String, GetBinding)> {
        // Patterns like:
        //   $x := get $.Values.foo.bar $k
        //   $x = get $.Values.foo $k
        let toks: Vec<&str> = text.split_whitespace().collect();
        let get_pos = toks.iter().position(|t| *t == "get")?;
        if get_pos < 2 {
            return None;
        }
        if get_pos + 2 >= toks.len() {
            return None;
        }

        let op = toks[get_pos - 1];
        if op != ":=" && op != "=" {
            return None;
        }

        let var_tok = toks[get_pos - 2];
        let var = var_tok.strip_prefix('$')?.to_string();

        let base_tok = toks[get_pos + 1];
        let base = base_tok
            .strip_prefix("$.Values.")
            .or_else(|| base_tok.strip_prefix(".Values."))?
            .to_string();

        let key_tok = toks[get_pos + 2];
        let key_var = key_tok.strip_prefix('$')?.to_string();
        Some((var, GetBinding { base, key_var }))
    }

    fn eq_literals_for_var(text: &str, var: &str) -> Vec<String> {
        let needle = format!("eq ${var} \"");
        let mut lits = Vec::new();
        let mut rest = text;
        while let Some(i) = rest.find(&needle) {
            let after = &rest[(i + needle.len())..];
            if let Some(end) = after.find('"') {
                let lit = &after[..end];
                if !lit.is_empty() {
                    lits.push(lit.to_string());
                }
                rest = &after[end..];
            } else {
                break;
            }
        }
        lits
    }

    fn extract_bound_values(&self, text: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();

        // Very small heuristic parser for `$var.field[.subfield]...` patterns.
        // This is used for templates that bind `$var := get $.Values.someMap $key`.
        for tok in text.split_whitespace() {
            let Some(tok) = tok.strip_prefix('$') else {
                continue;
            };
            let Some((var, rest)) = tok.split_once('.') else {
                continue;
            };

            let rest = rest
                .trim_end_matches(',')
                .trim_end_matches(')')
                .trim_end_matches('}')
                .trim_end_matches('|');

            let Some(binding) = self.get_bindings.get(var) else {
                continue;
            };
            let Some(domain) = self.range_domains.get(&binding.key_var) else {
                continue;
            };

            let mut skip_literals: HashSet<String> = HashSet::new();
            if rest == "enabled" && binding.base == "config" {
                for lit in Self::eq_literals_for_var(text, &binding.key_var) {
                    skip_literals.insert(lit);
                }
            }
            for v in domain {
                if skip_literals.contains(v) {
                    continue;
                }
                out.push(format!("{}.{}.{}", binding.base, v, rest));
            }
        }

        out.sort();
        out.dedup();
        out
    }

    fn extract_next_quoted_string_after(text: &str, key: &str) -> Option<String> {
        let needle = format!("\"{key}\"");
        let idx = text.find(&needle)? + needle.len();
        let rest = &text[idx..];
        let q1 = rest.find('"')?;
        let rest2 = &rest[(q1 + 1)..];
        let q2 = rest2.find('"')?;
        Some(rest2[..q2].to_string())
    }

    fn maybe_inline_nats_load_merge_patch(&mut self, text: &str) {
        if !Self::literal_template_calls(text)
            .iter()
            .any(|c| c == "nats.loadMergePatch")
        {
            return;
        }

        // `nats.loadMergePatch` reads `.merge` and `.patch` from its argument.
        // When called under a `with .Values.X` block, the argument is usually `.`.
        // Emit those value paths so schema generation includes them.
        if let Some(Some(prefix)) = self.dot_stack.last().cloned() {
            self.emit_use(
                format!("{prefix}.merge"),
                YamlPath(Vec::new()),
                ValueKind::Scalar,
            );
            self.emit_use(
                format!("{prefix}.patch"),
                YamlPath(Vec::new()),
                ValueKind::Scalar,
            );
        }

        let Some(file) = Self::extract_next_quoted_string_after(text, "file") else {
            return;
        };
        let path = if file.contains('/') {
            file
        } else {
            format!("files/{file}")
        };

        if self.inline_stack.iter().any(|p| p == &path) {
            return;
        }
        let Some(src) = self.defines.get_file(&path) else {
            return;
        };

        let Some(tree) = Self::parse_go_template(src) else {
            return;
        };

        let mut stack = self.inline_stack.clone();
        stack.push(path);
        let mut nested = SymbolicWalker::new(src, self.defines)
            .with_initial_guards(self.guards.clone())
            .with_inline_stack(stack)
            .with_inline_helpers_in_fragments(true);

        let uses = nested.run(&tree);
        self.uses.extend(uses);
    }

    fn literal_template_calls(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let toks: Vec<&str> = text.split_whitespace().collect();
        for i in 0..toks.len().saturating_sub(1) {
            let head = toks[i];
            if head != "include" && head != "template" {
                continue;
            }
            let arg = toks[i + 1];
            if !arg.starts_with('"') {
                continue;
            }
            let end = arg.rfind('"');
            if end.is_none() || end == Some(0) {
                continue;
            }
            let end = end.unwrap();
            if end <= 1 {
                continue;
            }
            let name = &arg[1..end];
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
        out.sort();
        out.dedup();
        out
    }

    fn collect_values_from_ast(
        node: &HelmAst,
        defines: &DefineIndex,
        visited: &mut HashSet<String>,
        output: &mut BTreeSet<String>,
        guards: &mut BTreeSet<String>,
    ) {
        match node {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Sequence { items }
            | HelmAst::Define { body: items, .. }
            | HelmAst::Block { body: items, .. } => {
                for it in items {
                    Self::collect_values_from_ast(it, defines, visited, output, guards);
                }
            }
            HelmAst::Pair { key, value } => {
                Self::collect_values_from_ast(key, defines, visited, output, guards);
                if let Some(v) = value {
                    Self::collect_values_from_ast(v, defines, visited, output, guards);
                }
            }
            HelmAst::HelmExpr { text } => {
                for v in extract_values_paths(text) {
                    output.insert(v);
                }
                for name in Self::literal_template_calls(text) {
                    if !visited.insert(name.clone()) {
                        continue;
                    }
                    if let Some(body) = defines.get(&name) {
                        for it in body {
                            Self::collect_values_from_ast(it, defines, visited, output, guards);
                        }
                    }
                }
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                for v in extract_values_paths(cond) {
                    guards.insert(v);
                }
                for it in then_branch {
                    Self::collect_values_from_ast(it, defines, visited, output, guards);
                }
                for it in else_branch {
                    Self::collect_values_from_ast(it, defines, visited, output, guards);
                }
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            }
            | HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                for v in extract_values_paths(header) {
                    guards.insert(v);
                }
                for it in body {
                    Self::collect_values_from_ast(it, defines, visited, output, guards);
                }
                for it in else_branch {
                    Self::collect_values_from_ast(it, defines, visited, output, guards);
                }
            }
            HelmAst::Scalar { .. } | HelmAst::HelmComment { .. } => {}
        }
    }

    fn values_from_define(&mut self, name: &str) -> DefineValues {
        if let Some(v) = self.define_value_cache.get(name).cloned() {
            return v;
        }

        let mut output = BTreeSet::<String>::new();
        let mut guards = BTreeSet::<String>::new();
        let mut visited = HashSet::<String>::new();
        visited.insert(name.to_string());
        if let Some(body) = self.defines.get(name) {
            for it in body {
                Self::collect_values_from_ast(
                    it,
                    self.defines,
                    &mut visited,
                    &mut output,
                    &mut guards,
                );
            }
        }

        let v = DefineValues {
            output: output.into_iter().collect(),
            guards: guards.into_iter().collect(),
        };
        self.define_value_cache.insert(name.to_string(), v.clone());
        v
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
        self.guards = self.seed_guards.clone();
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
        fn parse_indent_width(text: &str) -> Option<usize> {
            for kw in ["nindent", "indent"] {
                let Some(idx) = text.rfind(kw) else {
                    continue;
                };
                let after = &text[idx + kw.len()..];
                let after = after.trim_start();
                let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
                if digits.is_empty() {
                    continue;
                }
                if let Ok(n) = digits.parse::<usize>() {
                    return Some(n);
                }
            }
            None
        }

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

        // Only clear a pending mapping key at end-of-line when this looks like an inline scalar
        // value (e.g. `name: {{ ... }}`), not when the template action expands to a YAML fragment
        // via `nindent` / `toYaml` / `tpl`.
        let allow_clear_pending = if node.kind() == "template_action" {
            if let Ok(text) = node.utf8_text(self.source.as_bytes()) {
                !is_fragment_expr(text)
            } else {
                true
            }
        } else {
            false
        };

        let (indent, col) = if node.kind() == "template_action" && !allow_clear_pending {
            if let Ok(text) = node.utf8_text(self.source.as_bytes())
                && let Some(virtual_indent) = parse_indent_width(text)
                && virtual_indent > indent
            {
                (virtual_indent, virtual_indent)
            } else {
                (indent, col)
            }
        } else {
            (indent, col)
        };

        self.shape
            .sync_action_position(indent, col, allow_clear_pending);
    }

    fn children_with_field<'n>(
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

    fn emit_helper_use(&mut self, source_expr: String) {
        if source_expr.trim().is_empty() {
            return;
        }
        self.uses.push(ValueUse {
            source_expr,
            path: YamlPath(Vec::new()),
            kind: ValueKind::Scalar,
            guards: Vec::new(),
            resource: None,
        });
    }

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        let Ok(text) = node.utf8_text(self.source.as_bytes()) else {
            return;
        };

        self.maybe_inline_nats_load_merge_patch(text);

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

        let bound_values = self.extract_bound_values(text);

        let mut helper_output_values: Vec<String> = Vec::new();
        let mut helper_guard_values: Vec<String> = Vec::new();
        if kind == ValueKind::Scalar
            || (self.inline_helpers_in_fragments && kind == ValueKind::Fragment)
        {
            for name in Self::literal_template_calls(text) {
                if name.ends_with(".defaultValues") {
                    continue;
                }
                let v = self.values_from_define(&name);
                helper_output_values.extend(v.output);
                helper_guard_values.extend(v.guards);
            }
            helper_output_values.sort();
            helper_output_values.dedup();
            helper_guard_values.sort();
            helper_guard_values.dedup();
        }

        if values.is_empty()
            && bound_values.is_empty()
            && helper_output_values.is_empty()
            && helper_guard_values.is_empty()
        {
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
            let in_sequence_item = path
                .0
                .last()
                .map(std::string::String::as_str)
                .is_some_and(|s| s.ends_with("[*]"));

            let emit_path = if v.ends_with(".*") && !in_sequence_item {
                YamlPath(Vec::new())
            } else {
                path.clone()
            };
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
                eprintln!("symbolic: use={v} kind={kind:?} path={:?}", emit_path.0);
            }
            self.emit_use(v, emit_path, kind);
        }

        for v in bound_values {
            self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
        }

        for v in helper_output_values {
            self.emit_helper_use(v);
        }

        for v in helper_guard_values {
            self.emit_helper_use(v);
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

        for v in self.extract_bound_values(cond_text) {
            self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
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

    fn collect_range_guards(&mut self, header_text: &str, path: YamlPath) {
        let values = extract_values_paths(header_text);
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

        if self.walk_control_node(node) {
            return;
        }
        if self.walk_action_node(node) {
            return;
        }

        let mut c = node.walk();
        for ch in node.children(&mut c) {
            self.walk(ch);
        }
    }

    fn walk_control_node(&mut self, node: tree_sitter::Node<'_>) -> bool {
        match node.kind() {
            "text" | "yaml_no_injection_text" => {
                self.ingest_text_up_to(node.end_byte());
                true
            }
            "define_action" | "block_action" => true,
            _ => false,
        }
    }

    fn walk_action_node(&mut self, node: tree_sitter::Node<'_>) -> bool {
        match node.kind() {
            "variable_definition" | "assignment" => {
                self.handle_variable_definition_or_assignment(node);
                true
            }
            "if_action" => {
                self.handle_if_action(node);
                true
            }
            "with_action" => {
                self.handle_with_action(node);
                true
            }
            "range_action" => {
                self.handle_range_action(node);
                true
            }
            "template_action"
            | "chained_pipeline"
            | "parenthesized_pipeline"
            | "selector_expression"
            | "function_call"
            | "method_call" => {
                self.handle_output_node(node);
                true
            }
            _ => false,
        }
    }

    fn handle_variable_definition_or_assignment(&mut self, node: tree_sitter::Node<'_>) {
        if let Ok(txt) = node.utf8_text(self.source.as_bytes())
            && let Some((var, binding)) = Self::parse_get_binding(txt)
        {
            self.get_bindings.insert(var, binding);
        }

        self.no_output_depth += 1;
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            self.walk(ch);
        }
        self.no_output_depth = self.no_output_depth.saturating_sub(1);
    }

    fn handle_if_action(&mut self, node: tree_sitter::Node<'_>) {
        let saved = self.guards.len();
        let saved_domains = self.range_domains.clone();
        let saved_bindings = self.get_bindings.clone();

        if let Some(cond) = node.child_by_field_name("condition")
            && let Ok(txt) = cond.utf8_text(self.source.as_bytes())
        {
            self.collect_if_with_guards(txt);
        }

        let consequence = Self::children_with_field(node, "consequence");
        for ch in consequence {
            self.walk(ch);
        }

        self.guards.truncate(saved);
        self.range_domains = saved_domains;
        self.get_bindings = saved_bindings;

        // Note: else-if chains are represented as repeated condition/option fields.
        // For now, we only handle the plain else branch.
        let alternative = Self::children_with_field(node, "alternative");
        for ch in alternative {
            self.walk(ch);
        }
    }

    fn handle_with_action(&mut self, node: tree_sitter::Node<'_>) {
        let saved = self.guards.len();
        let saved_dot = self.dot_stack.len();
        let saved_domains = self.range_domains.clone();
        let saved_bindings = self.get_bindings.clone();

        if let Some(cond) = node.child_by_field_name("condition")
            && let Ok(txt) = cond.utf8_text(self.source.as_bytes())
        {
            self.push_with_dot_binding(txt);
            self.collect_if_with_guards(txt);
        }

        let consequence = Self::children_with_field(node, "consequence");
        for ch in consequence {
            self.walk(ch);
        }

        self.guards.truncate(saved);
        self.dot_stack.truncate(saved_dot);
        self.range_domains = saved_domains;
        self.get_bindings = saved_bindings;

        let alternative = Self::children_with_field(node, "alternative");
        for ch in alternative {
            self.walk(ch);
        }
    }

    fn handle_range_action(&mut self, node: tree_sitter::Node<'_>) {
        let saved = self.guards.len();
        let saved_dot = self.dot_stack.len();
        let saved_domains = self.range_domains.clone();
        let saved_bindings = self.get_bindings.clone();

        let mut header_text: Option<String> = None;
        let mut has_variable_definition = false;
        {
            let mut w = node.walk();
            for ch in node.named_children(&mut w) {
                if ch.kind() == "range_variable_definition" {
                    has_variable_definition = true;
                    break;
                }
            }
        }

        let mut body_emits_sequence_item = false;
        if !has_variable_definition {
            for ch in Self::children_with_field(node, "body") {
                if let Ok(txt) = ch.utf8_text(self.source.as_bytes()) {
                    for line in txt.lines() {
                        let t = line.trim_start();
                        if t.starts_with("- ") || t == "-" {
                            body_emits_sequence_item = true;
                            break;
                        }
                    }
                }
                if body_emits_sequence_item {
                    break;
                }
            }
        }
        if let Some(txt) = self.range_header_text(node) {
            header_text = Some(txt.clone());
            if let Some((var, lits)) = Self::parse_literal_list_range(&txt) {
                self.range_domains.insert(var, lits);
            }
            let guard_path = if has_variable_definition || body_emits_sequence_item {
                self.shape.current_path()
            } else {
                YamlPath(Vec::new())
            };
            self.collect_range_guards(&txt, guard_path);
        }

        // If the range header is a single `.Values.*` path, treat `.` inside the range body as an
        // item of that array and rewrite `.foo` to `.Values.<path>.*.foo`.
        //
        // This is a best-effort heuristic that improves inference for common patterns like:
        //   {{- range .Values.someList }}
        //     {{ .name }}
        //   {{- end }}
        let dot_prefix = header_text
            .as_deref()
            .map(extract_values_paths)
            .filter(|v| v.len() == 1)
            .and_then(|v| v.into_iter().next())
            .map(|p| format!("{p}.*"));

        self.dot_stack.push(dot_prefix);

        let body = Self::children_with_field(node, "body");
        for ch in body {
            self.walk(ch);
        }

        self.guards.truncate(saved);
        self.dot_stack.truncate(saved_dot);
        self.range_domains = saved_domains;
        self.get_bindings = saved_bindings;

        let alternative = Self::children_with_field(node, "alternative");
        for ch in alternative {
            self.walk(ch);
        }
    }
}

#[derive(Default, Clone, Debug)]
struct ResourceDetector {
    kind: Option<String>,
    api_versions: BTreeSet<String>,
    current: Option<ResourceRef>,
    header_done: bool,
    buf: String,
}

impl ResourceDetector {
    fn current(&self) -> Option<ResourceRef> {
        self.current.clone()
    }

    fn api_version_rank(api_version: &str) -> (u8, i32) {
        // Lower is better.
        // 0 = stable, 1 = beta, 2 = alpha, 3 = unknown.
        // For the second component, we prefer higher major versions.
        let (_, ver) = api_version.split_once('/').unwrap_or(("", api_version));

        let stability = if ver.contains("alpha") {
            2u8
        } else if ver.contains("beta") {
            1u8
        } else if ver.starts_with('v')
            && ver[1..].chars().next().is_some_and(|c| c.is_ascii_digit())
            && ver[1..].chars().all(|c| c.is_ascii_digit())
        {
            0u8
        } else {
            3u8
        };

        let major = ver
            .strip_prefix('v')
            .and_then(|s| {
                s.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<i32>()
                    .ok()
            })
            .unwrap_or(0);

        (stability, -major)
    }

    fn preferred_api_version(api_versions: &BTreeSet<String>) -> Option<String> {
        api_versions
            .iter()
            .min_by_key(|v| Self::api_version_rank(v))
            .cloned()
    }

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
            det.kind = None;
            det.api_versions.clear();
            det.current = None;
            det.header_done = false;
            return;
        }

        if indent != 0 {
            return;
        }

        // Only consider resource identity fields (`apiVersion`/`kind`) in the document
        // header before we've entered the first top-level object (e.g. `metadata:`, `spec:`).
        // This avoids picking up nested keys like `fieldRef.apiVersion: v1`.
        if det.header_done {
            if det.kind.is_none()
                && let Some(v) = Self::parse_literal_value(trimmed, "kind")
            {
                det.kind = Some(v);
            }
        } else {
            if det.kind.is_none()
                && let Some(v) = Self::parse_literal_value(trimmed, "apiVersion")
            {
                det.api_versions.insert(v);
            }

            if det.kind.is_none()
                && let Some(v) = Self::parse_literal_value(trimmed, "kind")
            {
                det.kind = Some(v);
            }

            // Once we see any other top-level key, stop scanning for apiVersion/kind.
            if det.kind.is_some()
                && !trimmed.starts_with("apiVersion")
                && !trimmed.starts_with("kind")
            {
                det.header_done = true;
            }
        }

        if let Some(kind) = &det.kind {
            let api_version = Self::preferred_api_version(&det.api_versions).unwrap_or_default();
            let api_version_candidates = det
                .api_versions
                .iter()
                .filter(|v| **v != api_version)
                .cloned()
                .collect::<Vec<_>>();
            det.current = Some(ResourceRef {
                api_version,
                kind: kind.clone(),
                api_version_candidates,
            });
        }
    }

    fn ingest(&mut self, text: &str) {
        self.buf.push_str(text);
        while let Some(nl) = self.buf.find('\n') {
            let line = self.buf[..nl].to_string();
            self.buf.drain(..=nl);
            Self::process_line(self, &line);
        }
    }
}
