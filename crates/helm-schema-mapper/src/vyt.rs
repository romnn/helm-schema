//! Minimal “Virtual YAML Tree” (VYT) probe.
//!
//! Goal: prove we can get stable YAML paths + sources + guards by abstractly
//! interpreting the template AST instead of emitting placeholders.

use color_eyre::eyre;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use crate::analyze::diagnostics;
use crate::sanitize::is_control_flow;
use crate::values::{parse_selector_chain, parse_selector_expression};
use helm_schema_template::parse::parse_gotmpl_document;
use tree_sitter::Node;

/// Helper to calculate the `Point` (row, col) at the end of a given string.
pub fn calculate_end_point(s: &str) -> tree_sitter::Point {
    let len = s.chars().count();
    let mut row = 0;
    let mut col = 0;
    for c in s.chars() {
        if c == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    tree_sitter::Point::new(row, col)
}

// Define index
#[derive(Default, Clone)]
pub struct DefineIndex {
    pub by_name: HashMap<String, (tree_sitter::Tree, String)>,
}

impl DefineIndex {
    pub fn insert(&mut self, name: String, tree: tree_sitter::Tree, src: String) {
        self.by_name.insert(name, (tree, src));
    }
    pub fn get(&self, name: &str) -> Option<&(tree_sitter::Tree, String)> {
        self.by_name.get(name)
    }
}

#[derive(Clone, Debug)]
enum BindingRole {
    MapKey,
    MapValue,
    Item, // for ranges over slices; not used in this test but harmless
}

#[derive(Clone, Debug)]
struct Binding {
    base: String,      // e.g. "env"
    role: BindingRole, // e.g. MapKey / MapValue
}

#[derive(Default, Clone, Debug)]
struct Bindings {
    // Each scope maps variable name ("$k") to a binding base ("env")
    scopes: Vec<HashMap<String, Binding>>,
    // What '.' is rebound to in this scope (if any)
    dot_stack: Vec<Option<Binding>>,
    // Mapping like "config" -> "node" (Values subtree path)
    alias_stack: Vec<HashMap<String, String>>,
}

impl Bindings {
    fn push_scope(&mut self, map: HashMap<String, Binding>, dot: Option<Binding>) {
        self.scopes.push(map);
        self.dot_stack.push(dot);
    }

    fn pop_scope(&mut self) {
        let _ = self.scopes.pop();
        let _ = self.dot_stack.pop();
    }

    fn lookup_var(&self, name: &str) -> Option<&Binding> {
        for m in self.scopes.iter().rev() {
            if let Some(b) = m.get(name) {
                return Some(b);
            }
        }
        None
    }

    fn dot_binding(&self) -> Option<&Binding> {
        self.dot_stack.iter().rev().find_map(|o| o.as_ref())
    }

    fn push_alias_map(&mut self, m: HashMap<String, String>) {
        self.alias_stack.push(m);
    }

    fn pop_alias_map(&mut self) {
        let _ = self.alias_stack.pop();
    }

    fn resolve_alias(&self, head: &str) -> Option<String> {
        for m in self.alias_stack.iter().rev() {
            if let Some(v) = m.get(head) {
                return Some(v.clone());
            }
        }
        None
    }
}

/// A minimal YAML path that’s easy to compare in tests.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct YPath(pub Vec<String>);

impl std::fmt::Display for YPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.join("."))
    }
}

/// What we record from interpretation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VYUse {
    pub source_expr: String, // e.g., "labels" (Values.labels)
    pub path: YPath,         // e.g., ["metadata","labels"]
    pub kind: VYKind,        // Scalar or Fragment
    pub guards: Vec<String>, // normalized guard value paths, e.g., ["enabled"]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VYKind {
    Scalar,
    Fragment,
}

/// A tiny text->shape tracker. Not a YAML parser; good enough for smoke tests.
/// It maintains a stack of containers determined from raw text lines.
#[derive(Default, Clone, Debug)]
struct Shape {
    /// Each level: (indent, container kind, pending mapping key at this level if any)
    stack: Vec<(usize, Container, Option<String>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Container {
    Mapping,
    Sequence,
}

impl Shape {
    fn current_path(&self) -> YPath {
        let mut out: Vec<String> = Vec::new();
        for (_, kind, pending_key) in &self.stack {
            match (kind, pending_key) {
                (Container::Mapping, Some(k)) => out.push(k.clone()),
                (Container::Mapping, None) => {}
                (Container::Sequence, _) => {
                    // We do not insert indices for smoke tests; parent path is sufficient.
                }
            }
        }
        YPath(out)
    }

    fn ingest(&mut self, text: &str) {
        for raw in text.split_inclusive('\n') {
            let line = raw.trim_end_matches('\n');
            let indent = line.chars().take_while(|&c| c == ' ').count();
            let after = &line[indent..];

            let trimmed = after.trim();

            // IMPORTANT: ignore blank/template lines BEFORE dedent,
            // so they don't pop the current mapping context.
            if trimmed.is_empty() || trimmed.starts_with("{{") {
                continue;
            }

            // Dedent only for meaningful YAML content lines
            while let Some((top_indent, _, _)) = self.stack.last().cloned() {
                if top_indent > indent {
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
                }
                continue;
            }

            if after.ends_with(':') || after.contains(':') {
                let key = after.split(':').next().unwrap_or("").trim().to_string();
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
                    _ => {
                        self.stack.push((indent, Container::Mapping, Some(key)));
                    }
                }
                continue;
            }

            if self.stack.is_empty() {
                self.stack.push((indent, Container::Mapping, None));
            }
        }
    }
}

/// Simple guard stack that collects `.Values.*` from if/with/range conditions.
#[derive(Default, Clone, Debug)]
struct Guards {
    active: Vec<String>,
}

impl Guards {}

impl Guards {
    fn push(&mut self, g: String) {
        self.active.push(g)
    }

    fn savepoint(&self) -> usize {
        self.active.len()
    }

    fn restore(&mut self, len: usize) {
        self.active.truncate(len)
    }

    fn snapshot(&self) -> Vec<String> {
        self.active.clone()
    }
}

// 1) Narrow control-flow detection so we don't treat pipelines/calls as control flow.
fn is_control_flow_kind(kind: &str) -> bool {
    matches!(
        kind,
        "if_action" | "with_action" | "range_action" | "else_action" | "define_action"
    )
}

fn first_values_base(expr: &str) -> Option<String> {
    for prefix in [".Values.", "$.Values.", "Values."] {
        if let Some(idx) = expr.find(prefix) {
            let rest = &expr[idx + prefix.len()..];
            // take up to the first non-ident/dot char
            let ident = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
                .collect::<String>();
            if !ident.is_empty() {
                return Some(ident);
            }
        }
    }
    None
}

fn header_of(node: Node) -> Option<Node> {
    let mut w = node.walk();
    for ch in node.children(&mut w) {
        if ch.is_named() && ch.kind() != "text" && ch.kind() != "ERROR" {
            return Some(ch);
        }
    }
    None
}

/// Minimal, synchronous interpreter.
///
/// NOTE: this prototype supports:
/// - raw text → shape ingest
/// - `.Values.*` scalars in value position
/// - fragments: `toYaml`, `fromYaml`, `tpl` (treated as fragment), `include` (left for phase 2)
/// - guards for `if`/`with`/`range` (condition recorded; no branching semantics for else in this probe)
// pub struct VYT<'a> {
pub struct VYT {
    pub source: String,
    // pub source: &'a str,
    pub uses: Vec<VYUse>,
    yaml_tree: IncrementalYamlTree,
    shape: Shape,
    guards: Guards,
    bindings: Bindings,
    defines: Option<Arc<DefineIndex>>,
}

// impl<'a> VYT<'a> {
impl VYT {
    // pub fn new(source: &'a str) -> Self {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            yaml_tree: IncrementalYamlTree::new(),
            uses: Vec::new(),
            shape: Shape::default(),
            guards: Guards::default(),
            bindings: Bindings::default(),
            defines: None,
        }
    }

    pub fn with_defines(mut self, defines: std::sync::Arc<DefineIndex>) -> Self {
        self.defines = Some(defines);
        self
    }

    pub fn run(mut self, tree: &tree_sitter::Tree) -> Vec<VYUse> {
        self.walk(tree.root_node());
        self.uses
    }

    // Add near other helpers if you want, or inline the logic below.
    fn deepest_values_path_in_header(&self, h: Node) -> Option<String> {
        let keys = self.collect_values_anywhere(h);
        keys.into_iter()
            .max_by_key(|k| (k.matches('.').count(), k.len()))
    }

    fn open_with_scope_if_any(&mut self, node: Node) -> bool {
        if node.kind() != "with_action" {
            return false;
        }

        // Find the header: first named, non-text, non-error child
        let mut w = node.walk();
        let header = node
            .children(&mut w)
            .find(|ch| ch.is_named() && ch.kind() != "text" && ch.kind() != "ERROR");

        let Some(h) = header else {
            return false;
        };

        if let Some(base) = self.deepest_values_path_in_header(h) {
            // Rebind '.' to the full path, e.g. "persistentVolumeClaims.storageClassName"
            self.bindings.push_scope(
                HashMap::new(),
                Some(Binding {
                    base,
                    role: BindingRole::MapValue,
                }),
            );
            return true;
        }

        false
    }

    fn open_range_scope_if_any(&mut self, node: Node) -> bool {
        if node.kind() != "range_action" {
            return false;
        }

        // Find the header node (first named, non-text, non-error child)
        let mut w = node.walk();
        let header = node
            .children(&mut w)
            .find(|ch| ch.is_named() && ch.kind() != "text" && ch.kind() != "ERROR");

        let Some(h) = header else {
            return false;
        };

        // Try to read the raw header text (used for the := form)
        let h_text = h
            .utf8_text(self.source.as_bytes())
            .unwrap_or_default()
            .trim()
            .to_string();

        // --- CASE A: map-style "range $k, $v := EXPR" (existing behavior) ---
        let re = regex::Regex::new(
            r#"(?x)
            ^
            (?P<vars>\$[A-Za-z_]\w*(?:\s*,\s*\$[A-Za-z_]\w*)?)
            \s*:=\s*
            (?P<expr>.+?)\s*$
        "#,
        )
        .unwrap();

        if let Some(caps) = re.captures(&h_text) {
            use std::collections::HashMap;

            let vars_raw = caps.name("vars").unwrap().as_str();
            let expr = caps.name("expr").unwrap().as_str();

            let base_full = first_values_base(expr).unwrap_or_else(|| "Values".to_string());
            let base_first = base_full
                .split('.')
                .next()
                .unwrap_or(&base_full)
                .to_string();

            let mut map = HashMap::new();

            // parse $k [,$v]
            let mut it = vars_raw.split(',').map(|s| s.trim().to_string());
            if let Some(v1) = it.next() {
                // 1-var form binds value
                map.insert(
                    v1,
                    Binding {
                        base: base_first.clone(),
                        role: BindingRole::MapValue,
                    },
                );
            }
            if let Some(v2) = it.next() {
                // two-var form: first is key, second is value
                if let Some((first, _)) = map.iter().next().map(|(k, v)| (k.clone(), v.clone())) {
                    map.insert(
                        first,
                        Binding {
                            base: base_first.clone(),
                            role: BindingRole::MapKey,
                        },
                    );
                }
                map.insert(
                    v2,
                    Binding {
                        base: base_first.clone(),
                        role: BindingRole::MapValue,
                    },
                );
            }

            // In range over map, '.' is the value
            let dot = Some(Binding {
                base: base_first,
                role: BindingRole::MapValue,
            });
            self.bindings.push_scope(map, dot);
            return true;
        }

        // --- CASE B: simple list/map range "range EXPR" (no :=) ---
        // Rebind '.' to the deepest .Values.* path; mark as Item so we can star-normalize later.
        if let Some(base_full) = self.deepest_values_path_in_header(h) {
            self.bindings.push_scope(
                HashMap::new(),
                Some(Binding {
                    base: base_full,
                    role: BindingRole::Item,
                }),
            );
            return true;
        }

        false
    }

    fn walk(&mut self, node: Node) {
        // Containers & control-flow: walk children with small pre/post hooks
        println!(
            "walk: visit node={:?} shape={:?} guards={:?}",
            node, self.shape, self.guards
        );

        if is_control_flow_kind(node.kind()) {
            let sp = self.guards.savepoint();
            self.collect_guards_from(node);

            let opened_range_scope = self.open_range_scope_if_any(node);
            let opened_with_scope = self.open_with_scope_if_any(node);

            let header = header_of(node).map(|h| h.id()); // <- remember header id

            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if !ch.is_named() {
                    continue;
                }
                // Skip the header subtree for output recording
                if Some(ch.id()) == header {
                    continue;
                }
                self.walk(ch);
            }

            if opened_with_scope {
                self.bindings.pop_scope();
            }
            if opened_range_scope {
                self.bindings.pop_scope();
            }

            self.guards.restore(sp);
            return;
        }

        match node.kind() {
            "text" => {
                let txt = &self.source[node.byte_range()];
                self.shape.ingest(txt);
                self.yaml_tree.update_partial_yaml(txt);
            }
            "function_call" => {
                // Special-case include
                if function_name_of(&node, &self.source).as_deref() == Some("include") {
                    if self.inline_include(node) {
                        return;
                    }
                }
                // regular handling
                self.handle_output(node);
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() {
                        self.walk(ch);
                    }
                }
            }

            "selector_expression" | "chained_pipeline" => {
                self.handle_output(node);
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() {
                        self.walk(ch);
                    }
                }
            }
            // NEW: variables and '.' (dot) must be recorded as scalar outputs
            "variable" | "dot" => {
                self.handle_output(node);
                // (they have no meaningful named children, so no need to descend)
            }
            _ => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() {
                        self.walk(ch);
                    }
                }
            }
        }
    }

    fn inline_include(&mut self, call: Node) -> bool {
        let Some(defs) = self.defines.as_ref().map(Arc::clone) else {
            return false;
        };

        // Find args: name (string) and ctx (anything)
        let mut name: Option<String> = None;
        let mut ctx: Option<Node> = None;

        // args are under argument_list
        for i in 0..call.child_count() {
            if let Some(ch) = call.child(i) {
                if ch.is_named() && ch.kind() == "argument_list" {
                    let mut w = ch.walk();
                    let args: Vec<Node> = ch.children(&mut w).filter(|n| n.is_named()).collect();
                    // expect: ["x", ctx]  (ignore extra)
                    for (idx, a) in args.iter().enumerate() {
                        let k = a.kind();
                        if k.contains("string")
                            || k == "raw_string_literal"
                            || k == "string_literal"
                        {
                            if name.is_none() {
                                if let Ok(t) = a.utf8_text(self.source.as_bytes()) {
                                    let n =
                                        t.trim().trim_matches('"').trim_matches('`').to_string();
                                    name = Some(n);
                                    if idx + 1 < args.len() {
                                        ctx = Some(args[idx + 1]);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let Some(tpl_name) = name else {
            return false;
        };
        let Some((tpl_tree, tpl_src)) = defs.get(&tpl_name) else {
            return false;
        };

        // Build alias map / dot binding from ctx
        let mut alias_map: HashMap<String, String> = HashMap::new();
        let mut dot_binding: Option<Binding> = None;
        if let Some(ctx_node) = ctx {
            self.extract_include_context(ctx_node, &mut alias_map, &mut dot_binding);
        }

        // Push scopes
        let had_alias = !alias_map.is_empty();
        if had_alias {
            self.bindings.push_alias_map(alias_map);
        }
        let pushed_dot = if let Some(b) = dot_binding {
            self.bindings.push_scope(HashMap::new(), Some(b));
            true
        } else {
            false
        };

        // Swap source while walking the included template
        let saved_src = self.source.to_owned();
        self.source = tpl_src.clone();

        // Walk included template with current shape and guards
        self.walk(tpl_tree.root_node());

        // Restore source and scopes
        self.source = saved_src;
        if pushed_dot {
            self.bindings.pop_scope();
        }
        if had_alias {
            self.bindings.pop_alias_map();
        }
        true
    }

    fn extract_include_context(
        &self,
        n: Node,
        alias_out: &mut HashMap<String, String>,
        dot_out: &mut Option<Binding>,
    ) {
        // Unwrap (dict ...) written as a parenthesized_pipeline
        let node_to_check = if n.kind() == "parenthesized_pipeline" {
            let mut w = n.walk();
            n.children(&mut w)
                .find(|c| c.is_named() && c.kind() == "function_call")
                .unwrap_or(n)
        } else {
            n
        };

        if function_name_of(&node_to_check, &self.source).as_deref() == Some("dict") {
            // Parse ("key" value) pairs
            if let Some(args) = node_to_check.child_by_field_name("arguments").or_else(|| {
                // fallback: find the argument_list child
                let mut w = node_to_check.walk();
                node_to_check
                    .children(&mut w)
                    .find(|c| c.is_named() && c.kind() == "argument_list")
            }) {
                let mut w = args.walk();
                let elems: Vec<Node> = args.children(&mut w).filter(|x| x.is_named()).collect();
                let mut i = 0;
                while i + 1 < elems.len() {
                    let key_node = elems[i];
                    let val_node = elems[i + 1];
                    let key_txt = key_node
                        .utf8_text(self.source.as_bytes())
                        .unwrap_or("")
                        .trim()
                        .trim_matches('"')
                        .trim_matches('`')
                        .to_string();

                    if !key_txt.is_empty() {
                        if let Some(p) = self.value_path_from_expr(val_node) {
                            // Only record aliases that resolve to .Values.* subpaths
                            alias_out.insert(key_txt, p);
                        }
                    }
                    i += 2;
                }
            }
            return;
        }

        // Not a dict: try to bind '.' to a concrete .Values.* base
        if let Some(p) = self.value_path_from_expr(n) {
            *dot_out = Some(Binding {
                base: p,
                role: BindingRole::Item,
            });
        }
    }

    /// Try to reduce an expression node to a `.Values.*` subpath String.
    fn value_path_from_expr(&self, n: Node) -> Option<String> {
        match n.kind() {
            "selector_expression" => {
                if let Some(segs) = parse_selector_chain(n, &self.source) {
                    let ss: Vec<_> = segs.iter().map(|s| s.as_str()).collect();
                    match ss.as_slice() {
                        [".", "Values", rest @ ..] | ["$", "Values", rest @ ..]
                            if !rest.is_empty() =>
                        {
                            return Some(rest.join("."));
                        }
                        ["Values", rest @ ..] if !rest.is_empty() => {
                            return Some(rest.join("."));
                        }
                        [".", head, rest @ ..] if !head.is_empty() => {
                            if let Some(base) = self.bindings.resolve_alias(head) {
                                if rest.is_empty() {
                                    return Some(base);
                                } else {
                                    return Some(format!("{}.{}", base, rest.join(".")));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                None
            }
            "dot" => self.bindings.dot_binding().map(|b| b.base.clone()),
            "variable" | "identifier" => {
                if let Ok(txt) = n.utf8_text(self.source.as_bytes()) {
                    let t = txt.trim();
                    if t.starts_with('$') {
                        if let Some(b) = self.bindings.lookup_var(t) {
                            return Some(b.base.clone());
                        }
                    }
                }
                None
            }
            "parenthesized_pipeline" => {
                let mut w = n.walk();
                for ch in n.children(&mut w) {
                    if ch.is_named() {
                        if let Some(p) = self.value_path_from_expr(ch) {
                            return Some(p);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Like the old free fn, but aware of alias bindings.
    fn collect_values_anywhere(&self, node: Node) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if n.kind() == "selector_expression" {
                if let Some(segs) = parse_selector_chain(n, &self.source) {
                    let ss: Vec<_> = segs.iter().map(|s| s.as_str()).collect();
                    // .Values.*
                    match ss.as_slice() {
                        [".", "Values", rest @ ..] | ["$", "Values", rest @ ..]
                            if !rest.is_empty() =>
                        {
                            out.insert(rest.join("."));
                        }
                        ["Values", rest @ ..] if !rest.is_empty() => {
                            out.insert(rest.join("."));
                        }
                        // .config.foo → alias "config" → node
                        [".", head, rest @ ..] if !rest.is_empty() => {
                            if let Some(base) = self.bindings.resolve_alias(head) {
                                out.insert(format!("{}.{}", base, rest.join(".")));
                            }
                        }
                        // .config (no tail)
                        [".", head] => {
                            if let Some(base) = self.bindings.resolve_alias(head) {
                                out.insert(base);
                            }
                        }
                        _ => {}
                    }
                }
            }
            let mut w = n.walk();
            for ch in n.children(&mut w) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }
        out
    }

    fn collect_guards_from(&mut self, node: Node) {
        println!(
            "collecting guards from {:?} [ {} ]",
            node,
            node.utf8_text(self.source.as_bytes()).unwrap_or_default(),
        );
        match node.kind() {
            "if_action" | "with_action" | "range_action" => {
                let mut w = node.walk();
                let header = node
                    .children(&mut w)
                    .find(|ch| ch.is_named() && ch.kind() != "text" && ch.kind() != "ERROR");

                if let Some(h) = header {
                    let keys = self.collect_values_anywhere(h);
                    for k in keys {
                        if !k.is_empty() {
                            // push onto active guards
                            self.guards.push(k.clone());
                            // ALSO: record a stand-alone guard use at the current YAML path
                            self.uses.push(VYUse {
                                source_expr: k,
                                path: self.shape.current_path(),
                                kind: VYKind::Scalar, // role-less model; scalar is fine for presence
                                guards: vec![],
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_output(&mut self, node: Node) {
        println!(
            "handle output: node={:?} [ {} ]",
            node,
            node.utf8_text(self.source.as_bytes()).unwrap_or_default(),
        );

        // if node.is_named() && (node.kind() == "identifier" || node.kind() == "variable") {

        // Is this a variable or a dot?
        if node.is_named() && (node.kind() == "variable" || node.kind() == "dot") {
            if let Ok(txt) = node.utf8_text(self.source.as_bytes()) {
                let t = txt.trim();
                let mut base: Option<String> = None;

                // if t == "." {
                if node.kind() == "dot" || t == "." {
                    if let Some(b) = self.bindings.dot_binding() {
                        // base = Some(b.base.clone());

                        // Start with the bound base
                        let mut src = b.base.clone();

                        // If this dot comes from a list range, and we're currently emitting
                        // under a YAML sequence,
                        // normalize to a wildcard element: "pvc.accessModes.*"
                        if matches!(b.role, BindingRole::Item) {
                            let in_sequence = self
                                .shape
                                .stack
                                .iter()
                                .any(|(_, kind, _)| *kind == Container::Sequence);
                            if in_sequence && !src.ends_with(".*") {
                                src.push_str(".*");
                            }
                        }

                        base = Some(src);
                    }
                } else if t.starts_with('$') {
                    if let Some(b) = self.bindings.lookup_var(t) {
                        base = Some(b.base.clone());
                    }
                }

                if let Some(b) = base {
                    let path = self.shape.current_path();
                    let guards = self.guards.snapshot();
                    self.uses.push(VYUse {
                        source_expr: b,
                        path,
                        kind: VYKind::Scalar,
                        guards,
                    });
                    // Do not traverse further; variables are leaves
                    return;
                }
            }
        }

        // Existing producer / selector handling as before
        let is_fragment = pipeline_contains_any(node, &self.source, &["toYaml", "fromYaml", "tpl"]);
        let sources = self.collect_values_anywhere(node);
        // let sources = collect_values_anywhere(node, self.source);
        if sources.is_empty() {
            return;
        }
        let path = self.shape.current_path();
        let kind = if is_fragment {
            VYKind::Fragment
        } else {
            VYKind::Scalar
        };
        let guards = self.guards.snapshot();

        for s in sources {
            self.uses.push(VYUse {
                source_expr: s,
                path: path.clone(),
                kind,
                guards: guards.clone(),
            });
        }
    }
}

// ---- small local helpers (robust variants reusing your parsers where possible) ----

fn first_text_start(n: Node) -> Option<usize> {
    let mut w = n.walk();
    for ch in n.children(&mut w) {
        if ch.is_named() && ch.kind() == "text" {
            return Some(ch.start_byte());
        }
    }
    None
}

fn function_name_of(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "function_call" {
        return None;
    }
    if let Some(f) = node.child_by_field_name("function") {
        if let Ok(s) = f.utf8_text(src.as_bytes()) {
            return Some(s.trim().to_string());
        }
    }
    for i in 0..node.child_count() {
        if let Some(ch) = node.child(i) {
            if ch.is_named() && ch.kind() == "identifier" {
                if let Ok(s) = ch.utf8_text(src.as_bytes()) {
                    return Some(s.trim().to_string());
                }
            }
        }
    }
    None
}

fn pipeline_contains_any(node: Node, src: &str, wanted: &[&str]) -> bool {
    fn has_name(call: &Node, src: &str, wanted: &[&str]) -> bool {
        if let Some(name) = function_name_of(call, src) {
            return wanted.iter().any(|w| *w == name);
        }
        false
    }
    match node.kind() {
        "function_call" => has_name(&node, src, wanted),
        "chained_pipeline" => {
            let mut w = node.walk();
            for ch in node.children(&mut w) {
                if ch.is_named() && ch.kind() == "function_call" && has_name(&ch, src, wanted) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

pub struct IncrementalYamlTree {
    parser: tree_sitter::Parser,
    tree: Option<tree_sitter::Tree>,
    source_text: String,
}

impl IncrementalYamlTree {
    pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter::Language::new(helm_schema_template_grammar::yaml::language());
        parser.set_language(&language).unwrap();

        Self {
            parser,
            tree: None,
            source_text: "".to_string(),
        }
    }

    pub fn update_partial_yaml(&mut self, chunk: &str) -> eyre::Result<()> {
        let old_source_len_bytes = self.source_text.len();
        let old_end_point = calculate_end_point(&self.source_text);

        self.source_text.push_str(&chunk);
        if true {
            println!("=== PARTIAL YAML === ");
            println!("{}", &self.source_text);
            println!("=================== ");
        }

        let new_end_point = calculate_end_point(&self.source_text);
        if let Some(tree) = self.tree.as_mut() {
            let edit = tree_sitter::InputEdit {
                start_byte: old_source_len_bytes,
                old_end_byte: old_source_len_bytes,
                new_end_byte: self.source_text.len(),
                start_position: old_end_point,
                old_end_position: old_end_point,
                new_end_position: new_end_point,
            };
            tree.edit(&edit);
        }

        self.tree = self
            .parser
            .parse(self.source_text.as_bytes(), self.tree.as_ref());

        if let Some(tree) = &self.tree {
            if true {
                let ast = helm_schema_template::fmt::SExpr::parse_tree(
                    &tree.root_node(),
                    &self.source_text,
                );
                println!("\n{}\n", ast.to_string_pretty());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{DefineIndex, VYKind, VYT, VYUse, YPath};
    use helm_schema_template::parse::parse_gotmpl_document;
    use indoc::indoc;
    use test_util::prelude::*;

    fn run_vyt(src: &str) -> Vec<VYUse> {
        let parsed = parse_gotmpl_document(src).expect("parse");
        let vyt = VYT::new(src);
        vyt.run(&parsed.tree)
    }

    fn has_use(uses: &[VYUse], kind: VYKind, src: &str, path: &[&str]) -> bool {
        uses.iter().any(|u| {
            u.kind == kind
                && u.source_expr == src
                && u.path == YPath(path.iter().map(|s| s.to_string()).collect())
        })
    }

    fn has_scalar(uses: &[VYUse], src: &str, path: &[&str]) -> bool {
        has_use(uses, VYKind::Scalar, src, path)
    }

    fn has_fragment(uses: &[VYUse], src: &str, path: &[&str]) -> bool {
        has_use(uses, VYKind::Fragment, src, path)
    }

    fn guards_of<'a>(uses: &'a [VYUse], src: &str) -> impl Iterator<Item = &'a String> {
        uses.iter()
            .find(|u| u.source_expr == src)
            .into_iter()
            .flat_map(|u| u.guards.iter())
    }

    fn parse_define(src: &str) -> (tree_sitter::Tree, String) {
        let parsed = parse_gotmpl_document(src).expect("parse");
        (parsed.tree, src.to_string())
    }

    fn make_define_index(defs: &[(&str, &str)]) -> std::sync::Arc<DefineIndex> {
        let mut idx = DefineIndex::default();
        for (name, body) in defs {
            let (t, s) = parse_define(body);
            idx.insert((*name).to_string(), t, s);
        }
        std::sync::Arc::new(idx)
    }

    fn run_vyt_with_defs(src: &str, defs: std::sync::Arc<DefineIndex>) -> Vec<VYUse> {
        let parsed = parse_gotmpl_document(src).expect("parse");
        let vyt = VYT::new(src).with_defines(defs);
        vyt.run(&parsed.tree)
    }

    #[test]
    fn to_yaml_fragment_under_mapping() {
        Builder::default().build();

        let tpl = indoc! {r#"
            metadata:
              labels:
                {{- toYaml .Values.labels | nindent 4 }}
        "#};
        let uses = run_vyt(tpl);
        dbg!(&uses);
        // Expect one fragment use with path "metadata.labels" and source "labels"
        assert!(
            uses.iter().any(|u| u.kind == VYKind::Fragment
                && u.source_expr == "labels"
                && u.path == YPath(vec!["metadata".into(), "labels".into()])),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn from_yaml_under_key() {
        Builder::default().build();

        let tpl = indoc! {r#"
            ingress: {{ fromYaml .Values.ingress }}
        "#};
        let uses = run_vyt(tpl);
        dbg!(&uses);
        assert!(
            uses.iter().any(|u| u.kind == VYKind::Fragment
                && u.source_expr == "ingress"
                && u.path == YPath(vec!["ingress".into()])),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn scalar_in_value_position_and_guard() {
        Builder::default().build();

        // 'if' guard should be attached to the recorded scalar
        let tpl = indoc! {r#"
            metadata:
              name: {{ if .Values.enabled }}{{ .Values.name }}{{ end }}
        "#};
        let uses = run_vyt(tpl);
        dbg!(&uses);
        assert!(
            uses.iter().any(|u| u.kind == VYKind::Scalar
                && u.source_expr == "name"
                && u.path == YPath(vec!["metadata".into(), "name".into()])
                && u.guards.contains(&"enabled".to_string())),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn quote_keeps_scalar_mapping_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            spec:
              image: {{ quote .Values.image }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            has_scalar(&uses, "image", &["spec", "image"]),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn upper_and_b64enc_do_not_muddle_types_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            metadata:
              name: {{ upper .Values.name }}
            data:
              pass: {{ b64enc .Values.password }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            has_scalar(&uses, "name", &["metadata", "name"]),
            "{:#?}",
            uses
        );
        assert!(
            has_scalar(&uses, "password", &["data", "pass"]),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn chained_functions_preserve_value_origin_vyt() {
        Builder::default().build();

        // default "nginx" (.Values.image | trim | lower) → still scalar from "image"
        let tpl = indoc! {r#"
            spec:
              image: {{ default "nginx" (.Values.image | trim | lower) }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            has_scalar(&uses, "image", &["spec", "image"]),
            "{:#?}",
            uses
        );
    }

    // ---------- defaults / coalesce / ternary / required ----------

    #[test]
    fn default_propagates_value_path_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            spec:
              imagePullPolicy: {{ default "IfNotPresent" .Values.pullPolicy }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            has_scalar(&uses, "pullPolicy", &["spec", "imagePullPolicy"]),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn coalesce_registers_all_candidate_paths_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            data:
              username: {{ coalesce .Values.primaryUser .Values.fallbackUser "guest" }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            has_scalar(&uses, "primaryUser", &["data", "username"]),
            "{:#?}",
            uses
        );
        assert!(
            has_scalar(&uses, "fallbackUser", &["data", "username"]),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn ternary_condition_is_recorded_as_scalar_source_vyt() {
        Builder::default().build();

        // In VYT probe, we record the condition's .Values.* as the source feeding the slot.
        let tpl = indoc! {r#"
            spec:
              profile: {{ ternary "prod" "dev" .Values.production }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            has_scalar(&uses, "production", &["spec", "profile"]),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn required_emits_scalar_value_use_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            spec:
              host: {{ required "hostname is required" .Values.host }}
        "#};
        let uses = run_vyt(tpl);
        assert!(has_scalar(&uses, "host", &["spec", "host"]), "{:#?}", uses);
    }

    #[test]
    fn if_required_in_header_becomes_guard_for_body_vyt() {
        Builder::default().build();

        // Guard from header should show up in the body use
        let tpl = indoc! {r#"
            {{- if required "must set enabled" .Values.enabled }}
            flag: {{ .Values.payload }}
            {{- end }}
        "#};
        let uses = run_vyt(tpl);
        // body scalar we can see:
        assert!(has_scalar(&uses, "payload", &["flag"]), "{:#?}", uses);
        // and the recorded guards should include "enabled"
        let g: Vec<_> = guards_of(&uses, "payload").cloned().collect();
        assert!(g.iter().any(|s| s == "enabled"), "{:#?}", g);
    }

    // ---------- fragments / YAML placement ----------

    #[test]
    fn to_yaml_emits_fragment_on_mapping_slot_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            metadata:
              labels:
                {{- toYaml .Values.labels | nindent 4 }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            has_fragment(&uses, "labels", &["metadata", "labels"]),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn from_yaml_bridges_value_to_yaml_path_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            ingress: {{ fromYaml .Values.ingress }}
        "#};
        let uses = run_vyt(tpl);
        assert!(has_fragment(&uses, "ingress", &["ingress"]), "{:#?}", uses);
    }

    #[test]
    fn from_yaml_then_access_only_tracks_source_vyt() {
        Builder::default().build();

        // We at least record that .Values.rawYaml was consumed.
        let tpl = indoc! {r#"
            {{- $o := fromYaml .Values.rawYaml }}
            result: {{ default "x" $o.someField }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            uses.iter().any(|u| u.source_expr == "rawYaml"),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn tpl_in_scalar_and_fragment_positions_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            command: {{ tpl .Values.commandTpl . }}
            metadata:
              annotations:
                {{- tpl .Values.annTpl . | nindent 4 }}
        "#};
        let uses = run_vyt(tpl);
        assert!(has_scalar(&uses, "commandTpl", &["command"]), "{:#?}", uses);
        assert!(
            has_fragment(&uses, "annTpl", &["metadata", "annotations"]),
            "{:#?}",
            uses
        );
    }

    // ---------- guards harvested from composite boolean expr ----------

    #[test]
    fn and_or_chains_in_if_become_guards_on_body_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            {{- if and .Values.enable (or .Values.a .Values.b) }}
            out: {{ .Values.payload }}
            {{- end }}
        "#};
        let uses = run_vyt(tpl);
        dbg!(&uses);

        // Find the payload use and assert everything about it
        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("payload"),
                path: eq(&YPath(vec!["out".into()])),
                kind: eq(&VYKind::Scalar),
                // guards can come in any order; must include "enable" and one of {"a","b"}
                guards: predicate(|gs: &Vec<String>| {
                    let has_enable = gs.iter().any(|g| g == "enable");
                    let has_a_or_b = gs.iter().any(|g| g == "a") || gs.iter().any(|g| g == "b");
                    has_enable && has_a_or_b
                }),
            }))
        );
    }

    // ---------- guarded-with + dot bodies (limitation documented) ----------

    #[test]
    fn with_scalar_dot_body_would_need_dot_rebinding() {
        Builder::default().build();
        // Original intent: .Values.persistentVolumeClaims.storageClassName → spec.storageClassName
        let tpl = indoc! {r#"
            spec:
              {{- with .Values.persistentVolumeClaims.storageClassName }}
              storageClassName: {{ . }}
              {{- end }}
        "#};
        let uses = run_vyt(tpl);
        dbg!(&uses);

        // The '.' inside the with-body must be attributed to the header source
        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("persistentVolumeClaims.storageClassName"),
                path: eq(&YPath(vec!["spec".into(), "storageClassName".into()])),
                kind: eq(&VYKind::Scalar),
                .. // guards not asserted here (they'll naturally include the with-condition)
            }))
        );
    }

    // ---------- map iteration / index family (partial) ----------

    #[test]
    fn index_with_variable_key_at_least_records_base_vyt() {
        Builder::default().build();
        // We don’t expand dynamic keys yet; ensure the base is recorded
        let tpl = indoc! {r#"
            {{- $k := "dynamic" }}
            data:
              chosen: {{ index .Values.extra $k }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            has_scalar(&uses, "extra", &["data", "chosen"]),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn get_function_with_default_like_pattern_records_base_vyt() {
        Builder::default().build();
        // default "Always" (get .Values.image "pullPolicy") → we at least see `image`
        let tpl = indoc! {r#"
            imagePullPolicy: {{ default "Always" (get .Values.image "pullPolicy") }}
        "#};
        let uses = run_vyt(tpl);
        assert!(
            has_scalar(&uses, "image", &["imagePullPolicy"]),
            "{:#?}",
            uses
        );
    }

    #[test]
    fn haskey_if_adds_container_guard_and_body_scalar_vyt() {
        Builder::default().build();
        // Guard should include `labels`; body uses `labels.app`
        let tpl = indoc! {r#"
            {{- if hasKey .Values.labels "app" }}
            app.kubernetes.io/name: {{ .Values.labels.app }}
            {{- end }}
        "#};
        let uses = run_vyt(tpl);
        // Recorded scalar:
        assert!(
            has_scalar(&uses, "labels.app", &["app.kubernetes.io/name"]),
            "{:#?}",
            uses
        );
        // Guard captured on the body scalar:
        let g: Vec<_> = guards_of(&uses, "labels.app").cloned().collect();
        assert!(g.iter().any(|s| s == "labels"), "{:#?}", g);
    }

    // ---------- things we intentionally park for later ----------

    #[test]
    fn include_chain_and_bindings_need_define_index_vyt() {
        Builder::default().build();
        let tpl = r#"{{ include "x" (dict "ctx" $ "config" .Values.node) }}"#;
        let uses = run_vyt(tpl);

        // We don't have a YAML placement here, so the path is [].
        // `include` isn't flagged as a fragment in the probe; we record a scalar-use of .Values.node.
        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("node"),
                path: eq(&YPath(vec![])),
                kind: eq(&VYKind::Scalar),
                .. // guards not asserted for this case
            }))
        );
    }

    #[test]
    fn range_over_map_key_value_pairs_needs_bindings_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            env:
              {{- range $k, $v := .Values.env }}
              - name: {{ $k }}
                value: {{ $v }}
              {{- end }}
        "#};
        let uses = run_vyt(tpl);

        // $k should be attributed to the .Values.env source under path env.name
        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("env"),
                path: eq(&YPath(vec!["env".into(), "name".into()])),
                kind: eq(&VYKind::Scalar),
                .. // guards not asserted
            }))
        );

        // $v should be attributed to the .Values.env source under path env.value
        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("env"),
                path: eq(&YPath(vec!["env".into(), "value".into()])),
                kind: eq(&VYKind::Scalar),
                .. // guards not asserted
            }))
        );
    }

    #[test]
    fn list_index_or_star_normalization_future_vyt() {
        Builder::default().build();

        let tpl = indoc! {r#"
            spec:
              accessModes:
                {{- range .Values.pvc.accessModes }}
                - {{ . }}
                {{- end }}
        "#};
        let uses = run_vyt(tpl);

        // We expect the source to be normalized to "pvc.accessModes.*"
        // and placed under the YAML path ["spec","accessModes"] (sequence items).
        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("pvc.accessModes.*"),
                path: eq(&YPath(vec!["spec".into(), "accessModes".into()])),
                kind: eq(&VYKind::Scalar),
                .. // guards not asserted
            }))
        );
    }

    #[test]
    fn dollar_root_misc_should_not_crash_and_is_ignored_vyt() {
        Builder::default().build();
        let tpl = r#"{{- if ($.Chart).AppVersion }}X{{- end }}"#;
        let uses = run_vyt(tpl);
        assert!(uses.is_empty(), "{:#?}", uses);
    }

    #[test]
    fn nested_include_passes_config_through_parent() {
        Builder::default().build();

        // child reads `.config.child`
        let child = indoc! {r#"
            {{- define "child" -}}
            child: {{ .config.child }}
            {{- end -}}
        "#};

        // parent passes `.Values.parent` as config
        let parent = indoc! {r#"
            {{- define "parent" -}}
            {{ include "child" (dict "config" .Values.parent) }}
            {{- end -}}
        "#};

        let defs = make_define_index(&[("child", child), ("parent", parent)]);

        let src = r#"{{ include "parent" . }}"#;
        let uses = run_vyt_with_defs(src, defs);

        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("parent.child"),
                path: eq(&YPath(vec!["child".into()])),
                kind: eq(&VYKind::Scalar),
                ..
            }))
        );
    }

    #[test]
    fn include_with_ctx_and_local_overrides_dot() {
        Builder::default().build();

        // helper expects `.ctx` to be the chart root and `.config` to a subtree
        let helper = indoc! {r#"
            {{- define "helper" -}}
            name: {{ include "fullname" .ctx }}
            leaf: {{ .config.leaf }}
            {{- end -}}
        "#};

        let fullname = indoc! {r#"
            {{- define "fullname" -}}
            {{- if .Values.fullnameOverride -}}
            {{- .Values.fullnameOverride -}}
            {{- else -}}
            {{- printf "%s-%s" .Release.Name .Chart.Name -}}
            {{- end -}}
            {{- end -}}
        "#};

        let defs = make_define_index(&[("helper", helper), ("fullname", fullname)]);

        let src = r#"{{ include "helper" (dict "ctx" $ "config" .Values.node) }}"#;
        let uses = run_vyt_with_defs(src, defs);

        // We only assert .Values.* surfaces:
        // 1) Guard appearance for fullnameOverride (standalone guard use)
        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("fullnameOverride"),
                .. // path/guards not asserted
            }))
        );

        // 2) The leaf value sourced from node.leaf
        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("node.leaf"),
                path: eq(&YPath(vec!["leaf".into()])),
                kind: eq(&VYKind::Scalar),
                ..
            }))
        );
    }

    #[test]
    fn include_chain_three_levels_deep_carries_bindings() {
        Builder::default().build();

        let c = indoc! {r#"
            {{- define "C" -}}
            field: {{ .config.final }}
            {{- end -}}
        "#};
        let b = indoc! {r#"
            {{- define "B" -}}
            {{ include "C" (dict "config" .config.inner) }}
            {{- end -}}
        "#};
        let a = indoc! {r#"
            {{- define "A" -}}
            {{ include "B" (dict "config" .Values.A) }}
            {{- end -}}
        "#};

        let defs = make_define_index(&[("A", a), ("B", b), ("C", c)]);

        let src = r#"{{ include "A" . }}"#;
        let uses = run_vyt_with_defs(src, defs);

        assert_that!(
            &uses,
            contains(matches_pattern!(VYUse {
                source_expr: eq("A.inner.final"),
                path: eq(&YPath(vec!["field".into()])),
                kind: eq(&VYKind::Scalar),
                ..
            }))
        );
    }
}
