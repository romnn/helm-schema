use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::{DefineIndex, HelmAst, Literal, TemplateExpr, parse_action_expressions};

use crate::helper_eval::{CapabilityGuard, HelperBranch, HelperBranchBody, HelperOutput};
use crate::walker::{
    extract_values_paths, is_fragment_expr, parse_condition, values_path_from_expr,
};
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
    root_bindings: HashMap<String, HelperBinding>,
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
enum HelperBinding {
    ValuesPath(String),
    RootContext,
}

#[derive(Default)]
struct BoundHelperAnalysis {
    output: BTreeSet<String>,
    guards: BTreeSet<String>,
    suppress_roots: BTreeSet<String>,
}

impl BoundHelperAnalysis {
    fn extend(&mut self, other: Self) {
        self.output.extend(other.output);
        self.guards.extend(other.guards);
        self.suppress_roots.extend(other.suppress_roots);
    }
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

fn parse_yaml_key(after: &str) -> Option<(String, bool)> {
    fn finalize_yaml_key(key: String, rest: &str) -> Option<(String, bool)> {
        if key.is_empty() {
            return None;
        }
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(':').unwrap_or(rest);
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
        Some((key, !is_block))
    }

    let after = after.trim_end();
    if after.starts_with("{{") {
        return None;
    }

    if let Some(quote) = after.chars().next().filter(|c| matches!(c, '"' | '\'')) {
        let quoted = &after[quote.len_utf8()..];
        let mut chars = quoted.char_indices().peekable();
        let mut key = String::new();
        while let Some((idx, ch)) = chars.next() {
            match (quote, ch) {
                ('"', '\\') => {
                    let Some((_, escaped)) = chars.next() else {
                        return None;
                    };
                    key.push(escaped);
                }
                ('\'', '\'') if chars.peek().is_some_and(|(_, next)| *next == '\'') => {
                    let _ = chars.next();
                    key.push('\'');
                }
                (_, ch) if ch == quote => {
                    let rest = &quoted[(idx + ch.len_utf8())..];
                    return finalize_yaml_key(key, rest);
                }
                _ => key.push(ch),
            }
        }
        return None;
    }

    let mut chars = after.chars();
    let mut key = String::new();
    while let Some(c) = chars.next() {
        if c == ':' {
            return finalize_yaml_key(key.trim().to_string(), chars.as_str());
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

fn first_mapping_colon_offset(line: &str) -> Option<usize> {
    // Find the first YAML mapping separator on the physical line while skipping
    // quoted scalars and Helm actions. This lets us distinguish template output
    // that contributes to a mapping key from output that contributes to the
    // mapping value.
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'{' if bytes.get(idx + 1) == Some(&b'{') => {
                idx += 2;
                while idx + 1 < bytes.len() {
                    if bytes[idx] == b'}' && bytes[idx + 1] == b'}' {
                        idx += 2;
                        break;
                    }
                    idx += 1;
                }
            }
            b'"' => {
                idx += 1;
                while idx < bytes.len() {
                    match bytes[idx] {
                        b'\\' => idx += 2,
                        b'"' => {
                            idx += 1;
                            break;
                        }
                        _ => idx += 1,
                    }
                }
            }
            b'\'' => {
                idx += 1;
                while idx < bytes.len() {
                    if bytes[idx] == b'\'' {
                        if bytes.get(idx + 1) == Some(&b'\'') {
                            idx += 2;
                            continue;
                        }
                        idx += 1;
                        break;
                    }
                    idx += 1;
                }
            }
            b':' => return Some(idx),
            _ => idx += 1,
        }
    }
    None
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
    fn shape_text_for_gap(gap: &str) -> String {
        if !(gap.contains("{{") || gap.contains("}}")) {
            gap.to_string()
        } else {
            // Preserve the literal YAML bytes around inline template actions so
            // shape tracking still sees sequence markers, keys, and scalar
            // prefixes on mixed text/template lines.
            let mut sanitized = String::with_capacity(gap.len());
            let bytes = gap.as_bytes();
            let mut idx = 0usize;
            let mut in_action = false;
            while idx < bytes.len() {
                if !in_action && bytes.get(idx..idx + 2) == Some(b"{{") {
                    in_action = true;
                    idx += 2;
                    continue;
                }
                if in_action && bytes.get(idx..idx + 2) == Some(b"}}") {
                    in_action = false;
                    idx += 2;
                    continue;
                }

                let Some(ch) = gap[idx..].chars().next() else {
                    break;
                };
                if !in_action || matches!(ch, '\n' | ' ' | '\t') {
                    sanitized.push(ch);
                }
                idx += ch.len_utf8();
            }
            sanitized
        }
    }

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
            root_bindings: HashMap::new(),
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

    fn with_helper_bindings(mut self, bindings: HashMap<String, HelperBinding>) -> Self {
        self.root_bindings = bindings;
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
        let mut literals = Vec::new();
        let mut rest = text;
        while let Some(i) = rest.find(&needle) {
            let after = &rest[(i + needle.len())..];
            if let Some(end) = after.find('"') {
                let lit = &after[..end];
                if !lit.is_empty() {
                    literals.push(lit.to_string());
                }
                rest = &after[end..];
            } else {
                break;
            }
        }
        literals
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

        // Two parallel buffers — `shape_buf` keeps the gap content
        // sanitized to whitespace (so the indent/shape tracker doesn't
        // see template actions as YAML keys), `detector_buf` keeps the
        // raw gap content (so the resource detector can see helper
        // calls like `apiVersion: {{ template "X" . }}` and resolve
        // them statically via `helper_literal_outputs`).
        let mut shape_buf = String::new();
        let mut detector_buf = String::new();

        while self.text_span_idx < self.text_spans.len() {
            let (s, e) = self.text_spans[self.text_span_idx];

            if e <= self.text_pos {
                self.text_span_idx += 1;
                continue;
            }
            if s >= target {
                // The gap from text_pos up to target is purely
                // non-span (template actions etc.). Feed it raw to the
                // detector before bailing so helper-returned
                // apiVersions in that region still get resolved.
                if self.text_pos < target {
                    let gap = &self.source[self.text_pos..target];
                    let shape_gap = Self::shape_text_for_gap(gap);
                    if !shape_gap.is_empty() {
                        shape_buf.push_str(&shape_gap);
                    }
                    detector_buf.push_str(gap);
                    self.text_pos = target;
                }
                break;
            }

            // Fill the leading gap (from text_pos to current span's
            // start) before processing the span itself. Previous
            // calls to `ingest_text_up_to` may have left text_pos in
            // the middle of an inter-span gap; the detector needs to
            // see the template actions in that gap to resolve
            // helper-returned apiVersions correctly.
            if self.text_pos < s {
                let gap = &self.source[self.text_pos..s];
                let shape_gap = Self::shape_text_for_gap(gap);
                if !shape_gap.is_empty() {
                    shape_buf.push_str(&shape_gap);
                }
                detector_buf.push_str(gap);
                self.text_pos = s;
            }

            let start = s.max(self.text_pos);
            let end = e.min(target);
            if start < end {
                let txt = &self.source[start..end];
                shape_buf.push_str(txt);
                detector_buf.push_str(txt);
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
                        let shape_gap = Self::shape_text_for_gap(gap);
                        if !shape_gap.is_empty() {
                            shape_buf.push_str(&shape_gap);
                        }
                        // Raw gap text for the detector: this is where
                        // template actions (`{{ template "X" . }}`)
                        // sit, and the detector needs to see them
                        // verbatim to resolve helper-returned
                        // apiVersions.
                        detector_buf.push_str(gap);
                        self.text_pos = end;
                    }
                }
            } else {
                self.text_pos = target;
            }

            if shape_buf.len() > 4096 || detector_buf.len() > 4096 {
                self.shape.ingest(&shape_buf);
                self.resource_detector.ingest(&detector_buf, self.defines);
                shape_buf.clear();
                detector_buf.clear();
            }
        }

        if self.text_pos < target {
            self.text_pos = target;
        }

        if !shape_buf.is_empty() {
            self.shape.ingest(&shape_buf);
        }
        if !detector_buf.is_empty() {
            self.resource_detector.ingest(&detector_buf, self.defines);
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

    fn starts_template_action_line(&self, byte_pos: usize) -> bool {
        let bytes = self.source.as_bytes();
        let mut line_start = byte_pos.min(bytes.len());
        while line_start > 0 {
            if bytes[line_start - 1] == b'\n' {
                break;
            }
            line_start -= 1;
        }

        let prefix = &self.source[line_start..byte_pos.min(self.source.len())];
        let trimmed = prefix.trim_start();
        trimmed.starts_with("{{")
    }

    fn fragment_indent_width(text: &str) -> Option<usize> {
        fn call_indent_width(expr: &TemplateExpr) -> Option<usize> {
            match expr {
                TemplateExpr::Call { function, args }
                    if matches!(function.as_str(), "indent" | "nindent") =>
                {
                    match args.first() {
                        Some(TemplateExpr::Literal(Literal::Int(n))) => usize::try_from(*n).ok(),
                        Some(TemplateExpr::Parenthesized(inner)) => call_indent_width(inner),
                        _ => None,
                    }
                }
                TemplateExpr::Parenthesized(inner) => call_indent_width(inner),
                TemplateExpr::Pipeline(stages) => stages.iter().rev().find_map(call_indent_width),
                _ => None,
            }
        }

        Self::parse_expr_text(text)
            .iter()
            .rev()
            .find_map(call_indent_width)
    }

    fn sync_action_for_node(&mut self, node: tree_sitter::Node<'_>) {
        if matches!(node.kind(), "text" | "yaml_no_injection_text") {
            return;
        }

        // Only sync placement for emitted template-output nodes. Control-action
        // lines (`if` / `with` / `range` / `define` / `block`) do not
        // contribute YAML structure themselves, so letting their physical
        // indentation mutate the shape stack can incorrectly pop surrounding
        // mapping context before the controlled body is walked.
        if node.kind() != "template_action" {
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
                self.resource_detector.ingest(&sanitized, self.defines);
                self.text_pos = pos;
            }
        }

        let (physical_indent, physical_col) = self.line_indent_and_col(pos);

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
                && let Some(virtual_indent) = Self::fragment_indent_width(text)
                && virtual_indent > physical_indent
            {
                (virtual_indent, virtual_indent)
            } else {
                (physical_indent, physical_col)
            }
        } else {
            (physical_indent, physical_col)
        };

        self.shape
            .sync_action_position(indent, col, allow_clear_pending);
    }

    fn children_with_field<'n>(
        node: tree_sitter::Node<'n>,
        field: &str,
    ) -> Vec<tree_sitter::Node<'n>> {
        let mut cursor = node.walk();
        node.children_by_field_name(field, &mut cursor)
            .filter(tree_sitter::Node::is_named)
            .collect()
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

    fn emit_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) {
        let path = if self.no_output_depth > 0 {
            YamlPath(Vec::new())
        } else {
            path
        };

        let mut guards = self.guards.clone();
        for guard in extra_guards {
            if !guards.contains(guard) {
                guards.push(guard.clone());
            }
        }

        self.uses.push(ValueUse {
            source_expr,
            path,
            kind,
            guards,
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

    fn current_dot_binding(&self) -> Option<HelperBinding> {
        self.dot_stack
            .last()
            .and_then(|prefix| prefix.as_ref().cloned())
            .map(HelperBinding::ValuesPath)
    }

    fn resolved_values_paths_in_context(&self, text: &str) -> Vec<String> {
        let mut paths: BTreeSet<String> = extract_values_paths(text).into_iter().collect();
        let exprs = Self::parse_expr_text(text);

        if !self.root_bindings.is_empty() {
            for expr in &exprs {
                expr.walk(|node| {
                    if let Some(path) = Self::resolve_bound_path_expr(node, &self.root_bindings) {
                        paths.insert(path);
                    }
                });
            }
        }

        if paths.is_empty()
            && let Some(Some(dot_prefix)) = self.dot_stack.last()
        {
            let is_bare_dot = exprs.len() == 1
                && matches!(
                    &exprs[0],
                    TemplateExpr::Field(path) if path.is_empty()
                );
            if text.trim() == "." || is_bare_dot {
                paths.insert(dot_prefix.clone());
            } else {
                let rewritten = rewrite_dot_expr_to_values(text, dot_prefix);
                paths.extend(extract_values_paths(&rewritten));
            }
        }

        paths.into_iter().collect()
    }

    fn condition_guards_in_context(&self, text: &str) -> Vec<Guard> {
        let cond_guards = parse_condition(text);
        if !cond_guards.is_empty() {
            return cond_guards;
        }
        self.resolved_values_paths_in_context(text)
            .into_iter()
            .map(|path| Guard::Truthy { path })
            .collect()
    }

    fn single_resolved_values_path(&self, text: &str) -> Option<String> {
        let mut paths = self.resolved_values_paths_in_context(text);
        if paths.len() == 1 { paths.pop() } else { None }
    }

    fn is_direct_path_expr(expr: &TemplateExpr, bindings: &HashMap<String, HelperBinding>) -> bool {
        match expr {
            TemplateExpr::Parenthesized(inner) => Self::is_direct_path_expr(inner, bindings),
            TemplateExpr::Field(_) => true,
            TemplateExpr::Selector { .. } => {
                Self::resolve_bound_path_expr(expr, bindings).is_some()
            }
            _ => false,
        }
    }

    fn single_direct_iterable_range_path(&self, text: &str) -> Option<String> {
        let exprs = Self::parse_expr_text(text);
        if exprs.len() != 1 || !Self::is_direct_path_expr(&exprs[0], &self.root_bindings) {
            return None;
        }
        self.single_resolved_values_path(text)
    }

    fn define_body_source(&self, name: &str) -> Option<String> {
        for (_path, src) in self.defines.file_sources() {
            for block in crate::extract_define_blocks(src) {
                if block.name == name {
                    return Some(block.body);
                }
            }
        }
        None
    }

    fn define_body_resource(&self, name: &str) -> Option<ResourceRef> {
        let body = self.defines.get(name)?;
        let ast = HelmAst::Document {
            items: body.to_vec(),
        };
        crate::ResourceDetector::detect(&crate::DefaultResourceDetector, &ast)
    }

    fn inline_exact_helper_call(&mut self, text: &str) -> bool {
        let exprs = Self::parse_expr_text(text);
        if exprs.len() != 1 {
            return false;
        }

        let TemplateExpr::Call { function, args } = &exprs[0] else {
            return false;
        };
        if !matches!(function.as_str(), "include" | "template") {
            return false;
        }
        let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
            return false;
        };
        if self.define_body_resource(name).is_none() {
            return false;
        }

        let Some(src) = self.define_body_source(name) else {
            return false;
        };
        let token = format!("define:{name}");
        if self.inline_stack.iter().any(|entry| entry == &token) {
            return false;
        }
        let Some(tree) = Self::parse_go_template(&src) else {
            return false;
        };

        let current_dot = self.current_dot_binding();
        let bindings = Self::bindings_for_helper_arg(
            args.get(1),
            Some(&self.root_bindings),
            current_dot.as_ref(),
        );
        let mut stack = self.inline_stack.clone();
        stack.push(token);
        let mut nested = SymbolicWalker::new(src.as_str(), self.defines)
            .with_initial_guards(self.guards.clone())
            .with_inline_stack(stack)
            .with_inline_helpers_in_fragments(true)
            .with_helper_bindings(bindings);
        let uses = nested.run(&tree);
        self.uses.extend(uses);
        true
    }

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        let Ok(text) = node.utf8_text(self.source.as_bytes()) else {
            return;
        };

        self.maybe_inline_nats_load_merge_patch(text);

        let kind = if is_fragment_expr(text) {
            ValueKind::Fragment
        } else {
            ValueKind::Scalar
        };

        let helper_inlined = self.inline_exact_helper_call(text);

        let values = self.resolved_values_paths_in_context(text);

        let bound_values = self.extract_bound_values(text);

        let mut helper_output_values: Vec<String> = Vec::new();
        let mut helper_guard_values: Vec<String> = Vec::new();
        let mut suppress_direct_values: BTreeSet<String> = BTreeSet::new();
        if !helper_inlined {
            for name in Self::literal_template_calls(text) {
                if name.ends_with(".defaultValues") {
                    continue;
                }
                let v = self.values_from_define(&name);
                helper_output_values.extend(v.output);
                helper_guard_values.extend(v.guards);
            }
            let bound = self.analyze_bound_helper_calls(text);
            helper_output_values.extend(bound.output);
            helper_guard_values.extend(bound.guards);
            suppress_direct_values.extend(bound.suppress_roots);
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

        let in_mapping_key = self.output_node_is_mapping_key_part(node);
        let mut path = if in_mapping_key {
            YamlPath(Vec::new())
        } else {
            self.shape.current_path()
        };
        if kind == ValueKind::Fragment
            && let Ok(node_text) = node.utf8_text(self.source.as_bytes())
        {
            let (physical_indent, _physical_col) = self.line_indent_and_col(node.start_byte());
            if self.starts_template_action_line(node.start_byte()) {
                let mut logical_indent = physical_indent;
                if let Some(virtual_indent) = Self::fragment_indent_width(node_text) {
                    logical_indent = virtual_indent;
                }

                let trailing_pending_segments = self
                    .shape
                    .stack
                    .iter()
                    .rev()
                    .take_while(|(indent, kind, pending)| {
                        *indent >= logical_indent
                            && *kind == Container::Mapping
                            && pending.is_some()
                    })
                    .count();
                for _ in 0..trailing_pending_segments {
                    path.0.pop();
                }
            }
        }
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
            if suppress_direct_values.contains(&v) {
                self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
                continue;
            }
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

    fn output_node_is_mapping_key_part(&self, node: tree_sitter::Node<'_>) -> bool {
        let start = node.start_byte();
        let end = node.end_byte();
        let line_start = self.source[..start].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = self.source[end..]
            .find('\n')
            .map_or(self.source.len(), |idx| end + idx);
        let line = &self.source[line_start..line_end];
        let rel_start = start - line_start;
        let rel_end = end - line_start;
        let Some(colon_offset) = first_mapping_colon_offset(line) else {
            return false;
        };
        // A template action used before the first mapping separator contributes
        // to key construction (for example `{{ .name }}.json: ...`), not to the
        // parent value slot.
        rel_start < colon_offset && rel_end <= colon_offset
    }

    fn parse_expr_text(text: &str) -> Vec<TemplateExpr> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        if trimmed.starts_with("{{") {
            parse_action_expressions(trimmed)
        } else {
            parse_action_expressions(&format!("{{{{ {trimmed} }}}}"))
        }
    }

    fn resolve_bound_path_expr(
        expr: &TemplateExpr,
        bindings: &HashMap<String, HelperBinding>,
    ) -> Option<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(path);
        }

        match expr {
            TemplateExpr::Parenthesized(inner) => Self::resolve_bound_path_expr(inner, bindings),
            TemplateExpr::Field(path) => Self::resolve_bound_segments(path, bindings),
            TemplateExpr::Selector { operand, path } => {
                if let Some(binding) = Self::binding_from_expr(operand, Some(bindings), None) {
                    return Self::apply_binding(&binding, path);
                }
                None
            }
            _ => None,
        }
    }

    fn resolve_bound_segments(
        segments: &[String],
        bindings: &HashMap<String, HelperBinding>,
    ) -> Option<String> {
        let (first, rest) = segments.split_first()?;
        let binding = bindings.get(first)?;
        Self::apply_binding(binding, rest)
    }

    fn apply_binding(binding: &HelperBinding, rest: &[String]) -> Option<String> {
        match binding {
            HelperBinding::ValuesPath(prefix) => {
                if rest.is_empty() {
                    Some(prefix.clone())
                } else {
                    Some(format!("{prefix}.{}", rest.join(".")))
                }
            }
            HelperBinding::RootContext => {
                if rest.first().map(String::as_str) == Some("Values") && rest.len() > 1 {
                    Some(rest[1..].join("."))
                } else {
                    None
                }
            }
        }
    }

    fn binding_from_expr(
        expr: &TemplateExpr,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> Option<HelperBinding> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(HelperBinding::ValuesPath(path));
        }

        match expr {
            TemplateExpr::Parenthesized(inner) => {
                Self::binding_from_expr(inner, outer, current_dot)
            }
            TemplateExpr::Field(path) if path.is_empty() => {
                current_dot.cloned().or(Some(HelperBinding::RootContext))
            }
            TemplateExpr::Variable(var) if var.is_empty() => Some(HelperBinding::RootContext),
            _ => outer
                .and_then(|bindings| Self::resolve_bound_path_expr(expr, bindings))
                .map(HelperBinding::ValuesPath),
        }
    }

    fn bindings_for_helper_arg(
        arg: Option<&TemplateExpr>,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> HashMap<String, HelperBinding> {
        let Some(arg) = arg else {
            return HashMap::new();
        };

        match arg {
            TemplateExpr::Parenthesized(inner) => {
                Self::bindings_for_helper_arg(Some(inner), outer, current_dot)
            }
            TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
            TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut bindings = HashMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key)) = &args[index] else {
                        index += 1;
                        continue;
                    };
                    if let Some(binding) =
                        Self::binding_from_expr(&args[index + 1], outer, current_dot)
                    {
                        bindings.insert(key.clone(), binding);
                    }
                    index += 2;
                }
                bindings
            }
            _ => HashMap::new(),
        }
    }

    fn analyze_bound_helper_calls(&self, text: &str) -> BoundHelperAnalysis {
        let mut seen = HashSet::new();
        let current_dot = self.current_dot_binding();
        Self::analyze_bound_helper_calls_with_bindings(
            text,
            None,
            current_dot.as_ref(),
            self.defines,
            &mut seen,
        )
    }

    fn analyze_bound_helper_calls_with_bindings(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        defines: &DefineIndex,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        let mut analysis = BoundHelperAnalysis::default();
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if !matches!(function.as_str(), "include" | "template") {
                    return;
                }
                let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                    return;
                };
                let nested = Self::analyze_bound_helper_call(
                    name,
                    args.get(1),
                    bindings,
                    current_dot,
                    defines,
                    seen,
                );
                analysis.extend(nested);
            });
        }
        analysis
    }

    fn analyze_bound_helper_call(
        name: &str,
        arg: Option<&TemplateExpr>,
        outer_bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        defines: &DefineIndex,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        if !seen.insert(name.to_string()) {
            return BoundHelperAnalysis::default();
        }

        let bindings = Self::bindings_for_helper_arg(arg, outer_bindings, current_dot);
        let mut analysis = BoundHelperAnalysis::default();
        if let Some(body) = defines.get(name) {
            for node in body {
                Self::collect_bound_helper_values_from_ast(
                    node,
                    &bindings,
                    defines,
                    seen,
                    &mut analysis,
                );
            }
        }

        for binding in bindings.values() {
            let HelperBinding::ValuesPath(root) = binding else {
                continue;
            };
            let prefix = format!("{root}.");
            if analysis
                .output
                .iter()
                .chain(analysis.guards.iter())
                .any(|path| path.starts_with(&prefix))
            {
                analysis.suppress_roots.insert(root.clone());
            }
        }

        seen.remove(name);
        analysis
    }

    fn collect_bound_paths_from_text(
        text: &str,
        bindings: &HashMap<String, HelperBinding>,
        defines: &DefineIndex,
        seen: &mut HashSet<String>,
        into: &mut BTreeSet<String>,
    ) {
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                if let Some(path) = Self::resolve_bound_path_expr(node, bindings) {
                    into.insert(path);
                }
            });
        }

        let nested = Self::analyze_bound_helper_calls_with_bindings(
            text,
            Some(bindings),
            None,
            defines,
            seen,
        );
        into.extend(nested.output);
        into.extend(nested.guards);
    }

    fn collect_bound_helper_values_from_ast(
        node: &HelmAst,
        bindings: &HashMap<String, HelperBinding>,
        defines: &DefineIndex,
        seen: &mut HashSet<String>,
        analysis: &mut BoundHelperAnalysis,
    ) {
        match node {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Sequence { items }
            | HelmAst::Define { body: items, .. }
            | HelmAst::Block { body: items, .. } => {
                for item in items {
                    Self::collect_bound_helper_values_from_ast(
                        item, bindings, defines, seen, analysis,
                    );
                }
            }
            HelmAst::Pair { key, value } => {
                Self::collect_bound_helper_values_from_ast(key, bindings, defines, seen, analysis);
                if let Some(value) = value {
                    Self::collect_bound_helper_values_from_ast(
                        value, bindings, defines, seen, analysis,
                    );
                }
            }
            HelmAst::HelmExpr { text } => {
                Self::collect_bound_paths_from_text(
                    text,
                    bindings,
                    defines,
                    seen,
                    &mut analysis.output,
                );
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                Self::collect_bound_paths_from_text(
                    cond,
                    bindings,
                    defines,
                    seen,
                    &mut analysis.guards,
                );
                for item in then_branch {
                    Self::collect_bound_helper_values_from_ast(
                        item, bindings, defines, seen, analysis,
                    );
                }
                for item in else_branch {
                    Self::collect_bound_helper_values_from_ast(
                        item, bindings, defines, seen, analysis,
                    );
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
                Self::collect_bound_paths_from_text(
                    header,
                    bindings,
                    defines,
                    seen,
                    &mut analysis.guards,
                );
                for item in body {
                    Self::collect_bound_helper_values_from_ast(
                        item, bindings, defines, seen, analysis,
                    );
                }
                for item in else_branch {
                    Self::collect_bound_helper_values_from_ast(
                        item, bindings, defines, seen, analysis,
                    );
                }
            }
            HelmAst::Scalar { .. } | HelmAst::HelmComment { .. } => {}
        }
    }

    fn collect_if_with_guards(&mut self, cond_text: &str) {
        let cond_guards = self.condition_guards_in_context(cond_text);

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

    fn collect_with_guards(&mut self, cond_text: &str) {
        let cond_guards = self.condition_guards_in_context(cond_text);

        // In a `with` block, every path that contributes to the binding is
        // null-tolerant (`with nil` skips the body). Tag each such path with
        // `Guard::With { path }` so downstream consumers can identify
        // with-bound paths uniformly:
        //
        //   `with .Values.X`           → Truthy{X}      → With{X}
        //   `with or .A .B`            → Or{[A,B]}      → With{A}, With{B}, Or{[A,B]}
        //   `with and (.A) (.B)`       → Truthy{A,B}    → With{A}, With{B}
        //
        // For non-trivial control flow (`Or`, `Not`, `Eq`) we KEEP the
        // original guard alongside the per-path `With` so downstream
        // consumers retain exact semantics. `Truthy { path }` is fully
        // subsumed by `With { path }` and is dropped.
        let cond_guards: Vec<Guard> = cond_guards
            .into_iter()
            .flat_map(|g| match g {
                Guard::Truthy { path } => vec![Guard::With { path }],
                Guard::Or { ref paths } => {
                    let mut out: Vec<Guard> = paths
                        .iter()
                        .map(|p| Guard::With { path: p.clone() })
                        .collect();
                    out.push(g);
                    out
                }
                Guard::Not { ref path } | Guard::Eq { ref path, .. } => {
                    vec![Guard::With { path: path.clone() }, g]
                }
                Guard::Range { .. } => vec![g],
                Guard::With { .. } => vec![g],
            })
            .collect();

        // Push the With guards before emitting header scalar uses so the
        // emitted uses themselves carry the With guard. This lets the schema
        // generator identify with-header uses by the presence of a matching
        // `Guard::With { path: source_expr }` in the use's guard list.
        for g in &cond_guards {
            if !self.guards.contains(g) {
                self.guards.push(g.clone());
            }
        }

        for v in self.extract_bound_values(cond_text) {
            self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
        }

        for g in &cond_guards {
            for path in g.value_paths() {
                self.emit_use(path.to_string(), YamlPath(Vec::new()), ValueKind::Scalar);
            }
        }
    }

    fn push_with_dot_binding(&mut self, header_text: &str) {
        if let Some(path) = self.single_resolved_values_path(header_text) {
            self.dot_stack.push(Some(path));
        } else {
            self.dot_stack.push(None);
        }
    }

    fn collect_range_guards(&mut self, header_text: &str, path: &YamlPath, emit_use: bool) {
        let values = self.range_source_paths(header_text);
        for v in &values {
            let guard = Guard::Range { path: v.clone() };
            if emit_use {
                self.emit_use_with_extra_guards(
                    v.clone(),
                    path.clone(),
                    ValueKind::Scalar,
                    std::slice::from_ref(&guard),
                );
            }
            if !self.guards.contains(&guard) {
                self.guards.push(guard);
            }
        }
    }

    fn range_source_paths(&self, header_text: &str) -> Vec<String> {
        self.resolved_values_paths_in_context(header_text)
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

    fn range_body_renders_scalar_sequence_items(&self, node: tree_sitter::Node<'_>) -> bool {
        let mut saw_sequence_item = false;
        let mut body_text = String::new();

        for body_node in Self::children_with_field(node, "body") {
            let Ok(text) = body_node.utf8_text(self.source.as_bytes()) else {
                continue;
            };
            body_text.push_str(text);
        }

        for line in body_text.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix('-') else {
                continue;
            };
            let rest = rest.trim_start();
            saw_sequence_item = true;

            if rest.is_empty() || parse_yaml_key(rest).is_some() || is_fragment_expr(rest) {
                return false;
            }
        }

        saw_sequence_item
    }

    fn direct_iterable_header_path(&self, header_text: &str) -> Option<String> {
        let mut txt = header_text.trim();
        loop {
            let trimmed = txt.trim();
            if trimmed.len() >= 2 && trimmed.starts_with('(') && trimmed.ends_with(')') {
                txt = &trimmed[1..trimmed.len() - 1];
                continue;
            }
            break;
        }

        self.single_direct_iterable_range_path(txt)
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
            | "dot"
            | "field"
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
            self.collect_with_guards(txt);
            self.push_with_dot_binding(txt);
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
        for ch in Self::children_with_field(node, "body") {
            if let Ok(txt) = ch.utf8_text(self.source.as_bytes()) {
                for line in txt.lines() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("- ") || trimmed == "-" {
                        body_emits_sequence_item = true;
                        break;
                    }
                }
            }
            if body_emits_sequence_item {
                break;
            }
        }
        let body_renders_scalar_sequence_items =
            !has_variable_definition && self.range_body_renders_scalar_sequence_items(node);
        if let Some(txt) = self.range_header_text(node) {
            header_text = Some(txt.clone());
            if let Some((var, literals)) = Self::parse_literal_list_range(&txt) {
                self.range_domains.insert(var, literals);
            }
            let current_path = self.shape.current_path();
            let direct_iterable_header_path = self.direct_iterable_header_path(&txt);
            let guard_path = if has_variable_definition {
                // Destructured range headers (`range $k, $v := .Values.map`) describe
                // the INPUT collection, not the rendered YAML shape of each output item.
                // Attaching the current rendered path here lets downstream provider
                // schemas project output arrays (for example `env:`) back onto map-like
                // chart inputs such as `.Values.environment`, producing bogus
                // `object | array` unions. Keep the header use pathless; values.yaml and
                // body uses still carry the input contract.
                YamlPath(Vec::new())
            } else if body_emits_sequence_item
                && body_renders_scalar_sequence_items
                && direct_iterable_header_path.is_some()
            {
                // A direct iterable source contributes the whole collection to the
                // current YAML sequence field only when each input item becomes the
                // rendered sequence item directly (`- {{ . }}`).
                self.shape.current_path()
            } else {
                YamlPath(Vec::new())
            };
            let emit_header_use = has_variable_definition
                || !body_emits_sequence_item
                || (body_renders_scalar_sequence_items && direct_iterable_header_path.is_some());
            self.collect_range_guards(&txt, &guard_path, emit_header_use);

            let renders_mapping_entries = has_variable_definition
                && !body_emits_sequence_item
                && !current_path.0.is_empty()
                && current_path
                    .0
                    .last()
                    .is_some_and(|segment| !segment.ends_with("[*]"));
            if renders_mapping_entries {
                // A destructured map range under a concrete object field
                // (`annotations:`, `matchLabels:`, ...) is effectively
                // rendering a YAML fragment for the whole source map.
                // Keep the header's scalar use pathless to avoid projecting
                // array output shapes like `env:` back onto map inputs, and
                // emit this separate fragment use so provider object schemas
                // can still type the destination field precisely.
                for source_path in self.range_source_paths(&txt) {
                    self.emit_use(source_path, current_path.clone(), ValueKind::Fragment);
                }
            }
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
            .and_then(|raw| self.direct_iterable_header_path(raw))
            .map(|path| format!("{path}.*"));

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
    /// Insertion-ordered, dedup'd list of apiVersions observed in the
    /// current document header. Source order matters for attribution:
    /// the first apiVersion the template author writes is the primary
    /// (their stated intent). Generic "stable beats beta" ranking is
    /// wrong for resource detection — `PodSecurityPolicy` only exists
    /// at `policy/v1beta1`, so ranking would pick `policy/v1` (stable)
    /// from a multi-branch helper and produce misleading
    /// `MissingSchema(policy/v1)` diagnostics.
    api_versions: Vec<String>,
    /// `true` when at least one `apiVersion:` line in this document's
    /// header was resolved from a helper or inline `if` chain that
    /// produced MULTIPLE literal outputs. Branches are NOT
    /// undifferentiated literals: the choice is conditioned on
    /// `.Capabilities.APIVersions.Has` — the chain layer evaluates
    /// those guards against its K8s version cache and picks the live
    /// branch. Picking a primary via source-order would be heuristic
    /// collapse.
    multi_branch_helper: bool,
    /// Typed branch structure observed in this document's apiVersion
    /// resolution: either harvested from a helper body via
    /// `helper_evaluate` returning `HelperOutput::Branched`, or from
    /// inline `{{- if .Capabilities.APIVersions.Has "X" -}}` /
    /// `{{- else -}}` lines wrapping a literal `apiVersion:` line in
    /// the document header. Populated for the elasticsearch PSP shape
    /// (inline if/else around `apiVersion:`) and the grafana HPA shape
    /// (helper-returned multi-branch apiVersion).
    api_version_branches: Vec<HelperBranch>,
    /// State machine for tracking the innermost inline `if` whose
    /// condition is a decodable `Capabilities.APIVersions.Has` guard.
    /// When active, literal `apiVersion:` values seen while inside the
    /// if/else/end block are attributed to the current branch.
    inline_if: Option<InlineIfState>,
    /// Depth of nested `{{- if X -}}` / `{{- with X -}}` / `{{- range X -}}`
    /// blocks. Used to match `{{- else }}` / `{{- end }}` directives to
    /// the right opening directive, so a `{{- else }}` for a nested if
    /// doesn't accidentally close the tracked branch.
    inline_block_depth: usize,
    current: Option<ResourceRef>,
    header_done: bool,
    buf: String,
}

/// Branch-capture state for inline `{{- if Capabilities.APIVersions.Has "X" -}}`
/// chains that decide which `apiVersion:` literal the template emits.
#[derive(Clone, Debug)]
struct InlineIfState {
    /// `inline_block_depth` at the moment this tracker started (i.e.
    /// after the opening `{{- if -}}` incremented depth). Matching
    /// `{{- else }}` / `{{- end }}` are recognised by being seen at
    /// this exact depth — nested ifs / withs / ranges at deeper depths
    /// don't affect the tracker.
    start_depth: usize,
    /// Branches accumulated so far. The last entry is the open branch
    /// that subsequent literals get attributed to. `{{- else if X }}`
    /// and `{{- else }}` close the current branch and open a new one
    /// (with a new guard or unguarded, respectively).
    branches: Vec<HelperBranch>,
}

/// Classification of a single Go-template directive line (the body
/// between `{{` and `}}` after stripping trim modifiers). Only the
/// subset of directives that affects block nesting or apiVersion
/// branch attribution is recognised — everything else is `Other` and
/// has no inline-branch effect.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ActionDirective {
    /// `if <cond>` — captured for branch tracking when `cond` decodes
    /// to a `Capabilities.APIVersions.Has` guard.
    If(String),
    /// `else if <cond>` — same condition extraction as `If`.
    ElseIf(String),
    /// `else`
    Else,
    /// `end`
    End,
    /// `with <expr>` — opens a block (depth +1).
    With,
    /// `range <expr>` — opens a block (depth +1).
    Range,
    /// `define "<name>"` — opens a block (depth +1).
    Define,
    /// `block "<name>" <pipeline>` — opens a block (depth +1).
    Block,
    /// Any other expression / value substitution / pipeline. No
    /// effect on block depth or branch tracking.
    Other,
}

impl ActionDirective {
    fn parse(body: &str) -> Self {
        let trimmed = body.trim();
        if trimmed == "end" {
            return ActionDirective::End;
        }
        if trimmed == "else" {
            return ActionDirective::Else;
        }
        if let Some(rest) = strip_keyword(trimmed, "else if") {
            return ActionDirective::ElseIf(rest.to_string());
        }
        if let Some(rest) = strip_keyword(trimmed, "if") {
            return ActionDirective::If(rest.to_string());
        }
        if strip_keyword(trimmed, "with").is_some() {
            return ActionDirective::With;
        }
        if strip_keyword(trimmed, "range").is_some() {
            return ActionDirective::Range;
        }
        if strip_keyword(trimmed, "define").is_some() {
            return ActionDirective::Define;
        }
        if strip_keyword(trimmed, "block").is_some() {
            return ActionDirective::Block;
        }
        ActionDirective::Other
    }
}

/// Match a leading go-template keyword followed by whitespace, and
/// return the remainder. Whitespace-required so `iframe` doesn't
/// match `if`, etc.
fn strip_keyword<'a>(body: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = body.strip_prefix(keyword)?;
    let next = rest.chars().next()?;
    if next.is_whitespace() {
        Some(rest.trim_start())
    } else {
        None
    }
}

impl ResourceDetector {
    fn current(&self) -> Option<ResourceRef> {
        self.current.clone()
    }

    /// Insert `v` into `api_versions` only if not already present
    /// (dedup), preserving source order.
    fn insert_api_version(&mut self, v: String) {
        if !v.is_empty() && !self.api_versions.contains(&v) {
            self.api_versions.push(v);
        }
    }

    fn first_seen_api_version(&self) -> Option<String> {
        self.api_versions.first().cloned()
    }

    /// Record a literal apiVersion value from the document header. If
    /// an inline `if .Capabilities.APIVersions.Has` branch is active,
    /// also attribute the literal to the current branch so the chain
    /// layer can resolve the apiVersion structurally.
    fn attribute_api_version_literal(&mut self, v: String) {
        if v.is_empty() {
            return;
        }
        if let Some(state) = self.inline_if.as_mut()
            && let Some(branch) = state.branches.last_mut()
            && let HelperBranchBody::Literals { values } = &mut branch.body
            && !values.contains(&v)
        {
            values.push(v.clone());
            // Two literals in the same branch (e.g. someone wrote
            // `apiVersion:` twice inside the same if) or one literal
            // each in then/else makes this multi-branch.
            let multi = state.branches.len() > 1
                || state.branches.iter().any(|b| match &b.body {
                    HelperBranchBody::Literals { values } => values.len() > 1,
                    HelperBranchBody::Nested { .. } => true,
                });
            if multi {
                self.multi_branch_helper = true;
            }
        }
        self.insert_api_version(v);
    }

    /// Process a directive line (`{{- if … }}`, `{{- else }}`, …) for
    /// inline-branch tracking. Maintains `inline_block_depth` and the
    /// `inline_if` state machine.
    fn process_action_directive(det: &mut ResourceDetector, line: &str) {
        let Some(action) = Self::strip_action_wrapping(line) else {
            return;
        };
        let directive = ActionDirective::parse(&action);
        match directive {
            ActionDirective::If(cond) => {
                det.inline_block_depth += 1;
                if det.inline_if.is_none() {
                    let guard = crate::helper_eval::decode_guard(&cond);
                    if matches!(
                        guard,
                        CapabilityGuard::Has { .. } | CapabilityGuard::NotHas { .. }
                    ) {
                        det.inline_if = Some(InlineIfState {
                            start_depth: det.inline_block_depth,
                            branches: vec![HelperBranch::with_literals(Some(guard), Vec::new())],
                        });
                    }
                }
            }
            ActionDirective::ElseIf(cond) => {
                if let Some(state) = det.inline_if.as_mut()
                    && state.start_depth == det.inline_block_depth
                {
                    let guard = crate::helper_eval::decode_guard(&cond);
                    state
                        .branches
                        .push(HelperBranch::with_literals(Some(guard), Vec::new()));
                }
            }
            ActionDirective::Else => {
                if let Some(state) = det.inline_if.as_mut()
                    && state.start_depth == det.inline_block_depth
                {
                    state
                        .branches
                        .push(HelperBranch::with_literals(None, Vec::new()));
                }
            }
            ActionDirective::End => {
                if let Some(state) = det.inline_if.as_ref()
                    && state.start_depth == det.inline_block_depth
                {
                    let mut branches = det.inline_if.take().unwrap().branches;
                    // Drop branches that ended up with no body content
                    // AND no guard (purely structural empty branches
                    // add no information). Branches with a guard but
                    // no literals carry information (the guard might
                    // gate apiVersion from a helper or other source),
                    // so keep them.
                    branches.retain(|b| !b.body.is_empty() || b.guard.is_some());
                    if !branches.is_empty() {
                        det.api_version_branches.extend(branches);
                        det.multi_branch_helper = true;
                    }
                }
                if det.inline_block_depth > 0 {
                    det.inline_block_depth -= 1;
                }
            }
            ActionDirective::With | ActionDirective::Range => {
                det.inline_block_depth += 1;
            }
            ActionDirective::Define | ActionDirective::Block => {
                det.inline_block_depth += 1;
            }
            ActionDirective::Other => {}
        }
    }

    /// Strip the `{{`, `}}`, leading `-`, trailing `-`, and outer
    /// whitespace from a directive line, returning the inner body
    /// (e.g. `"if .Foo"`, `"else"`, `"end"`). Returns `None` for
    /// lines that don't start with `{{` or are otherwise malformed —
    /// they're treated as `Other` (no inline-branch effect).
    fn strip_action_wrapping(line: &str) -> Option<String> {
        let after_open = line.trim_start().strip_prefix("{{")?;
        let close_at = after_open.find("}}")?;
        let body = &after_open[..close_at];
        let body = body.strip_prefix('-').unwrap_or(body);
        let body = body.strip_suffix('-').unwrap_or(body);
        Some(body.trim().to_string())
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

    /// Extract a helper invocation name from
    /// `<key>: {{ template "NAME" … }}` / `<key>: {{ include "NAME" … }}`
    /// (with optional `-` trim modifiers, either quote style,
    /// arbitrary trailing args, and pipelines). Returns `None` if the
    /// value isn't a helper call.
    ///
    /// Uses the shared tree-sitter-based action parser so quoting,
    /// pipelines, and nested calls in arg positions are handled
    /// structurally — this used to be a hand-rolled `strip_prefix` /
    /// `find` chain that would have missed perfectly valid header
    /// shapes (`{{ template "X" . | quote }}`,
    /// `{{ include "X" (dict ...) }}`, etc.).
    fn parse_helper_call_value(line: &str, key: &str) -> Option<String> {
        let rest = line.strip_prefix(key)?;
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(':')?.trim_start();
        if !rest.starts_with("{{") {
            return None;
        }
        // Find the closing `}}` so we don't drag a trailing YAML comment
        // or sibling key into the parse. parse_action_expressions
        // accepts a body, but we want only the first `{{ … }}` action.
        let close_at = rest.find("}}")?;
        let action = &rest[..close_at + 2];
        let exprs = helm_schema_ast::parse_action_expressions(action);

        // Walk every expression and every sub-expression looking for
        // `template "NAME" …` / `include "NAME" …`. `walk` is preorder
        // and visits args too, so it catches nested forms like
        // `{{ printf "%s" (template "X" .) }}`.
        let mut found: Option<String> = None;
        for expr in &exprs {
            if found.is_some() {
                break;
            }
            expr.walk(|node| {
                if found.is_some() {
                    return;
                }
                if let helm_schema_ast::TemplateExpr::Call { function, args } = node
                    && matches!(function.as_str(), "template" | "include")
                    && let Some(helm_schema_ast::TemplateExpr::Literal(lit)) = args.first()
                    && let Some(name) = lit.as_string()
                    && !name.is_empty()
                {
                    found = Some(name.to_string());
                }
            });
        }
        found
    }

    fn process_line(det: &mut ResourceDetector, helpers: &DefineIndex, line: &str) {
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
            det.multi_branch_helper = false;
            det.api_version_branches.clear();
            det.inline_if = None;
            det.inline_block_depth = 0;
            det.current = None;
            det.header_done = false;
            return;
        }

        if indent != 0 {
            return;
        }

        // Helm template-directive lines (`{{- if … }}`, `{{- else }}`,
        // `{{- end }}`, `{{- range … }}`, `{{ define … }}`, …) are
        // control flow that wraps header fields, not header content.
        // Two responsibilities here:
        //   1. Track inline branch state for `apiVersion:` lines that
        //      are gated by `{{- if .Capabilities.APIVersions.Has … -}}`
        //      (the elasticsearch PSP shape) so the chain layer can
        //      evaluate the guard against its K8s cache and pick the
        //      live branch instead of treating mutually-exclusive
        //      alternatives as peer candidates.
        //   2. Otherwise pass through unchanged — directive lines
        //      never advance `header_done`, never set `kind`, etc.
        if trimmed.starts_with("{{") {
            Self::process_action_directive(det, trimmed);
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
            // `apiVersion` and `kind` can appear in EITHER order in a
            // K8s YAML document header; neither field gates the other.
            // The old code guarded apiVersion parsing on
            // `det.kind.is_none()`, which silently dropped apiVersion
            // any time `kind:` happened to appear first — exactly the
            // shape `kind: NetworkPolicy\napiVersion:
            // networking.k8s.io/v1` that Temporal's templates ship,
            // producing `api_version=""` resources that then leaked
            // through the chain as `MissingSchema(kind=..., api_version=)`.
            if let Some(v) = Self::parse_literal_value(trimmed, "apiVersion") {
                det.attribute_api_version_literal(v);
            } else if let Some(name) = Self::parse_helper_call_value(trimmed, "apiVersion") {
                // Round-5 Finding 1: `apiVersion: {{ template "X" . }}`
                // and `apiVersion: {{ include "X" . }}` — statically
                // resolve the helper to its typed output so the
                // detector no longer silently drops apiVersion when
                // vendored charts use the common
                // `<chart>.<resource>.apiVersion` helper pattern.
                let output = crate::helper_eval::helper_evaluate(&name, helpers);
                match output {
                    HelperOutput::Literals(literals) => {
                        if literals.len() > 1 {
                            det.multi_branch_helper = true;
                        }
                        for candidate in literals {
                            det.insert_api_version(candidate);
                        }
                    }
                    HelperOutput::Branched { branches } => {
                        // Round-7 Finding 2/3: helper body has typed
                        // branch structure. Propagate the branches
                        // verbatim — the chain will evaluate guards
                        // against its K8s cache and pick the first
                        // live branch. Also flatten into api_versions
                        // for back-compat with callers that don't
                        // honour the branch structure (the typed
                        // chain path will short-circuit before they
                        // see the resource).
                        det.multi_branch_helper = branches.len() > 1
                            || branches.iter().any(|b| match &b.body {
                                HelperBranchBody::Literals { values } => values.len() > 1,
                                HelperBranchBody::Nested { .. } => true,
                            });
                        for branch in &branches {
                            for lit in branch.body.all_literals() {
                                det.insert_api_version(lit);
                            }
                        }
                        det.api_version_branches.extend(branches);
                    }
                }
            }

            if det.kind.is_none()
                && let Some(v) = Self::parse_literal_value(trimmed, "kind")
            {
                det.kind = Some(v);
            }

            // Once we see any other top-level key, stop scanning for
            // apiVersion/kind — but only after BOTH have had a chance
            // to land (we wait for kind because kind-only is the
            // single-shot identifier; further apiVersion lines after
            // a non-header key are unusual and ignored).
            if det.kind.is_some()
                && !trimmed.starts_with("apiVersion")
                && !trimmed.starts_with("kind")
            {
                det.header_done = true;
            }
        }

        if let Some(kind) = &det.kind {
            // Round-7 Finding 2/3: when typed branches are present
            // (helper returned `HelperOutput::Branched` or inline
            // `{{- if .Capabilities.APIVersions.Has "X" -}}` chains
            // around `apiVersion:` produced HelperBranch entries), the
            // chain layer evaluates guards against its K8s cache and
            // selects the live branch. Round-6 Finding 2: when only a
            // flat list of multi-branch literals is available (no
            // decoded guard), preserve ALL alternatives as candidates
            // with an EMPTY primary so the chain can pick whichever
            // resolves. Source-order primary collapse would be a
            // heuristic guess.
            let (api_version, api_version_candidates) = if det.multi_branch_helper {
                (String::new(), det.api_versions.clone())
            } else {
                let primary = det.first_seen_api_version().unwrap_or_default();
                let candidates = det
                    .api_versions
                    .iter()
                    .filter(|v| **v != primary)
                    .cloned()
                    .collect::<Vec<_>>();
                (primary, candidates)
            };
            det.current = Some(ResourceRef {
                api_version,
                kind: kind.clone(),
                api_version_candidates,
                api_version_branches: det.api_version_branches.clone(),
            });
        }
    }

    fn ingest(&mut self, text: &str, helpers: &DefineIndex) {
        self.buf.push_str(text);
        while let Some(nl) = self.buf.find('\n') {
            let line = self.buf[..nl].to_string();
            self.buf.drain(..=nl);
            Self::process_line(self, helpers, &line);
        }
    }
}

#[cfg(test)]
mod detector_unit_tests {
    use super::ResourceDetector;
    use helm_schema_ast::{DefineIndex, TreeSitterParser};

    #[test]
    fn helper_returned_api_version_resolves_via_detector_directly() {
        let helpers_src = r#"{{- define "x.apiVersion" -}}
{{- print "apps/v1" -}}
{{- end -}}"#;
        let mut idx = DefineIndex::new();
        idx.add_source(&TreeSitterParser, helpers_src)
            .expect("parse helpers");
        let mut det = ResourceDetector::default();
        det.ingest(
            "apiVersion: {{ template \"x.apiVersion\" . }}\nkind: Deployment\n",
            &idx,
        );
        let r = det.current().expect("detector must produce a resource");
        assert_eq!(r.kind, "Deployment");
        assert_eq!(
            r.api_version, "apps/v1",
            "helper-returned apiVersion must resolve via parse_helper_call_value + helper_literal_outputs"
        );
    }

    #[test]
    fn parse_helper_call_value_extracts_template_name() {
        let name = ResourceDetector::parse_helper_call_value(
            "apiVersion: {{ template \"x.apiVersion\" . }}",
            "apiVersion",
        );
        assert_eq!(name.as_deref(), Some("x.apiVersion"));
    }

    #[test]
    fn parse_helper_call_value_extracts_include_name() {
        let name = ResourceDetector::parse_helper_call_value(
            "apiVersion: {{ include \"grafana.hpa.apiVersion\" . }}",
            "apiVersion",
        );
        assert_eq!(name.as_deref(), Some("grafana.hpa.apiVersion"));
    }

    /// Round-7 Finding 2/3: the elasticsearch PSP template uses an
    /// inline `{{- if .Capabilities.APIVersions.Has "policy/v1" -}}` /
    /// `{{- else }}` chain around the literal `apiVersion:` line. The
    /// detector must capture both literals as typed branches so the
    /// chain layer can attribute MissingSchema to the else branch
    /// (the conservative runtime fallback) instead of emitting one
    /// diagnostic per mutually-exclusive alternative.
    #[test]
    fn inline_if_else_around_api_version_produces_typed_branches() {
        let idx = DefineIndex::new();
        let mut det = ResourceDetector::default();
        det.ingest(
            "{{- if .Capabilities.APIVersions.Has \"policy/v1\" -}}\n\
             apiVersion: policy/v1\n\
             {{- else }}\n\
             apiVersion: policy/v1beta1\n\
             {{- end }}\n\
             kind: PodSecurityPolicy\n",
            &idx,
        );
        let r = det.current().expect("detector must produce a resource");
        assert_eq!(r.kind, "PodSecurityPolicy");
        assert_eq!(
            r.api_version_branches.len(),
            2,
            "expected 2 typed branches; got {:?}",
            r.api_version_branches
        );
        // First branch: CapabilityHas guard + the then-branch literal.
        assert_eq!(
            r.api_version_branches[0].guard,
            Some(crate::CapabilityGuard::Has {
                api: "policy/v1".to_string()
            }),
        );
        assert_eq!(
            r.api_version_branches[0].body,
            crate::HelperBranchBody::Literals {
                values: vec!["policy/v1".to_string()]
            }
        );
        // Second branch: unguarded else, the fallback literal.
        assert_eq!(r.api_version_branches[1].guard, None);
        assert_eq!(
            r.api_version_branches[1].body,
            crate::HelperBranchBody::Literals {
                values: vec!["policy/v1beta1".to_string()]
            }
        );
        // The flat candidate list still carries both, so chain
        // iteration tries each — the typed branches only affect
        // MissingSchema attribution, not the resolution attempt.
        assert!(r.api_version_candidates.contains(&"policy/v1".to_string()));
        assert!(
            r.api_version_candidates
                .contains(&"policy/v1beta1".to_string())
        );
    }

    /// Round-7: when the wrapping `{{- if -}}` has an opaque condition
    /// (e.g. `semverCompare ".Capabilities.KubeVersion.GitVersion"`),
    /// the detector must NOT capture branches — there's no decodable
    /// guard for the chain to act on, and the user-facing semantics
    /// fall back to source-order primary plus candidate alternatives.
    #[test]
    fn inline_if_else_with_opaque_condition_does_not_produce_branches() {
        let idx = DefineIndex::new();
        let mut det = ResourceDetector::default();
        det.ingest(
            "{{- if semverCompare \"<1.14-0\" .Capabilities.KubeVersion.GitVersion -}}\n\
             apiVersion: apps/v1beta1\n\
             {{- else }}\n\
             apiVersion: apps/v1\n\
             {{- end }}\n\
             kind: StatefulSet\n",
            &idx,
        );
        let r = det.current().expect("detector must produce a resource");
        assert_eq!(r.kind, "StatefulSet");
        assert!(
            r.api_version_branches.is_empty(),
            "opaque guard must not create typed branches; got {:?}",
            r.api_version_branches
        );
        // Both literals still in flat candidates so chain iteration
        // can probe both.
        assert!(
            r.api_version_candidates
                .contains(&"apps/v1beta1".to_string())
                || r.api_version == "apps/v1beta1"
        );
        assert!(
            r.api_version_candidates.contains(&"apps/v1".to_string()) || r.api_version == "apps/v1"
        );
    }

    /// Round-7: nested `{{- if -}}` wrapping an inline branch chain
    /// must not interfere — the outer condition is opaque (Values
    /// reference) but the inner CapabilityHas-guarded chain still
    /// produces typed branches. The depth counter is what makes this
    /// work; `{{- end }}` of the outer if doesn't accidentally close
    /// the inner branch tracker.
    #[test]
    fn nested_outer_opaque_if_inner_capability_if_captures_inner_branches() {
        let idx = DefineIndex::new();
        let mut det = ResourceDetector::default();
        det.ingest(
            "{{- if .Values.podSecurityPolicy.create -}}\n\
             {{- if .Capabilities.APIVersions.Has \"policy/v1\" -}}\n\
             apiVersion: policy/v1\n\
             {{- else }}\n\
             apiVersion: policy/v1beta1\n\
             {{- end }}\n\
             kind: PodSecurityPolicy\n\
             {{- end }}\n",
            &idx,
        );
        let r = det.current().expect("detector must produce a resource");
        assert_eq!(r.kind, "PodSecurityPolicy");
        assert_eq!(
            r.api_version_branches.len(),
            2,
            "inner CapabilityHas if must produce 2 branches; got {:?}",
            r.api_version_branches
        );
    }
}
