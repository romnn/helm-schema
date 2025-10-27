use helm_schema_template::{
    parse::parse_gotmpl_document,
    values::{ValuePath as TmplValuePath, extract_values_paths},
};
use owo_colors::OwoColorize;
use std::collections::{BTreeMap, BTreeSet};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
use thiserror::Error;
use tree_sitter::Node;
use vfs::VfsPath;

use crate::yaml_path::{YamlPath, compute_yaml_paths_for_placeholders};
use crate::{
    analyze,
    sanitize::{Placeholder, Slot, build_sanitized_with_placeholders},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// This template action appears at a mapping/scalar "value position" (after a colon),
    /// so we can place a scalar placeholder and map to a YAML path.
    ScalarValue,
    /// Appears before the colon in a key position (rare). Placeholder not inserted (for now).
    MappingKey,
    /// Control flow or renders YAML fragments (e.g. include/toYaml) — we don't insert a scalar placeholder.
    Fragment,
    /// Used only in `if/with/range` guard; no scalar emission.
    Guard,
    /// Unknown/ambiguous – no placeholder.
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ValueUse {
    pub value_path: String, // e.g., "ingress.pathType"
    pub role: Role,
    pub action_span: Range<usize>, // byte range in original source for the owning template action
    pub yaml_path: Option<YamlPath>, // present if role == ScalarValue and we found a placeholder target
}

#[derive(Clone, Debug)]
pub struct Occurrence {
    pub role: Role,
    pub path: Option<YamlPath>,
    pub action_span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalBinding {
    pub role: Role,
    pub path: Option<String>,
    /// How many total occurrences we observed for this .Values key
    pub sources: usize,
}

pub fn group_uses(uses: &[ValueUse]) -> BTreeMap<String, Vec<Occurrence>> {
    let mut m: BTreeMap<String, Vec<Occurrence>> = BTreeMap::new();
    for u in uses {
        let key = u.value_path.trim();
        if key.is_empty() {
            // Defensive: if anything upstream produced an empty key, drop it here
            eprintln!(
                "[group_uses] Skipping empty value_path at bytes {:?}.",
                u.action_span
            );
            continue;
        }
        m.entry(key.to_string()).or_default().push(Occurrence {
            role: u.role,
            path: u.yaml_path.clone(),
            action_span: u.action_span.clone(),
        });
    }
    m
}

fn role_rank(r: Role) -> u8 {
    match r {
        Role::ScalarValue => 4,
        Role::Fragment => 3,
        Role::Guard => 2,
        Role::Unknown => 1,
        Role::MappingKey => 0,
    }
}

pub fn canonicalize_group(group: &[Occurrence]) -> CanonicalBinding {
    // Sort by (role_rank desc, has_path desc, action_start asc)
    let mut best = None::<&Occurrence>;
    for o in group {
        match best {
            None => best = Some(o),
            Some(b) => {
                let lhs = (role_rank(o.role), o.path.is_some(), o.action_span.start);
                let rhs = (role_rank(b.role), b.path.is_some(), b.action_span.start);
                // role desc, has_path desc, start asc
                let ord = lhs
                    .0
                    .cmp(&rhs.0)
                    .then(lhs.1.cmp(&rhs.1))
                    .then(rhs.2.cmp(&lhs.2)); // invert to prefer smaller start
                if ord.is_gt() {
                    best = Some(o);
                }
            }
        }
    }
    // let chosen = best.ok_or_eyre("missing non-empty group")?;
    let chosen = best.unwrap();
    CanonicalBinding {
        role: chosen.role,
        path: chosen.path.as_ref().map(|p| p.to_string()),
        sources: group.len(),
    }
}

pub fn canonicalize_uses(uses: &[ValueUse]) -> BTreeMap<String, CanonicalBinding> {
    let grouped = group_uses(uses);
    grouped
        .into_iter()
        .map(|(k, v)| (k, canonicalize_group(&v)))
        .collect()
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("VFS: {0}")]
    Vfs(#[from] vfs::VfsError),
    #[error(transparent)]
    Include(#[from] IncludeError),
    #[error("failed to parse template")]
    ParseTemplate,
    #[error("Parse YAML failed")]
    YamlParse,
}

/// Heuristic: classify a template action by looking at the original source line content
fn classify_action(src: &str, action_span: &Range<usize>) -> Role {
    // Get line slice
    let start = action_span.start;
    let line_start = src[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = src[start..]
        .find('\n')
        .map(|i| start + i)
        .unwrap_or(src.len());
    let line = &src[line_start..line_end];

    // position of action within the line
    let rel = start - line_start;

    // If there's a ':' before action and no ':' after -> value position
    if let Some(_) = line[..rel].rfind(':') {
        if line[rel..].find(':').is_none() {
            return Role::ScalarValue;
        }
    }

    // If action appears before ':' on the same line -> key position
    if let Some(_) = line[rel..].find(':') {
        return Role::MappingKey;
    }

    // Control flow keywords at start of action often not scalar
    let snippet = &src[action_span.clone()];
    let is_ctrl = ["if ", "with ", "range ", "end", "else"]
        .iter()
        .any(|kw| snippet.contains(kw));
    if is_ctrl {
        return Role::Guard;
    }

    Role::Unknown
}

fn root_children_in_order<'tree>(tree: &'tree tree_sitter::Tree) -> Vec<Node<'tree>> {
    let root = tree.root_node();
    let mut c = root.walk();
    root.children(&mut c).collect()
}

/// Find the smallest *top-level* template child that contains a node range.
/// Top-level means immediate child of the root `template` node that is not a `text` node.
fn owning_action_child<'tree>(
    tree: &'tree tree_sitter::Tree,
    byte_range: Range<usize>,
) -> Option<Node<'tree>> {
    for ch in root_children_in_order(tree) {
        if ch.kind() == "text" {
            continue;
        }
        let r = ch.byte_range();
        if r.start <= byte_range.start && byte_range.end <= r.end {
            return Some(ch);
        }
    }
    None
}

fn is_ctrl_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_action" | "with_action" | "range_action" | "else_clause"
    )
}

fn first_text_start(n: &tree_sitter::Node) -> Option<usize> {
    let mut w = n.walk();
    for ch in n.children(&mut w) {
        if ch.is_named() && ch.kind() == "text" {
            return Some(ch.start_byte());
        }
    }
    None
}

fn collect_guards(tree: &tree_sitter::Tree, src: &str) -> Vec<ValueUse> {
    let mut out = Vec::new();
    let root = tree.root_node();
    let mut stack = vec![root];

    while let Some(n) = stack.pop() {
        if is_ctrl_flow(n.kind()) {
            let body_start = first_text_start(&n).unwrap_or(usize::MAX);

            let mut w = n.walk();
            for ch in n.children(&mut w) {
                if !ch.is_named() {
                    continue;
                }
                if ch.kind() == "text" {
                    break;
                }
                // everything before first "text" is the guard region
                if ch.end_byte() <= body_start {
                    for v in collect_values_in_subtree(&ch, src) {
                        out.push(ValueUse {
                            value_path: v,
                            role: Role::Guard,
                            action_span: ch.byte_range(),
                            yaml_path: None,
                        });
                    }
                } else {
                    break;
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

pub fn analyze_template_file(path: &VfsPath) -> Result<Vec<ValueUse>, Error> {
    let source = path.read_to_string()?;
    let parsed = parse_gotmpl_document(&source).ok_or(Error::ParseTemplate)?;

    // Index sibling defines (*.tpl)
    let dir = path.parent();
    let mut files = Vec::<(VfsPath, Arc<String>)>::new();
    for p in dir.read_dir()? {
        if p.filename().ends_with(".tpl") {
            files.push((p.clone(), Arc::new(p.read_to_string()?)));
        }
    }
    let define_index = collect_defines(&files)?;

    analyze_template(&parsed.tree, &source, define_index)
}

pub fn analyze_template(
    tree: &tree_sitter::Tree,
    source: &str,
    define_index: DefineIndex,
) -> Result<Vec<ValueUse>, Error> {
    // let ast = helm_schema_template::fmt::SExpr::parse_tree(&parsed.tree.root_node(), &source);
    // println!("{}", ast.to_string_pretty());

    let inline = InlineState {
        define_index: &define_index,
    };
    let mut guard = ExpansionGuard::new();

    // Start scope at chart root (treat dot/dollar as .Values by convention for analysis)
    let scope = Scope {
        dot: ExprOrigin::Selector(vec!["Values".into()]),
        dollar: ExprOrigin::Selector(vec!["Values".into()]),
        bindings: Default::default(),
    };

    // Inline + sanitize to a single YAML buffer with placeholders
    let mut out = InlineOut::default();
    inline_emit_tree(tree, &source, &scope, &inline, &mut guard, &mut out)?;

    println!(
        "=== SANITIZED YAML ===\n\n{}\n========================",
        out.buf
    );

    // Sanity: the YAML needs to parse so we can compute placeholder paths
    if let Err(e) = crate::sanitize::validate_yaml_strict_all_docs(&out.buf) {
        eprintln!("{}", crate::sanitize::pretty_yaml_error(&out.buf, &e));
        return Err(Error::YamlParse);
    }

    // Compute YAML paths for placeholders (key/value position & exact YamlPath)
    let ph_map = compute_yaml_paths_for_placeholders(&out.buf).map_err(|_| Error::YamlParse)?;

    // Convert placeholders → ValueUse rows
    let mut uses = Vec::<ValueUse>::new();
    for ph in out.placeholders {
        let binding = ph_map.get(&ph.id).cloned().unwrap_or_default();
        // Role: key/value from YAML unless we force Fragment for fragment outputs
        let role = if ph.is_fragment_output {
            Role::Fragment
        } else {
            binding.role
        };
        let yaml_path = binding.path;
        for v in ph.values {
            uses.push(ValueUse {
                value_path: v,
                role,
                action_span: ph.action_span.clone(),
                yaml_path: yaml_path.clone(),
            });
        }
    }

    // Add guard uses (no yaml_path)
    uses.extend(out.guards.into_iter());

    // Dedup identical rows
    let mut seen = HashSet::new();
    uses.retain(|u| {
        seen.insert((
            u.value_path.clone(),
            u.role,
            u.action_span.start,
            u.action_span.end,
            u.yaml_path.clone(),
        ))
    });

    Ok(uses)
}

fn collect_values_in_subtree(node: &Node, src: &str) -> Vec<String> {
    // NOTE: we capture a global/static? no; we’ll override this via a closure in analyze_template_file (see below)
    // This plain version remains here; the closure will wrap it to add include-closure values.
    let mut out = BTreeSet::<String>::new();
    let mut stack = vec![*node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "selector_expression" => {
                let parent_is_selector = n
                    .parent()
                    .map(|p| p.kind() == "selector_expression")
                    .unwrap_or(false);
                if !parent_is_selector {
                    if let Some(segs0) = parse_selector_chain(n, src) {
                        let segs = normalize_segments(&segs0);
                        // Keep only .Values.* and strip the "Values" root for the returned key
                        if segs.len() >= 2
                            && segs[0] == "."
                            && segs[1] == "Values"
                            && segs.len() > 2
                        {
                            out.insert(segs[2..].join("."));
                        } else if segs.len() >= 2
                            && segs[0] == "$"
                            && segs[1] == "Values"
                            && segs.len() > 2
                        {
                            out.insert(segs[2..].join("."));
                        } else if !segs.is_empty() && segs[0] == "Values" && segs.len() > 1 {
                            out.insert(segs[1..].join("."));
                        }
                    }
                }
            }
            // "selector_expression" => {
            //     let parent_is_selector = n
            //         .parent()
            //         .map(|p| p.kind() == "selector_expression")
            //         .unwrap_or(false);
            //     if !parent_is_selector {
            //         if let Some(segs) =
            //             helm_schema_template::values::parse_selector_expression(&n, src)
            //         {
            //             if !segs.is_empty() {
            //                 out.insert(segs.join("."));
            //             }
            //         }
            //     }
            // }
            "function_call" => {
                if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
                    if !segs.is_empty() {
                        out.insert(segs.join("."));
                    }
                }
                // Handle grammar quirk: `.Values.foo` as `function_call(Values, .foo)`
                if let Some(k) = parse_values_field_call(&n, src) {
                    out.insert(k);
                }
                // NOTE: include-closure injection is handled by the wrapper closure in analyze_template_file
            }
            _ => {}
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }
    out.into_iter().collect()
}

fn function_name_of(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "function_call" {
        return None;
    }
    // Try a field first (some grammars expose it)…
    if let Some(f) = node.child_by_field_name("function") {
        if let Ok(s) = f.utf8_text(src.as_bytes()) {
            return Some(s.trim().to_string());
        }
    }
    // …otherwise fall back to the first identifier child.
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

fn arg_list<'tree>(node: &Node<'tree>) -> Option<Node<'tree>> {
    node.child_by_field_name("argument_list")
        .or_else(|| node.child_by_field_name("arguments"))
}

fn pipeline_head(node: Node) -> Option<Node> {
    match node.kind() {
        // e.g., {{ .host | quote }} → head is `.host`
        "chained_pipeline" | "parenthesized_pipeline" => node.named_child(0),
        // treat anything else as its own head (selector_expression, field, dot, variable, function_call)
        _ => Some(node),
    }
}

/// Best-effort parse for the grammar variant where `.Values.foo` is tokenized
/// as a function call `Values` with a single argument that is a `field` node `.foo`.
/// Returns the extracted Values key without the `.Values.` prefix, e.g. "commonLabels".
fn parse_values_field_call(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "function_call" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if func.kind() != "identifier" {
        return None;
    }
    let fname = func.utf8_text(src.as_bytes()).ok()?;
    if fname != "Values" {
        return None;
    }
    let args = arg_list(node)?;
    let mut w = args.walk();
    for ch in args.named_children(&mut w) {
        if ch.kind() == "field" {
            // field node text looks like ".commonLabels"
            let raw = ch.utf8_text(src.as_bytes()).ok()?.trim();
            let trimmed = raw.strip_prefix('.').unwrap_or(raw);
            // Only a single segment is expected in this odd form
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

// Info we index for every {{ define "name" }}...{{ end }}
#[derive(Debug, Clone, Default)]
pub struct DefineInfo {
    pub values: BTreeSet<String>,   // .Values.* used inside this define body
    pub includes: BTreeSet<String>, // nested {{ include "..." ... }} calls
}

// Best-effort string unquote ( "foo", `foo` )
fn unquote(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('`') && t.ends_with('`')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

// Find include name if this is {{ include "name" ... }}
fn include_call_name(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "function_call" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if !(func.kind() == "identifier" && func.utf8_text(src.as_bytes()).ok()? == "include") {
        return None;
    }
    // let args = node.child_by_field_name("arguments")?;
    let args = arg_list(node)?;
    let mut c = args.walk();
    for ch in args.named_children(&mut c) {
        // first string literal is the template name
        if ch.kind() == "interpreted_string_literal" || ch.kind() == "raw_string_literal" {
            let name = unquote(&src[ch.byte_range()]);
            return Some(name);
        }
    }
    None
}

// Get define name for {{ define "name" }} body node
fn define_name(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "define_action" {
        return None;
    }
    if let Some(n) = node.child_by_field_name("name") {
        return Some(unquote(&src[n.byte_range()]));
    }
    // fallback: first string literal child
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if !ch.is_named() {
            continue;
        }
        if ch.kind() == "interpreted_string_literal" || ch.kind() == "raw_string_literal" {
            return Some(unquote(&src[ch.byte_range()]));
        }
    }
    None
}

// Collect .Values + nested include names inside a define’s subtree
fn collect_define_info(node: &Node, src: &str) -> DefineInfo {
    let mut values = BTreeSet::<String>::new();
    let mut includes = BTreeSet::<String>::new();

    let root_id = node.id(); // <-- NEW: remember which define we were called with
    let mut stack = vec![*node];

    while let Some(n) = stack.pop() {
        match n.kind() {
            "selector_expression" => {
                // OUTERMOST selector only
                let parent_is_selector = n
                    .parent()
                    .map(|p| p.kind() == "selector_expression")
                    .unwrap_or(false);
                if !parent_is_selector {
                    if let Some(segs0) = parse_selector_chain(n, src) {
                        let segs = normalize_segments(&segs0);
                        if segs.len() >= 2
                            && segs[0] == "."
                            && segs[1] == "Values"
                            && segs.len() > 2
                        {
                            values.insert(segs[2..].join("."));
                        } else if segs.len() >= 2
                            && segs[0] == "$"
                            && segs[1] == "Values"
                            && segs.len() > 2
                        {
                            values.insert(segs[2..].join("."));
                        } else if !segs.is_empty() && segs[0] == "Values" && segs.len() > 1 {
                            values.insert(segs[1..].join("."));
                        }
                    }
                }
            }
            // "selector_expression" => {
            //     // OUTERMOST selector only
            //     let parent_is_selector = n
            //         .parent()
            //         .map(|p| p.kind() == "selector_expression")
            //         .unwrap_or(false);
            //     if !parent_is_selector {
            //         if let Some(segs) =
            //             helm_schema_template::values::parse_selector_expression(&n, src)
            //         {
            //             if !segs.is_empty() {
            //                 values.insert(segs.join("."));
            //             }
            //         }
            //     }
            // }
            "function_call" => {
                if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
                    if !segs.is_empty() {
                        values.insert(segs.join("."));
                    }
                }
                // Also catch `.Values.foo` parsed as `Values .foo`
                if let Some(k) = parse_values_field_call(&n, src) {
                    values.insert(k);
                }
                if let Some(name) = include_call_name(&n, src) {
                    includes.insert(name);
                }
            }
            "define_action" => {
                // If this is a nested define (not the root define), do not recurse into it.
                if n.id() != root_id {
                    continue;
                }
                // otherwise (the root define) fall through to push its children below
            }
            _ => {}
        }

        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }

    DefineInfo { values, includes }
}

// Index all defines in a directory (e.g., templates/_*.tpl)
pub fn index_defines_in_dir(dir: &VfsPath) -> Result<BTreeMap<String, DefineInfo>, Error> {
    let mut out = BTreeMap::<String, DefineInfo>::new();
    for path in dir.read_dir()? {
        let name = path.filename();
        // keep it simple: process *.tpl only
        if !name.ends_with(".tpl") {
            continue;
        }
        dbg!(&path);
        let src = path.read_to_string()?;
        if let Some(parsed) = helm_schema_template::parse::parse_gotmpl_document(&src) {
            let root = parsed.tree.root_node();

            // let ast = helm_schema_template::fmt::SExpr::parse_tree(&root, &src);
            // println!("{}", ast.to_string_pretty());

            let mut stack = vec![root];
            while let Some(n) = stack.pop() {
                if n.kind() == "define_action" {
                    if let Some(define_name) = define_name(&n, &src) {
                        let info = collect_define_info(&n, &src);
                        out.entry(define_name.clone())
                            .or_default()
                            .values
                            .extend(info.values);
                        out.entry(define_name)
                            .or_default()
                            .includes
                            .extend(info.includes);
                    }
                    // don’t recurse into define bodies again; continue
                    continue;
                }
                let mut c = n.walk();
                for ch in n.children(&mut c) {
                    if ch.is_named() {
                        stack.push(ch);
                    }
                }
            }
        }
    }
    Ok(out)
}

// Transitive closure of .Values per define (follows nested includes)
pub fn compute_define_closure(
    defs: &BTreeMap<String, DefineInfo>,
) -> BTreeMap<String, BTreeSet<String>> {
    fn dfs<'a>(
        name: &str,
        defs: &'a BTreeMap<String, DefineInfo>,
        seen: &mut BTreeSet<String>,
        memo: &mut BTreeMap<String, BTreeSet<String>>,
    ) -> BTreeSet<String> {
        if let Some(m) = memo.get(name) {
            return m.clone();
        }
        if !seen.insert(name.to_string()) {
            return BTreeSet::new();
        }

        let mut acc = BTreeSet::<String>::new();
        if let Some(info) = defs.get(name) {
            acc.extend(info.values.clone());
            for inc in &info.includes {
                acc.extend(dfs(inc, defs, seen, memo));
            }
        }
        memo.insert(name.to_string(), acc.clone());
        acc
    }

    let mut memo = BTreeMap::<String, BTreeSet<String>>::new();
    for key in defs.keys() {
        let mut seen = BTreeSet::new();
        let _ = dfs(key, defs, &mut seen, &mut memo);
    }
    memo
}

#[derive(Debug, Clone)]
pub struct DefineEntry {
    pub name: String,
    pub path: VfsPath,
    pub source: std::sync::Arc<String>,
    // Byte range within `source` for the body of the define.
    pub body_byte_range: std::ops::Range<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct DefineIndex {
    map: HashMap<String, DefineEntry>,
}

impl DefineIndex {
    pub fn insert(&mut self, e: DefineEntry) {
        self.map.insert(e.name.clone(), e);
    }

    pub fn get(&self, name: &str) -> Option<&DefineEntry> {
        self.map.get(name)
    }
}

/// Find nodes by kind (fast + tiny)
fn find_nodes<'tree>(tree: &'tree tree_sitter::Tree, kind: &str) -> Vec<Node<'tree>> {
    let mut out = Vec::new();
    let mut cursor = tree.root_node().walk();
    let root = tree.root_node();

    fn rec<'a>(node: Node<'a>, kind: &str, out: &mut Vec<Node<'a>>) {
        if node.kind() == kind {
            out.push(node);
        }
        let mut c = node.walk();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                rec(child, kind, out);
            }
        }
        drop(c);
    }

    rec(root, kind, &mut out);
    drop(cursor);
    out
}

/// Extract define name and an approximate body slice.
/// Grammar nuances differ, so we simply:
/// - look for the first string literal child => template name (strip quotes)
/// - body = node byte range minus the outermost `{{- define ... -}}` and the trailing `{{- end -}}`
///   We heuristically trim the first newline after the name, and the last `{{` opener of "end".
fn extract_define_name_and_body(
    src: &str,
    node: Node,
) -> (Option<String>, Option<std::ops::Range<usize>>) {
    // 1) name
    let mut name: Option<String> = None;
    for i in 0..node.child_count() {
        if let Some(ch) = node.child(i) {
            let kd = ch.kind();
            if kd == "interpreted_string_literal" || kd == "raw_string_literal" || kd == "string" {
                let text = ch.utf8_text(src.as_bytes()).unwrap_or("");
                // text is quoted ("foo"); strip
                let trimmed = text.trim();
                let nm = trimmed.trim_matches('"').trim_matches('`').to_string();
                name = Some(nm);
                break;
            }
        }
    }
    // 2) body slice (best-effort)
    let node_range = node.byte_range();
    let slice = &src.as_bytes()[node_range.clone()];
    // find the last "{{" that starts the closing end action
    let hay = std::str::from_utf8(slice).unwrap_or("");
    let end_pos = hay.rfind("{{").unwrap_or(hay.len());
    let start_pos = hay.find("}}").map(|p| p + 2).unwrap_or(0); // after the opening action
    let body_start = node_range.start + start_pos;
    let body_end = node_range.start + end_pos;
    let body = if body_end > body_start {
        Some(body_start..body_end)
    } else {
        None
    };
    (name, body)
}

fn collect_defines(files: &[(VfsPath, Arc<String>)]) -> Result<DefineIndex, Error> {
    let mut idx = DefineIndex::default();
    for (path, src) in files {
        println!("\n======================= AST for helper {}", path.as_str());

        let parsed = parse_gotmpl_document(src).ok_or(Error::ParseTemplate)?;

        // let ast = helm_schema_template::fmt::SExpr::parse_tree(&parsed.tree.root_node(), src);
        // println!("{}", ast.to_string_pretty());
        // println!("");

        for node in find_nodes(&parsed.tree, "define_action") {
            if let (Some(name), Some(body_range)) = extract_define_name_and_body(src, node) {
                idx.insert(DefineEntry {
                    name,
                    path: path.clone(),
                    source: src.clone(),
                    body_byte_range: body_range,
                });
            }
        }
    }
    Ok(idx)
}

#[derive(Debug, Clone)]
enum ExprOrigin {
    // A canonicalized chain like `.Values.persistentVolumeClaims`
    Selector(Vec<String>),
    // The current dot (whatever it canonicalizes to)
    Dot,
    // The root `$` from the caller
    Dollar,
    // Unknown / dynamic
    Opaque,
}

fn origin_to_value_path(o: &ExprOrigin) -> Option<String> {
    match o {
        ExprOrigin::Selector(s) if !s.is_empty() && s[0] == "Values" => {
            // turn ["Values","persistentVolumeClaims","name"] into "persistentVolumeClaims.name"
            Some(s[1..].join("."))
        }
        _ => None,
    }
}

fn normalize_segments(raw: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(raw.len() + 4);
    for s in raw {
        if s == "." || s == "$" || s == "Values" {
            out.push(s.clone());
            continue;
        }
        // $cfg → ["$", "cfg"]
        if s.starts_with('$') && s.len() > 1 {
            out.push("$".to_string());
            out.push(s[1..].to_string());
            continue;
        }
        // .config → [".", "config"]
        if s.starts_with('.') {
            out.push(".".to_string());
            let trimmed = s.trim_start_matches('.');
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
            continue;
        }
        out.push(s.clone());
    }
    out
}

fn is_helm_builtin_root(name: &str) -> bool {
    matches!(name, "Capabilities" | "Chart" | "Release" | "Template")
}

#[derive(Debug, Clone)]
struct Scope {
    // What does `.` mean here?
    dot: ExprOrigin,
    // What does `$` mean here? (usually root; we carry it through)
    dollar: ExprOrigin,
    // Local bindings for paths inside the define, e.g. "config" => <call-site expr origin>
    bindings: HashMap<String, ExprOrigin>,
}

impl Scope {
    pub fn bind_dot(&mut self, origin: ExprOrigin) {
        self.dot = origin;
    }

    pub fn dot_origin(&self) -> ExprOrigin {
        self.dot.clone()
    }

    fn bind_key(&mut self, key: &str, origin: ExprOrigin) {
        self.bindings.insert(key.to_string(), origin);
    }

    // Resolve something like `.config.size` inside the define into a canonical value_path at the call site.
    fn resolve_selector(&self, segments: &[String]) -> ExprOrigin {
        if segments
            .iter()
            .any(|s| s.starts_with('.') || s.starts_with('$'))
        {
            eprintln!("[resolve pre] raw={:?}", segments);
        }

        // NEW: normalize unexpected tokenizations (".config", "$cfg", etc.)
        let segments = normalize_segments(segments);

        if segments.is_empty() {
            return self.dot.clone();
        }

        let selector = match segments[0].as_str() {
            "." => {
                // .Values.* should be absolute, *not* relative to the current dot/bindings
                if segments.len() >= 2 && segments[1].as_str() == "Values" {
                    let mut v = vec!["Values".to_string()];
                    if segments.len() > 2 {
                        v.extend_from_slice(&segments[2..]);
                    }
                    return ExprOrigin::Selector(v);
                }

                // Ignore Helm builtins like `.Capabilities.*`, `.Chart.*`, etc.
                if segments.len() >= 2 && is_helm_builtin_root(&segments[1]) {
                    return ExprOrigin::Opaque;
                }

                if segments.len() == 1 {
                    return self.dot.clone();
                }
                let first_field = segments[1].as_str();
                if let Some(bound) = self.bindings.get(first_field) {
                    if segments.len() == 2 {
                        return bound.clone();
                    }
                    return self.extend_origin(bound.clone(), &segments[2..]);
                }
                // Otherwise extend current dot
                return self.extend_origin(self.dot.clone(), &segments[1..]);
            }
            "$" => {
                // Ignore `$ .Capabilities.*` / `$ .Chart.*` / etc.
                if segments.len() >= 2 && is_helm_builtin_root(&segments[1]) {
                    return ExprOrigin::Opaque;
                }
                if segments.len() == 1 {
                    return self.dollar.clone();
                }
                return self.extend_origin(self.dollar.clone(), &segments[1..]);
            }
            first => {
                // Ignore bare `Capabilities.*` / `Chart.*` / etc.
                if is_helm_builtin_root(first) {
                    return ExprOrigin::Opaque;
                }

                // allow "config" binding; also handle a stray leading dot already removed above
                let first_norm = first;
                if let Some(bound) = self.bindings.get(first_norm) {
                    if segments.len() == 1 {
                        return bound.clone();
                    }
                    return self.extend_origin(bound.clone(), &segments[1..]);
                }
                if first_norm == "Values" {
                    let mut v = vec!["Values".to_string()];
                    v.extend_from_slice(&segments[1..]);
                    return ExprOrigin::Selector(v);
                }
                if let ExprOrigin::Selector(_) = self.dot {
                    return self.extend_origin(self.dot.clone(), &segments);
                }
                ExprOrigin::Opaque
            } // first => {
              //     if let Some(bound) = self.bindings.get(first) {
              //         if segments.len() == 1 {
              //             return bound.clone();
              //         }
              //         return self.extend_origin(bound.clone(), &segments[1..]);
              //     }
              //     if first == "Values" {
              //         let mut v = vec!["Values".to_string()];
              //         v.extend_from_slice(&segments[1..]);
              //         return ExprOrigin::Selector(v);
              //     }
              //     if let ExprOrigin::Selector(_) = self.dot {
              //         return self.extend_origin(self.dot.clone(), segments);
              //     }
              //     ExprOrigin::Opaque
              // }
        };

        eprintln!("[resolve] {:?} -> {:?}", segments, selector);

        selector
    }

    fn extend_origin(&self, base: ExprOrigin, tail: &[String]) -> ExprOrigin {
        match base {
            ExprOrigin::Selector(mut v) => {
                // Remove accidental leading '.' in tail segments
                let mut t: Vec<String> = tail
                    .iter()
                    .map(|s| s.trim_start_matches('.').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                // Drop duplicated "Values" when joining selector pieces
                if !v.is_empty() && v[0] == "Values" && !t.is_empty() && t[0] == "Values" {
                    t.remove(0);
                }
                v.extend(t);
                ExprOrigin::Selector(v)

                // // Drop a duplicated leading "Values" (e.g. ["Values"] + ["Values","x"]).
                // let mut t: Vec<String> = tail.to_vec();
                // if !v.is_empty() && v[0] == "Values" && !t.is_empty() && t[0] == "Values" {
                //     t.remove(0);
                // }
                // v.extend(t);
                // ExprOrigin::Selector(v)
            }
            ExprOrigin::Dot | ExprOrigin::Dollar | ExprOrigin::Opaque => ExprOrigin::Opaque,
        }
    }
}

fn guard_selectors_for_action(node: Node, src: &str, scope: &Scope) -> Vec<ExprOrigin> {
    // Everything before the first named "text" child is the guard region.
    // Walk that subtree with the robust collector so we also catch selectors under `and`, etc.
    let mut out: Vec<ExprOrigin> = Vec::new();
    let mut uniq = std::collections::BTreeSet::<String>::new();

    // Find guard/body boundary
    let body_start = first_text_start(&node).unwrap_or(usize::MAX);

    let mut w = node.walk();
    for ch in node.children(&mut w) {
        if !ch.is_named() {
            continue;
        }
        // stop when we hit the body
        if ch.end_byte() > body_start {
            break;
        }

        // Use the robust walker (recursive) so nested selectors inside `function_call`s are included.
        // We don't need $var expansion for the Signoz case; use an empty env here.
        let keys = collect_values_with_scope(ch, src, scope, &Env::default());
        for k in keys {
            if k.is_empty() {
                continue;
            }
            if uniq.insert(k.clone()) {
                // Turn "a.b.c" into ["Values","a","b","c"] so callers can resolve/rebind dot.
                let mut segs = Vec::with_capacity(1 + k.matches('.').count() + 1);
                segs.push("Values".to_string());
                segs.extend(k.split('.').map(|s| s.to_string()));
                out.push(ExprOrigin::Selector(segs));
            }
        }
    }

    out
}

// fn guard_selectors_for_action(node: Node, src: &str, scope: &Scope) -> Vec<ExprOrigin> {
//     // everything before the first named "text" child is the guard region
//     let mut out = Vec::new();
//     let mut w = node.walk();
//     let mut seen_text = false;
//     for ch in node.children(&mut w) {
//         if !ch.is_named() {
//             continue;
//         }
//         if ch.kind() == "text" {
//             seen_text = true;
//             break;
//         }
//
//         // Collect *outermost* selector in guard
//         if ch.kind() == "selector_expression" {
//             if let Some(segs) = parse_selector_chain(ch, src) {
//                 out.push(scope.resolve_selector(&segs));
//             }
//         } else if ch.kind() == "dot" {
//             out.push(scope.dot.clone());
//         } else if ch.kind() == "variable" {
//             // `$` variable in guard
//             let t = ch.utf8_text(src.as_bytes()).unwrap_or("");
//             if t.trim() == "$" {
//                 out.push(scope.dollar.clone());
//             }
//         } else if ch.kind() == "field" {
//             // Handle guards like `range .accessModes` where guard is a `field`
//             // Build a relative selector like [".", "<name>"]
//             let ident = ch
//                 .child_by_field_name("identifier")
//                 .or_else(|| ch.child_by_field_name("field_identifier"))
//                 .and_then(|id| id.utf8_text(src.as_bytes()).ok())
//                 .or_else(|| ch.utf8_text(src.as_bytes()).ok())
//                 .unwrap_or_default();
//
//             let name = ident.trim().trim_start_matches('.').to_string();
//             if !name.is_empty() {
//                 out.push(scope.resolve_selector(&vec![".".into(), name]));
//             }
//             // let ident = ch
//             //     .child_by_field_name("identifier")
//             //     .or_else(|| ch.child_by_field_name("field_identifier"))
//             //     .and_then(|id| id.utf8_text(src.as_bytes()).ok())
//             //     .unwrap_or_else(|| &src[ch.byte_range()]);
//             //
//             // let name = ident.trim().trim_start_matches('.').to_string();
//             // if !name.is_empty() {
//             //     out.push(scope.resolve_selector(&vec![".".into(), name]));
//             // }
//         }
//         // Note: could add more cases (function_call pipelines), but this covers our PVC shapes
//     }
//     out
// }

/// Arg expression for include's second parameter
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ArgExpr {
    Dot,
    Dollar,
    /// e.g. .Values.persistentVolumeClaims
    Selector(Vec<String>),
    /// (dict "key" <expr> ...)
    Dict(Vec<(String, ArgExpr)>),
    /// we don't understand
    Opaque,
}

impl ArgExpr {
    fn selector<S>(segments: impl IntoIterator<Item = S>) -> Self
    where
        S: Into<String>,
    {
        Self::Selector(segments.into_iter().map(Into::into).collect())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum IncludeError {
    #[error("unknown define with name {0:?}")]
    UnknownDefine(String),
    #[error("not an include")]
    NotAnInclude,
    #[error("include is missing an argument list")]
    MissingArgumentList,
    #[error("include is missing name")]
    MissingName,
}

/// Parse `include "tpl.name" <arg>` out of a function_call node
fn parse_include_call(call_node: Node, src: &str) -> Result<(String, ArgExpr), IncludeError> {
    // Ensure it's an include
    // The grammar nodes in gotmpl crate name the function identifier node as "identifier"
    let mut name_ok = false;
    let mut arglist: Option<Node> = None;
    for i in 0..call_node.child_count() {
        let ch = call_node.child(i).unwrap();
        match ch.kind() {
            "identifier" => {
                let id = ch.utf8_text(src.as_bytes()).unwrap_or("");
                if id == "include" {
                    name_ok = true;
                }
            }
            "argument" | "argument_list" => arglist = Some(ch),
            _ => {}
        }
    }
    if !name_ok {
        return Err(IncludeError::NotAnInclude);
    }
    let arglist = arglist.ok_or(IncludeError::MissingArgumentList)?;

    // Expect first arg: string literal (template name)
    let mut tpl_name: Option<String> = None;
    let mut second_arg: Option<Node> = None;
    let mut found_first = false;
    for i in 0..arglist.child_count() {
        let ch = arglist.child(i).unwrap();
        match ch.kind() {
            "interpreted_string_literal" | "raw_string_literal" | "string" if !found_first => {
                let t = ch
                    .utf8_text(src.as_bytes())
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('`')
                    .to_string();
                tpl_name = Some(t);
                found_first = true;
            }
            _ if found_first => {
                // first token after the name that is not whitespace/comment
                if ch.is_named() {
                    second_arg = Some(ch);
                    break;
                }
            }
            _ => {}
        }
    }

    let tpl_name = tpl_name.ok_or(IncludeError::MissingName)?;

    // Parse second arg to ArgExpr
    fn parse_arg_expr(node: Node, src: &str) -> ArgExpr {
        match node.kind() {
            "dot" => ArgExpr::Dot,
            "variable" => {
                // `$` root variable
                let t = node.utf8_text(src.as_bytes()).unwrap_or("");
                if t.trim() == "$" {
                    ArgExpr::Dollar
                } else {
                    ArgExpr::Opaque
                }
            }
            // "selector_expression" => {
            //     if let Some(segs) =
            //         helm_schema_template::values::parse_selector_expression(&node, src)
            //     {
            //         ArgExpr::Selector(segs)
            //     } else {
            //         ArgExpr::Opaque
            //     }
            // }
            "selector_expression" => {
                // Robustly extract segments, then re-introduce the syntactic prefix we care about
                // so tests can assert the normalized form:
                //   .Values.foo   => Selector([".", "Values", "foo"])
                //   .Values       => Selector([".", "Values"])
                //   (.Values.foo) => Selector([".", "Values", "foo"])
                let text = &src[node.byte_range()];
                let trimmed = text.trim();

                if let Some(mut segs) =
                    helm_schema_template::values::parse_selector_expression(&node, src)
                {
                    let mut out = Vec::<String>::new();

                    // If the original text starts with '.', we want to preserve that as a prefix token.
                    if trimmed.starts_with('.') {
                        out.push(".".to_string());

                        // If it specifically starts with ".Values", ensure "Values" immediately follows.
                        // parse_selector_expression() may or may not include "Values" as the first segment,
                        // depending on grammar/tokenization; normalize either way.
                        if trimmed.starts_with(".Values") {
                            // Avoid duplicating "Values"
                            if !segs.is_empty() && segs[0] == "Values" {
                                // keep segs as-is; we'll just add "."
                            } else {
                                out.push("Values".to_string());
                            }
                        }
                    }

                    // If we already injected ".Values", drop a redundant leading "Values" from segs.
                    if out.len() >= 2 && out[0] == "." && out[1] == "Values" {
                        if !segs.is_empty() && segs[0] == "Values" {
                            segs.remove(0);
                        }
                    }

                    out.extend(segs);
                    ArgExpr::Selector(out)
                } else {
                    ArgExpr::Opaque
                }
            }
            // // NEW: handle `.ctx` / `.something` passed to include
            // "field" => {
            //     let name = node
            //         .child_by_field_name("identifier")
            //         .or_else(|| node.child(0))
            //         .and_then(|id| id.utf8_text(src.as_bytes()).ok())
            //         .map(|s| s.trim().trim_start_matches('.').to_string());
            //     if let Some(nm) = name {
            //         ArgExpr::Selector(vec![".".to_string(), nm])
            //     } else {
            //         ArgExpr::Opaque
            //     }
            // }
            // NEW: handle `.ctx` / `.something` passed to include (robust to grammar variants)
            "field" => {
                // Prefer explicit identifier nodes; fall back to first *named* child; and finally to full node text.
                let ident_node = node
                    .child_by_field_name("identifier")
                    .or_else(|| node.child_by_field_name("field_identifier"))
                    .or_else(|| {
                        let mut w = node.walk();
                        node.named_children(&mut w).find(|ch| {
                            let k = ch.kind();
                            k == "identifier" || k == "field_identifier"
                        })
                    });

                // Try to get the identifier text; if that fails, fall back to full `.foo`
                let raw_name = ident_node
                    .and_then(|id| {
                        id.utf8_text(src.as_bytes())
                            .ok()
                            .map(|s| s.trim().to_string())
                    })
                    .or_else(|| {
                        node.utf8_text(src.as_bytes())
                            .ok()
                            .map(|s| s.trim().to_string())
                    });

                let nm = raw_name
                    .map(|s| s.trim_start_matches('.').to_string())
                    .filter(|s| !s.is_empty());

                if let Some(nm) = nm {
                    ArgExpr::Selector(vec![".".to_string(), nm])
                } else {
                    ArgExpr::Opaque
                }
            }
            "parenthesized_pipeline" => {
                // possibly (dict ...)
                if let Some(inner) = node.named_child(0) {
                    parse_arg_expr(inner, src)
                } else {
                    ArgExpr::Opaque
                }
            }
            "function_call" => {
                // Only special-case dict; other function calls are opaque as include args.
                if function_name_of(&node, src).as_deref() == Some("dict") {
                    if let Some(args) = arg_list(&node) {
                        let mut kvs: Vec<(String, ArgExpr)> = Vec::new();
                        let mut j = 0;
                        while j < args.child_count() {
                            let k_node = args.child(j).unwrap();
                            if !k_node.is_named() {
                                j += 1;
                                continue;
                            }
                            if !matches!(
                                k_node.kind(),
                                "interpreted_string_literal" | "raw_string_literal" | "string"
                            ) {
                                break;
                            }
                            let key = k_node
                                .utf8_text(src.as_bytes())
                                .unwrap_or("")
                                .trim()
                                .trim_matches('"')
                                .trim_matches('`')
                                .to_string();

                            j += 1;
                            while j < args.child_count() && !args.child(j).unwrap().is_named() {
                                j += 1;
                            }
                            if j >= args.child_count() {
                                break;
                            }
                            let v_node = args.child(j).unwrap();
                            let val = parse_arg_expr(v_node, src);
                            kvs.push((key, val));
                            j += 1;
                        }
                        return ArgExpr::Dict(kvs);
                    }
                }
                ArgExpr::Opaque
            }
            // "function_call" => {
            //     // maybe dict "k" <expr> ...
            //     let mut is_dict = false;
            //     let mut kvs: Vec<(String, ArgExpr)> = Vec::new();
            //     let mut ident_seens = 0;
            //     for i in 0..node.child_count() {
            //         let ch = node.child(i).unwrap();
            //         match ch.kind() {
            //             "identifier" => {
            //                 let id = ch.utf8_text(src.as_bytes()).unwrap_or("");
            //                 if id.trim() == "dict" {
            //                     is_dict = true;
            //                 }
            //             }
            //             "argument_list" if is_dict => {
            //                 // parse pairs: "k" <expr> ...
            //                 let args = ch;
            //                 let mut j = 0;
            //                 while j < args.child_count() {
            //                     // find string key
            //                     let k_node = args.child(j).unwrap();
            //                     if !k_node.is_named() {
            //                         j += 1;
            //                         continue;
            //                     }
            //                     if k_node.kind() != "interpreted_string_literal"
            //                         && k_node.kind() != "raw_string_literal"
            //                         && k_node.kind() != "string"
            //                     {
            //                         // not a key; bail
            //                         break;
            //                     }
            //                     let key = k_node
            //                         .utf8_text(src.as_bytes())
            //                         .unwrap_or("")
            //                         .trim()
            //                         .trim_matches('"')
            //                         .trim_matches('`')
            //                         .to_string();
            //                     // find value node
            //                     j += 1;
            //                     while j < args.child_count() && !args.child(j).unwrap().is_named() {
            //                         j += 1;
            //                     }
            //                     if j >= args.child_count() {
            //                         break;
            //                     }
            //                     let v_node = args.child(j).unwrap();
            //                     let val = parse_arg_expr(v_node, src);
            //                     kvs.push((key, val));
            //                     j += 1;
            //                 }
            //             }
            //             _ => {}
            //         }
            //     }
            //     if is_dict {
            //         ArgExpr::Dict(kvs)
            //     } else {
            //         ArgExpr::Opaque
            //     }
            // }
            _ => ArgExpr::Opaque,
        }
    }

    let arg_expr = second_arg
        .map(|n| parse_arg_expr(n, src))
        .unwrap_or(ArgExpr::Opaque);

    println!(
        "parse include {} with args {}",
        tpl_name.bright_red(),
        format!("{arg_expr:?}").bright_red()
    );

    Ok((tpl_name, arg_expr))
}

/// Prevent runaway recursion
struct ExpansionGuard {
    stack: Vec<String>,
}
impl ExpansionGuard {
    fn new() -> Self {
        Self { stack: Vec::new() }
    }
}

/// This state sits next to your existing emitter state. We keep it tiny.
#[derive(Debug, Clone)]
struct InlineState<'a> {
    define_index: &'a DefineIndex,
}

/// Walk the define body with a callee scope and let your *existing emitter* handle nodes.
/// The only "special" case we intercept here is `include` nodes to inline them recursively.
fn walk_template_ast(
    tree: &tree_sitter::Tree,
    src: &str,
    callee_scope: &Scope,
    inline: &InlineState<'_>,
    guard: &mut ExpansionGuard,
    mut on_include: impl FnMut(Node, &Scope, &mut ExpansionGuard) -> Result<(), Error>,
    mut on_fallback: impl FnMut(Node, &Scope, &mut ExpansionGuard) -> Result<(), Error>,
) -> Result<(), Error> {
    let root = tree.root_node();

    fn dfs<'a>(
        node: Node<'a>,
        src: &str,
        scope: &Scope,
        inline: &InlineState<'_>,
        guard: &mut ExpansionGuard,
        on_include: &mut impl FnMut(Node, &Scope, &mut ExpansionGuard) -> Result<(), Error>,
        on_fallback: &mut impl FnMut(Node, &Scope, &mut ExpansionGuard) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if node.kind() == "function_call" {
            // is it an include?
            // peek identifier
            let mut is_include = false;
            for i in 0..node.child_count() {
                let ch = node.child(i).unwrap();
                if ch.kind() == "identifier" {
                    let id = ch.utf8_text(src.as_bytes()).unwrap_or("");
                    if id.trim() == "include" {
                        is_include = true;
                        break;
                    }
                }
            }
            if is_include {
                on_include(node, scope, guard)?;
                return Ok(());
            }
        }
        // default: hand off to fallback (your existing node handler),
        // and recurse into children
        on_fallback(node, scope, guard)?;
        for i in 0..node.child_count() {
            if let Some(ch) = node.child(i) {
                dfs(ch, src, scope, inline, guard, on_include, on_fallback)?;
            }
        }
        Ok(())
    }
    dfs(
        root,
        src,
        callee_scope,
        inline,
        guard,
        &mut on_include,
        &mut on_fallback,
    )
}

/// Load all templates under templates/ and index defines
fn build_define_index(root_templates: &[(vfs::VfsPath, Arc<String>)]) -> DefineIndex {
    let mut idx = DefineIndex::default();
    for (path, src) in root_templates {
        if let Some(parsed) = parse_gotmpl_document(src) {
            for node in find_nodes(&parsed.tree, "define_action") {
                let (name, body) = extract_define_name_and_body(src, node);
                if let (Some(name), Some(body_range)) = (name, body) {
                    idx.insert(DefineEntry {
                        name,
                        path: path.clone(),
                        source: src.clone(),
                        body_byte_range: body_range,
                    });
                }
            }
        }
    }
    idx
}

// Tiny helper for the check above (kept separate to avoid repeating code)
fn is_include_call(call_node: Node, src: &str) -> bool {
    for i in 0..call_node.child_count() {
        let ch = call_node.child(i).unwrap();
        if ch.kind() == "identifier" {
            let id = ch.utf8_text(src.as_bytes()).unwrap_or("");
            if id.trim() == "include" {
                return true;
            }
        }
    }
    false
}

fn parse_selector_chain(node: Node, src: &str) -> Option<Vec<String>> {
    fn rec(n: Node, src: &str, out: &mut Vec<String>) {
        match n.kind() {
            "selector_expression" => {
                let left = n
                    .child_by_field_name("operand")
                    .or_else(|| n.child(0))
                    .unwrap();
                let right = n
                    .child_by_field_name("field")
                    .or_else(|| n.child(1))
                    .unwrap();
                rec(left, src, out);
                let id = right
                    .utf8_text(src.as_bytes())
                    .ok()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !id.is_empty() {
                    out.push(id);
                }
            }
            "dot" => out.push(".".into()),
            "variable" => {
                let t = n.utf8_text(src.as_bytes()).ok().unwrap_or("").trim();
                if t == "$" {
                    out.push("$".into());
                }
            }
            "identifier" | "field" | "field_identifier" => {
                let id = n
                    .utf8_text(src.as_bytes())
                    .ok()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !id.is_empty() {
                    out.push(id);
                }
            }
            _ => {}
        }
    }
    let mut segs = Vec::new();
    rec(node, src, &mut segs);
    if segs.is_empty() { None } else { Some(segs) }
}

type Env = BTreeMap<String, BTreeSet<String>>;

fn variable_ident_of(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "variable" {
        return None;
    }
    // prefer identifier child if present
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.is_named() && ch.kind() == "identifier" {
            return ch.utf8_text(src.as_bytes()).ok().map(|s| s.to_string());
        }
    }
    node.utf8_text(src.as_bytes())
        .ok()
        .map(|t| t.trim().trim_start_matches('$').to_string())
}

/// Collect `.Values.*` keys reachable from `node` using the caller's `scope` and current `$var` env.
fn collect_values_with_scope(node: Node, src: &str, scope: &Scope, env: &Env) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![node];

    while let Some(n) = stack.pop() {
        match n.kind() {
            "dot" => {
                if let Some(vp) = origin_to_value_path(&scope.dot) {
                    if !vp.is_empty() {
                        out.insert(vp);
                    }
                }
            }
            // Handle bare `.foo` nodes
            "field" => {
                let ident = n
                    .child_by_field_name("identifier")
                    .or_else(|| n.child_by_field_name("field_identifier"))
                    .and_then(|id| id.utf8_text(src.as_bytes()).ok())
                    .unwrap_or_else(|| n.utf8_text(src.as_bytes()).unwrap_or(""));
                let name = ident.trim().trim_start_matches('.').to_string();
                if !name.is_empty() {
                    let segs = vec![".".to_string(), name];
                    let origin = scope.resolve_selector(&segs);
                    if let Some(vp) = origin_to_value_path(&origin) {
                        out.insert(vp);
                    }
                }
            }
            "selector_expression" => {
                // 0) First, if the left operand is a variable ($ or $name), expand via env — do this
                //    even if parse_selector_expression() returns None on this grammar.
                if let Some(op) = n.child_by_field_name("operand").or_else(|| n.child(0)) {
                    if op.kind() == "variable" {
                        if let Some(var) = variable_ident_of(&op, src) {
                            if let Some(bases) = env.get(&var) {
                                // Build tail after the variable using the robust chain walker
                                let segs2 = parse_selector_chain(n, src).unwrap_or_default();
                                // Skip the variable tokens (`$`, `$var`, `var`); keep the rest
                                let mut i = 0usize;
                                while i < segs2.len()
                                    && (segs2[i] == "$"
                                        || segs2[i].trim_start_matches('$') == var
                                        || segs2[i] == var)
                                {
                                    i += 1;
                                }
                                let tail = segs2[i..]
                                    .iter()
                                    .map(|s| s.trim_start_matches('.'))
                                    .filter(|s| !s.is_empty())
                                    .collect::<Vec<_>>()
                                    .join(".");

                                for base in bases {
                                    let key = if tail.is_empty() {
                                        base.clone()
                                    } else {
                                        format!("{base}.{tail}")
                                    };
                                    if !key.is_empty() {
                                        out.insert(key);
                                    }
                                }
                                // handled
                                continue;
                            }
                        }
                    }
                }

                // 1) Normal path: if we can parse the selector segments, use scope resolution
                if let Some(segs) = helm_schema_template::values::parse_selector_expression(&n, src)
                    .or_else(|| parse_selector_chain(n, src))
                {
                    let parent_is_selector = n
                        .parent()
                        .map(|p| p.kind() == "selector_expression")
                        .unwrap_or(false);

                    if parent_is_selector {
                        // We'll still traverse its children later; just don't record this node itself.
                    } else {
                        // if let Some(segs) = helm_schema_template::values::parse_selector_expression(&n, src)
                        // {
                        // Handle tokenizations that include $ in the segments
                        if !segs.is_empty() && segs[0] == "$" && segs.len() >= 2 {
                            let var = segs[1].trim_start_matches('.');
                            if let Some(bases) = env.get(var) {
                                let tail = if segs.len() > 2 {
                                    segs[2..]
                                        .iter()
                                        .map(|s| s.trim_start_matches('.'))
                                        .collect::<Vec<_>>()
                                        .join(".")
                                } else {
                                    String::new()
                                };
                                for base in bases {
                                    let key = if tail.is_empty() {
                                        base.clone()
                                    } else {
                                        format!("{base}.{tail}")
                                    };
                                    if !key.is_empty() {
                                        out.insert(key);
                                    }
                                }
                                continue;
                            }
                        }
                        if let Some(first) = segs.first() {
                            if first.starts_with('$') {
                                let var = first.trim_start_matches('$');
                                if let Some(bases) = env.get(var) {
                                    let tail = if segs.len() > 1 {
                                        segs[1..].join(".")
                                    } else {
                                        String::new()
                                    };
                                    for base in bases {
                                        let key = if tail.is_empty() {
                                            base.clone()
                                        } else {
                                            format!("{base}.{tail}")
                                        };
                                        if !key.is_empty() {
                                            out.insert(key);
                                        }
                                    }
                                    continue;
                                }
                            }
                        }

                        // Fallback: resolve through scope (. / $ / bindings / .Values)
                        let origin = scope.resolve_selector(&segs);
                        if let Some(vp) = origin_to_value_path(&origin) {
                            if !vp.is_empty() {
                                out.insert(vp);
                            }
                        }
                    }
                }
            }
            "chained_pipeline" => {
                if let Some(head) = n.named_child(0) {
                    if head.kind() == "selector_expression" {
                        if let Some(segs) =
                            helm_schema_template::values::parse_selector_expression(&head, src)
                                .or_else(|| parse_selector_chain(head, src))
                        {
                            let origin = scope.resolve_selector(&segs);
                            if let Some(vp) = origin_to_value_path(&origin) {
                                if !vp.is_empty() {
                                    out.insert(vp);
                                }
                            }
                        }
                    }
                }
            }
            "function_call" => {
                // Existing: .Values… and $ "Values" fast-paths (keep)
                if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
                    if !segs.is_empty() {
                        if segs[0] == "Values" && segs.len() > 1 {
                            out.insert(segs[1..].join("."));
                        } else if segs.len() >= 2
                            && segs[0] == "$"
                            && segs[1] == "Values"
                            && segs.len() > 2
                        {
                            out.insert(segs[2..].join("."));
                        }
                    }
                }

                // NEW: generic index base: . / binding / $var / field
                if function_name_of(&n, src).as_deref() == Some("index") {
                    if let Some(args) = arg_list(&n) {
                        let mut w = args.walk();
                        let mut named = args.named_children(&mut w);
                        if let Some(base) = named.next() {
                            // ... (existing code that computes `bases` and `keys`)
                            // gather string keys
                            let mut keys: Vec<String> = Vec::new();
                            let mut w2 = args.walk();
                            for ch in args.named_children(&mut w2) {
                                if ch.id() == base.id() {
                                    continue;
                                }
                                if matches!(
                                    ch.kind(),
                                    "interpreted_string_literal" | "raw_string_literal" | "string"
                                ) {
                                    let k = ch
                                        .utf8_text(src.as_bytes())
                                        .unwrap_or("")
                                        .trim()
                                        .trim_matches('"')
                                        .trim_matches('`')
                                        .to_string();
                                    if !k.is_empty() {
                                        keys.push(k);
                                    }
                                }
                            }

                            // resolve base -> list of base paths
                            let mut bases: Vec<String> = Vec::new();
                            match base.kind() {
                                "selector_expression" => {
                                    if let Some(segs) =
                                        helm_schema_template::values::parse_selector_expression(
                                            &base, src,
                                        )
                                    {
                                        if let Some(vp) =
                                            origin_to_value_path(&scope.resolve_selector(&segs))
                                        {
                                            bases.push(vp);
                                        }
                                    }
                                }
                                "dot" => {
                                    if let Some(vp) = origin_to_value_path(&scope.dot) {
                                        bases.push(vp);
                                    }
                                }
                                "variable" => {
                                    let raw = base.utf8_text(src.as_bytes()).unwrap_or("").trim();
                                    if raw == "$" {
                                        if let Some(vp) = origin_to_value_path(&scope.dollar) {
                                            bases.push(vp); // "" at root
                                        }
                                    } else {
                                        let name = raw.trim_start_matches('$');
                                        if let Some(set) = env.get(name) {
                                            bases.extend(set.iter().cloned());
                                        }
                                    }
                                }
                                "field" => {
                                    // Prefer explicit identifier nodes if present, otherwise use the full node text.
                                    let name = base
                                        .child_by_field_name("identifier")
                                        .or_else(|| base.child_by_field_name("field_identifier"))
                                        .and_then(|id| {
                                            id.utf8_text(src.as_bytes())
                                                .ok()
                                                .map(|s| s.trim().to_string())
                                        })
                                        .unwrap_or_else(|| {
                                            // Fallback: ".config" → "config"
                                            base.utf8_text(src.as_bytes())
                                                .unwrap_or("")
                                                .trim()
                                                .trim_start_matches('.')
                                                .to_string()
                                        });

                                    if let Some(bound) = scope.bindings.get(&name) {
                                        if let Some(vp) = origin_to_value_path(bound) {
                                            debug_assert!(!vp.is_empty());
                                            if !vp.is_empty() {
                                                bases.push(vp);
                                            }
                                        } else {
                                            // Binding exists but isn't a .Values selector: resolve relative to dot.
                                            let segs = vec![".".to_string(), name.clone()];
                                            if let Some(vp) =
                                                origin_to_value_path(&scope.resolve_selector(&segs))
                                            {
                                                bases.push(vp);
                                            }
                                        }
                                    } else {
                                        // No binding: resolve `.name` relative to current dot/bindings.
                                        let segs = vec![".".to_string(), name];
                                        if let Some(vp) =
                                            origin_to_value_path(&scope.resolve_selector(&segs))
                                        {
                                            debug_assert!(!vp.is_empty());
                                            if !vp.is_empty() {
                                                bases.push(vp);
                                            }
                                        }
                                    }
                                }

                                _ => {}
                            }

                            // and finally inserts only `{base}.{joined keys}` (never the bare base)
                            for mut b in bases {
                                if b == "." {
                                    b.clear();
                                }
                                if keys.is_empty() {
                                    // no-op: do not insert a bare base for index()
                                } else if b.is_empty() {
                                    if !keys.is_empty() && keys[0] == "Values" {
                                        out.insert(keys[1..].join("."));
                                    } else {
                                        out.insert(keys.join("."));
                                    }
                                } else {
                                    out.insert(format!("{b}.{}", keys.join(".")));
                                }
                            }
                        }
                    }
                    // NEW: fully handled; do not descend into children
                    continue;
                }
            }
            _ => {}
        }

        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }

    out
}

fn collect_values_following_includes(
    node: Node,
    src: &str,
    scope: &Scope,
    inline: &InlineState<'_>,
    guard: &mut ExpansionGuard,
    env: &Env,
) -> BTreeSet<String> {
    let mut acc = collect_values_with_scope(node, src, scope, env);

    // DFS: find nested include() calls and union their (scoped) values, recursively.
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "function_call" && is_include_call(n, src) {
            if let Ok((tpl, arg)) = parse_include_call(n, src) {
                if let Some(entry) = inline.define_index.get(&tpl) {
                    // Build callee scope from arg
                    let mut callee = Scope {
                        dot: scope.dot.clone(),
                        dollar: scope.dollar.clone(),
                        bindings: Default::default(),
                    };
                    match arg {
                        ArgExpr::Dot => callee.dot = scope.dot.clone(),
                        ArgExpr::Dollar => callee.dot = scope.dollar.clone(),
                        // ArgExpr::Selector(sel) => callee.dot = ExprOrigin::Selector(sel),
                        ArgExpr::Selector(sel) => callee.dot = scope.resolve_selector(&sel),
                        ArgExpr::Dict(kvs) => {
                            for (k, v) in kvs {
                                let origin = match v {
                                    ArgExpr::Dot => scope.dot.clone(),
                                    ArgExpr::Dollar => scope.dollar.clone(),
                                    // ArgExpr::Selector(s) => ExprOrigin::Selector(s),
                                    ArgExpr::Selector(s) => scope.resolve_selector(&s),
                                    _ => ExprOrigin::Opaque,
                                };
                                callee.bind_key(&k, origin);
                            }
                        }
                        _ => {}
                    }

                    if !guard.stack.iter().any(|s| s == &tpl) {
                        guard.stack.push(tpl.clone());
                        let body_src = &entry.source[entry.body_byte_range.clone()];
                        if let Some(parsed) = parse_gotmpl_document(body_src) {
                            // Recurse into callee body (and its nested includes)
                            let sub = collect_values_following_includes(
                                parsed.tree.root_node(),
                                body_src,
                                &callee,
                                inline,
                                guard,
                                env,
                            );
                            acc.extend(sub);
                        }
                        guard.stack.pop();
                    }
                }
            }
        }

        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }

    acc
}

#[derive(Default)]
struct InlineOut {
    buf: String,
    placeholders: Vec<crate::sanitize::Placeholder>,
    guards: Vec<ValueUse>,
    next_id: usize,
    env: Env, // $var -> set of .Values.* it refers to
}

fn looks_like_fragment(node: &Node, src: &str) -> bool {
    // true if the node itself or any function in its pipeline is include|toYaml
    let mut check = |n: &Node| -> bool {
        if n.kind() != "function_call" {
            return false;
        }
        if let Some(name) = function_name_of(n, src) {
            return name == "include" || name == "toYaml";
        }
        false
    };
    match node.kind() {
        "function_call" => check(node),
        "chained_pipeline" => {
            let mut w = node.walk();
            for ch in node.children(&mut w) {
                if ch.is_named() && ch.kind() == "function_call" && check(&ch) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

fn first_text_child_start(n: &Node) -> Option<usize> {
    let mut w = n.walk();
    for ch in n.children(&mut w) {
        if ch.is_named() && ch.kind() == "text" {
            return Some(ch.start_byte());
        }
    }
    None
}

fn is_guard_piece(node: &Node) -> bool {
    if let Some(parent) = node.parent() {
        if crate::sanitize::is_control_flow(parent.kind()) {
            if let Some(body_start) = first_text_child_start(&parent) {
                return node.end_byte() <= body_start;
            }
        }
    }
    false
}

fn current_slot_in_buf(buf: &str) -> Slot {
    // Look at the current and the previous non-empty line
    let mut it = buf.rsplit('\n');

    let cur = it.next().unwrap_or(""); // current (possibly empty) line
    let cur_trim = cur.trim_start();

    // If current line already starts with "- " → list item context
    if cur_trim.starts_with("- ") {
        return Slot::SequenceItem;
    }

    let prev_non_empty = it.find(|l| !l.trim().is_empty()).unwrap_or("");

    let prev_trim = prev_non_empty.trim_end();

    // Definitely mapping value if the previous non-empty line ends with ':'
    if prev_trim.ends_with(':') {
        return Slot::MappingValue;
    }

    // NEW: stay in mapping if the previous non-empty line is a placeholder mapping entry
    // e.g.   "  "__TSG_PLACEHOLDER_3__": 0"
    if prev_trim.contains("__TSG_PLACEHOLDER_") && prev_trim.contains("\": 0") {
        return Slot::MappingValue;
    }

    Slot::Plain
}

fn write_fragment_placeholder_line(buf: &mut String, id: usize, slot: Slot) {
    use std::fmt::Write as _;
    match slot {
        Slot::MappingValue => {
            let _ = writeln!(buf, "\"__TSG_PLACEHOLDER_{id}__\": 0");
        }
        Slot::SequenceItem => {
            let _ = writeln!(buf, "\"__TSG_PLACEHOLDER_{id}__\"");
        }
        Slot::Plain => {
            let _ = writeln!(buf, "\"__TSG_PLACEHOLDER_{id}__\"");
        }
    }
}

fn write_placeholder(
    out: &mut InlineOut,
    node: Node,
    src: &str,
    values: BTreeSet<String>,
    is_fragment_output: bool,
) {
    let id = out.next_id;
    out.next_id += 1;

    fn last_non_empty_line(buf: &str) -> &str {
        for line in buf.lines().rev() {
            if !line.trim().is_empty() {
                return line;
            }
        }
        ""
    }

    if is_fragment_output {
        let slot = current_slot_in_buf(&out.buf);
        eprintln!(
            "[frag-ph] id={id} slot={:?} after='{}'",
            slot,
            last_non_empty_line(&out.buf).trim_end()
        );
        write_fragment_placeholder_line(&mut out.buf, id, slot);
    } else {
        out.buf.push('"');
        out.buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
        out.buf.push('"');
    }

    out.placeholders.push(Placeholder {
        id,
        role: Role::Unknown,
        action_span: node.byte_range(),
        values: values.into_iter().collect(),
        is_fragment_output,
    });
}

// Walk a template/define body and inline includes when appropriate.
fn inline_emit_tree(
    tree: &tree_sitter::Tree,
    src: &str,
    scope: &Scope,
    inline: &InlineState<'_>,
    guard: &mut ExpansionGuard,
    out: &mut InlineOut,
) -> Result<(), Error> {
    fn walk_container(
        container: Node,
        src: &str,
        scope: &Scope,
        inline: &InlineState<'_>,
        guard: &mut ExpansionGuard,
        out: &mut InlineOut,
    ) -> Result<(), Error> {
        let mut c = container.walk();

        // If this container is a control-flow node, pre-compute the body scope with adjusted dot
        let container_is_cf = crate::sanitize::is_control_flow(container.kind());
        let mut guards_emitted_for_container = false;

        // Only `with` and `range` rebind `.`. `if` does NOT rebind.
        let rebind_dot_here = matches!(container.kind(), "with_action" | "range_action");

        let body_scope_for_cf: Option<Scope> = if container_is_cf && rebind_dot_here {
            let mut s = scope.clone();

            // Choose the *deepest* selector from the guard (e.g. prefer
            // ".config.storageClassName" over ".config").
            let mut best: Option<Vec<String>> = None;
            let mut best_depth: usize = 0;
            for o in guard_selectors_for_action(container, src, scope) {
                if let ExprOrigin::Selector(v) = o {
                    // depth = segments after "Values"
                    let depth = if !v.is_empty() && v[0] == "Values" {
                        v.len().saturating_sub(1)
                    } else {
                        v.len()
                    };
                    if depth > best_depth {
                        best_depth = depth;
                        best = Some(v);
                    }
                }
            }
            if let Some(sel) = best {
                s.dot = ExprOrigin::Selector(sel);
            }

            // if let Some(sel) = guard_selectors_for_action(container, src, scope)
            //     .into_iter()
            //     .find_map(|o| match o {
            //         ExprOrigin::Selector(v) => Some(v),
            //         _ => None,
            //     })
            // {
            //     s.dot = ExprOrigin::Selector(sel);
            // }

            Some(s)
        } else {
            None
        };

        for ch in container.children(&mut c) {
            if !ch.is_named() {
                continue;
            }

            // Determine which scope to use for THIS child
            // - Guard pieces: original `scope`
            // - Body pieces of a control-flow container: `body_scope_for_cf`
            let use_scope = if container_is_cf && !is_guard_piece(&ch) {
                body_scope_for_cf.as_ref().unwrap_or(scope)
            } else {
                scope
            };

            // raw text: keep your define suppression behavior
            if ch.kind() == "text" {
                let parent_is_define = container.kind() == "define_action";
                if !parent_is_define {
                    out.buf.push_str(&src[ch.byte_range()]);
                }
                continue;
            }

            // GUARD: record and continue (must use the *original* scope here)
            if is_guard_piece(&ch) {
                if container_is_cf {
                    if !guards_emitted_for_container {
                        let origins = guard_selectors_for_action(container, src, scope);

                        let origins = guard_selectors_for_action(container, src, scope);

                        // Range vs With:
                        //  - range: emit ONLY the exact guard key; and if it's just '.', suppress entirely
                        //  - with  : keep prefix expansion (e.g. "config.accessModes" => "config", "config.accessModes")
                        if container.kind() == "range_action" {
                            let dot_vp = origin_to_value_path(&scope.dot);
                            let mut seen = std::collections::BTreeSet::new();
                            for o in origins {
                                if let Some(vp) = origin_to_value_path(&o) {
                                    if vp.is_empty() {
                                        continue;
                                    }
                                    // Suppress guard for `range .` (vp equals current dot)
                                    if dot_vp.as_ref() == Some(&vp) {
                                        continue;
                                    }
                                    if seen.insert(vp.clone()) {
                                        out.guards.push(ValueUse {
                                            value_path: vp,
                                            role: Role::Guard,
                                            action_span: ch.byte_range(),
                                            yaml_path: None,
                                        });
                                    }
                                }
                            }
                        } else {
                            // WITH/IF: emit every non-empty dot-separated prefix only once
                            let mut seen = std::collections::BTreeSet::new();
                            for o in origins {
                                if let Some(vp) = origin_to_value_path(&o) {
                                    if vp.is_empty() {
                                        continue;
                                    }
                                    // split "a.b.c" -> ["a", "a.b", "a.b.c"]
                                    let mut acc = String::new();
                                    for (i, seg) in vp.split('.').enumerate() {
                                        if i == 0 {
                                            acc.push_str(seg);
                                        } else {
                                            acc.push('.');
                                            acc.push_str(seg);
                                        }
                                        if seen.insert(acc.clone()) {
                                            out.guards.push(ValueUse {
                                                value_path: acc.clone(),
                                                role: Role::Guard,
                                                action_span: ch.byte_range(),
                                                yaml_path: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        // // NEW: emit every non-empty dot-separated prefix only once
                        // let mut seen = std::collections::BTreeSet::new();
                        // for o in origins {
                        //     if let Some(vp) = origin_to_value_path(&o) {
                        //         if vp.is_empty() {
                        //             continue;
                        //         }
                        //         // split "a.b.c" -> ["a", "a.b", "a.b.c"]
                        //         let mut acc = String::new();
                        //         for (i, seg) in vp.split('.').enumerate() {
                        //             if i == 0 {
                        //                 acc.push_str(seg);
                        //             } else {
                        //                 acc.push('.');
                        //                 acc.push_str(seg);
                        //             }
                        //             if seen.insert(acc.clone()) {
                        //                 out.guards.push(ValueUse {
                        //                     value_path: acc.clone(),
                        //                     role: Role::Guard,
                        //                     action_span: ch.byte_range(),
                        //                     yaml_path: None,
                        //                 });
                        //             }
                        //         }
                        //     }
                        // }
                        guards_emitted_for_container = true;
                    }
                    // IMPORTANT: short-circuit so we don't fall back and duplicate
                    continue;
                } else {
                    // Non-control-flow guard-ish piece (rare)
                    let vals = collect_values_with_scope(ch, src, scope, &out.env);
                    for v in vals {
                        if !v.is_empty() {
                            out.guards.push(ValueUse {
                                value_path: v,
                                role: Role::Guard,
                                action_span: ch.byte_range(),
                                yaml_path: None,
                            });
                        }
                    }
                    continue;
                }

                // if container_is_cf {
                //     if !guards_emitted_for_container {
                //         let origins = guard_selectors_for_action(container, src, scope);
                //         let mut seen = std::collections::BTreeSet::new();
                //         for o in origins {
                //             if let Some(vp) = origin_to_value_path(&o) {
                //                 if !vp.is_empty() && seen.insert(vp.clone()) {
                //                     out.guards.push(ValueUse {
                //                         value_path: vp,
                //                         role: Role::Guard,
                //                         action_span: ch.byte_range(),
                //                         yaml_path: None,
                //                     });
                //                 }
                //             }
                //         }
                //         guards_emitted_for_container = true;
                //     }
                //     // IMPORTANT: short-circuit so we don't fall back and duplicate
                //     continue;
                // } else {
                //     // Non-control-flow guard-ish piece (rare): fallback but filter empties
                //     let vals = collect_values_with_scope(ch, src, scope, &out.env);
                //     for v in vals {
                //         if !v.is_empty() {
                //             out.guards.push(ValueUse {
                //                 value_path: v,
                //                 role: Role::Guard,
                //                 action_span: ch.byte_range(),
                //                 yaml_path: None,
                //             });
                //         }
                //     }
                //     continue;
                // }

                // // We'll emit guards only once per container to avoid duplicates
                // // (e.g., dot inside the selector_expression).
                // static EMITTED_KEY: &str = "__guards_emitted__";
                // // Attach a tiny flag to the node via a local map we keep on the stack of this function call.
                // // We can emulate it by capturing a boolean per container invocation.
                // // Put this boolean near the top of walk_container:
                // if container_is_cf && !guards_emitted_for_container {
                //     let origins = guard_selectors_for_action(container, src, scope);
                //     let mut seen = std::collections::BTreeSet::new();
                //     for o in origins {
                //         if let Some(vp) = origin_to_value_path(&o) {
                //             if !vp.is_empty() && seen.insert(vp.clone()) {
                //                 out.guards.push(ValueUse {
                //                     value_path: vp,
                //                     role: Role::Guard,
                //                     action_span: ch.byte_range(),
                //                     yaml_path: None,
                //                 });
                //             }
                //         }
                //     }
                //     guards_emitted_for_container = true;
                // } else {
                //     // Fallback (non-control-flow): keep old behavior but filter empties
                //     let vals = collect_values_with_scope(ch, src, scope, &out.env);
                //     for v in vals {
                //         if !v.is_empty() {
                //             out.guards.push(ValueUse {
                //                 value_path: v,
                //                 role: Role::Guard,
                //                 action_span: ch.byte_range(),
                //                 yaml_path: None,
                //             });
                //         }
                //     }
                // }
                // continue;

                // // Suppress the extra Guard for `range .`:
                // // In a range_action whose subject is a plain dot, we don't want
                // // to count another Guard (the `with` already guarded the data).
                // if container.kind() == "range_action" && ch.kind() == "dot" {
                //     // still traverse everything else later, but don't record a Guard
                //     continue;
                // }
                //
                // let vals = collect_values_with_scope(ch, src, scope, &out.env);
                // for v in vals {
                //     out.guards.push(ValueUse {
                //         value_path: v,
                //         role: Role::Guard,
                //         action_span: ch.byte_range(),
                //         yaml_path: None,
                //     });
                // }
                // continue;
            }

            // Nested containers / control flow
            if crate::sanitize::is_container(ch.kind())
                || crate::sanitize::is_control_flow(ch.kind())
            {
                // Recurse using the child-specific scope
                walk_container(ch, src, use_scope, inline, guard, out)?;
                continue;
            }

            // Output expressions (+ include handling)
            if matches!(
                ch.kind(),
                "selector_expression"
                    | "function_call"
                    | "chained_pipeline"
                    | "variable"
                    | "dot"
                    | "field"
            ) {
                if ch.kind() == "function_call" && is_include_call(ch, src) {
                    // Classify site
                    let ctx = classify_action(src, &ch.byte_range());
                    let (tpl, arg) = parse_include_call(ch, src)?;

                    if guard.stack.iter().any(|n| n == &tpl) {
                        continue;
                    }
                    let entry = inline
                        .define_index
                        .get(&tpl)
                        .ok_or_else(|| IncludeError::UnknownDefine(tpl.clone()))?;

                    // Build callee scope FROM THE CHILD-SPECIFIC SCOPE
                    let mut callee = Scope {
                        dot: use_scope.dot.clone(),
                        dollar: use_scope.dollar.clone(),
                        bindings: Default::default(),
                    };
                    match arg {
                        ArgExpr::Dot => callee.dot = use_scope.dot.clone(),
                        ArgExpr::Dollar => callee.dot = use_scope.dollar.clone(),
                        // ArgExpr::Selector(sel) => callee.dot = ExprOrigin::Selector(sel),
                        ArgExpr::Selector(sel) => callee.dot = use_scope.resolve_selector(&sel),
                        ArgExpr::Dict(kvs) => {
                            for (k, v) in kvs {
                                let origin = match v {
                                    ArgExpr::Dot => use_scope.dot.clone(),
                                    ArgExpr::Dollar => use_scope.dollar.clone(),
                                    // ArgExpr::Selector(s) => ExprOrigin::Selector(s),
                                    ArgExpr::Selector(s) => use_scope.resolve_selector(&s),
                                    _ => ExprOrigin::Opaque,
                                };
                                callee.bind_key(&k, origin);
                            }
                        }
                        _ => {}
                    }

                    let body_src = &entry.source[entry.body_byte_range.clone()];
                    let body = parse_gotmpl_document(body_src).ok_or(Error::ParseTemplate)?;

                    match ctx {
                        Role::ScalarValue => {
                            // 1) Walk callee to learn its local $var bindings and guards
                            let mut tmp = InlineOut::default();
                            inline_emit_tree(
                                &body.tree, body_src, &callee, inline, guard, &mut tmp,
                            )?;

                            // 2) Now collect values with the callee’s env so $cfg.name / $name resolve
                            let vals = collect_values_following_includes(
                                body.tree.root_node(),
                                body_src,
                                &callee,
                                inline,
                                guard,
                                &tmp.env,
                            );

                            // 3) Emit one scalar placeholder at the include site
                            write_placeholder(
                                out, ch, src, vals, /*is_fragment_output=*/ false,
                            );

                            // 4) Keep callee guards
                            out.guards.extend(tmp.guards.into_iter());
                        }

                        _ => {
                            guard.stack.push(tpl.clone());
                            inline_emit_tree(&body.tree, body_src, &callee, inline, guard, out)?;
                            guard.stack.pop();
                        }
                    }
                    continue;
                }

                // Non-include output
                let is_frag = looks_like_fragment(&ch, src);

                // Prefer using the pipeline "head" (covers bare `field`, `selector_expression`,
                // or the head of a `chained_pipeline`). If that yields exactly one key,
                // use it; otherwise fall back to the full include-aware collector.
                let vals: BTreeSet<String> = if let Some(head) = pipeline_head(ch) {
                    let head_keys =
                        collect_values_with_scope(head, src, use_scope, &Env::default());
                    if head_keys.len() == 1 {
                        head_keys
                    } else {
                        collect_values_following_includes(
                            ch, src, use_scope, inline, guard, &out.env,
                        )
                    }
                } else {
                    collect_values_following_includes(ch, src, use_scope, inline, guard, &out.env)
                };

                write_placeholder(out, ch, src, vals, is_frag);
                continue;

                // let vals =
                //     collect_values_following_includes(ch, src, use_scope, inline, guard, &out.env);
                // write_placeholder(out, ch, src, vals, is_frag);
                // continue;
            }

            // Assignments ($x := ...): use the child-specific scope too
            if crate::sanitize::is_assignment_kind(ch.kind()) {
                let vals =
                    collect_values_following_includes(ch, src, use_scope, inline, guard, &out.env);
                let id = out.next_id;
                out.next_id += 1;
                out.placeholders.push(Placeholder {
                    id,
                    role: Role::Fragment,
                    action_span: ch.byte_range(),
                    values: vals.iter().cloned().collect(),
                    is_fragment_output: true,
                });
                let mut vc = ch.walk();
                for sub in ch.children(&mut vc) {
                    if sub.is_named() && sub.kind() == "variable" {
                        if let Some(name) = variable_ident_of(&sub, src) {
                            out.env.insert(name, vals.clone());
                        }
                        break;
                    }
                }
                continue;
            }
        }
        Ok(())
    }

    walk_container(tree.root_node(), src, scope, inline, guard, out)
}

#[cfg(test)]
mod scope_resolving {
    use super::*;
    use color_eyre::eyre::{self, OptionExt, WrapErr};
    use helm_schema_template::parse::parse_gotmpl_document;
    use indoc::indoc;
    use test_util::prelude::*;

    // convenience: turn ExprOrigin into an Option<String> like "a.b.c" (only for .Values.*)
    fn vp(o: &ExprOrigin) -> Option<String> {
        super::origin_to_value_path(o)
    }

    // convenience: build Vec<String> segments
    fn segs(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    fn root_scope() -> Scope {
        Scope {
            dot: ExprOrigin::Selector(vec!["Values".into()]),
            dollar: ExprOrigin::Selector(vec!["Values".into()]),
            bindings: Default::default(),
        }
    }

    #[test]
    fn resolves_dot_values_as_absolute_at_top_level() {
        Builder::default().build();
        let s = root_scope();
        let got = s.resolve_selector(&segs(&[".", "Values", "foo", "bar"]));
        assert_that!(vp(&got), some(eq("foo.bar")));
    }

    #[test]
    fn resolves_dot_values_as_absolute_even_when_dot_is_nested() {
        Builder::default().build();
        let mut s = root_scope();
        s.dot = ExprOrigin::Selector(vec!["Values".into(), "persistentVolumeClaims".into()]);
        let got = s.resolve_selector(&segs(&[".", "Values", "fullnameOverride"]));
        assert_that!(vp(&got), some(eq("fullnameOverride"))); // absolute, not pvc.fullnameOverride
    }

    #[test]
    fn resolves_dollar_values_as_absolute() {
        Builder::default().build();
        let s = root_scope();
        let got = s.resolve_selector(&segs(&["$", "Values", "x", "y"]));
        assert_that!(vp(&got), some(eq("x.y")));
    }

    #[test]
    fn resolves_plain_values_chain_as_absolute() {
        Builder::default().build();
        let s = root_scope();
        let got = s.resolve_selector(&segs(&["Values", "a", "b"]));
        assert_that!(vp(&got), some(eq("a.b")));
    }

    #[test]
    fn resolves_relative_field_against_current_dot_selector() {
        Builder::default().build();
        let mut s = root_scope();
        s.dot = ExprOrigin::Selector(vec!["Values".into(), "list".into()]);
        let got = s.resolve_selector(&segs(&[".", "name"]));
        assert_that!(vp(&got), some(eq("list.name")));
    }

    #[test]
    fn resolves_binding_from_dict_key_like_config() {
        Builder::default().build();
        let mut s = root_scope();
        s.bind_key(
            "config",
            ExprOrigin::Selector(vec!["Values".into(), "persistentVolumeClaims".into()]),
        );
        let got = s.resolve_selector(&segs(&[".", "config", "size"]));
        assert_that!(vp(&got), some(eq("persistentVolumeClaims.size")));
    }

    #[test]
    fn resolves_bare_binding_identifier_without_leading_dot() {
        Builder::default().build();
        let mut s = root_scope();
        s.bind_key(
            "cfg",
            ExprOrigin::Selector(vec!["Values".into(), "ingress".into()]),
        );
        let got = s.resolve_selector(&segs(&["cfg", "pathType"]));
        assert_that!(vp(&got), some(eq("ingress.pathType")));
    }

    #[test]
    fn unknown_selector_is_opaque_and_does_not_produce_value_path() {
        Builder::default().build();
        let mut s = root_scope();
        s.dot = ExprOrigin::Opaque;
        let got = s.resolve_selector(&segs(&["whatever", "field"]));
        assert_that!(vp(&got), none());
    }

    #[test]
    fn parse_values_field_call_variant_values_dot_foo() -> eyre::Result<()> {
        Builder::default().build();
        // {{ Values .commonLabels }}
        let src = r#"{{ Values .commonLabels }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let mut target = None;
        let mut stack = vec![parsed.tree.root_node()];
        while let Some(n) = stack.pop() {
            if n.kind() == "function_call" {
                target = Some(n);
                break;
            }
            let mut w = n.walk();
            for ch in n.children(&mut w) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }
        let node = target.ok_or_eyre("missing function_call")?;
        let got = super::parse_values_field_call(&node, src);
        assert_that!(got.as_deref(), some(eq("commonLabels")));
        Ok(())
    }

    #[test]
    fn collect_index_over_dollar_values_base() -> eyre::Result<()> {
        Builder::default().build();
        // {{ index $ "Values" "foo" "bar" }}
        let src = r#"{{ index $ "Values" "foo" "bar" }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let mut node = parsed.tree.root_node();
        // find the function_call
        {
            let mut stack = vec![node];
            while let Some(n) = stack.pop() {
                if n.kind() == "function_call" {
                    node = n;
                    break;
                }
                let mut w = n.walk();
                for ch in n.children(&mut w) {
                    if ch.is_named() {
                        stack.push(ch);
                    }
                }
            }
        }

        let scope = root_scope();
        let env: super::Env = Default::default();
        let vals = super::collect_values_with_scope(node, src, &scope, &env);
        assert_that!(
            &vals.iter().collect::<Vec<_>>(),
            unordered_elements_are![eq(&"foo.bar")]
        );
        Ok(())
    }

    #[test]
    fn collect_index_over_dot_binding_base() -> eyre::Result<()> {
        Builder::default().build();
        // {{ index .config "name" }}
        let src = r#"{{ index .config "name" }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let mut node = parsed.tree.root_node();
        {
            let mut stack = vec![node];
            while let Some(n) = stack.pop() {
                if n.kind() == "function_call" {
                    node = n;
                    break;
                }
                let mut w = n.walk();
                for ch in n.children(&mut w) {
                    if ch.is_named() {
                        stack.push(ch);
                    }
                }
            }
        }

        let mut scope = root_scope();
        scope.bind_key(
            "config",
            ExprOrigin::Selector(vec!["Values".into(), "persistentVolumeClaims".into()]),
        );
        let env: super::Env = Default::default();

        let vals = super::collect_values_with_scope(node, src, &scope, &env);
        assert_that!(
            &vals.iter().collect::<Vec<_>>(),
            unordered_elements_are![eq(&"persistentVolumeClaims.name"),]
        );
        Ok(())
    }

    #[test]
    fn collect_selector_expands_variable_env_dollar_cfg() -> eyre::Result<()> {
        Builder::default().build();
        // {{ $cfg.name }}
        let src = r#"{{ $cfg.name }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let mut node = parsed.tree.root_node();
        {
            let mut stack = vec![node];
            while let Some(n) = stack.pop() {
                if n.kind() == "selector_expression" {
                    node = n;
                    break;
                }
                let mut w = n.walk();
                for ch in n.children(&mut w) {
                    if ch.is_named() {
                        stack.push(ch);
                    }
                }
            }
        }

        let scope = root_scope();
        let mut env: super::Env = Default::default();
        let mut set = BTreeSet::new();
        set.insert("persistentVolumeClaims".to_string());
        env.insert("cfg".to_string(), set);

        let vals = super::collect_values_with_scope(node, src, &scope, &env);
        assert_that!(
            &vals.iter().collect::<Vec<_>>(),
            unordered_elements_are![eq(&"persistentVolumeClaims.name"),]
        );
        Ok(())
    }

    #[test]
    fn guard_with_sets_body_dot_to_values_key() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            {{- with .Values.ingress }}
            {{ .path }}
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let mut with_node = None;
        let mut stack = vec![parsed.tree.root_node()];
        while let Some(n) = stack.pop() {
            if n.kind() == "with_action" {
                with_node = Some(n);
                break;
            }
            let mut w = n.walk();
            for ch in n.children(&mut w) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }
        let with_node = with_node.ok_or_eyre("missing with_action")?;
        let scope = root_scope();
        let origins = super::guard_selectors_for_action(with_node, src, &scope);
        let base = origins
            .into_iter()
            .find_map(|o| match o {
                ExprOrigin::Selector(v) => Some(ExprOrigin::Selector(v)),
                _ => None,
            })
            .ok_or_eyre("missing selector")?;
        assert_that!(vp(&base), some(eq("ingress"))); // `. -> .Values.ingress` for body

        // And ensure `.path` under that body resolves properly.
        let mut body_scope = root_scope();
        body_scope.dot = base;
        let got = body_scope.resolve_selector(&segs(&[".", "path"]));
        assert_that!(vp(&got), some(eq("ingress.path")));
        Ok(())
    }

    #[test]
    fn guard_range_sets_body_dot_to_values_key() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            {{- range .Values.persistentVolumeClaims }}
            {{ .size }}
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;

        let mut range_node = None;
        let mut stack = vec![parsed.tree.root_node()];
        while let Some(n) = stack.pop() {
            if n.kind() == "range_action" {
                range_node = Some(n);
                break;
            }
            let mut w = n.walk();
            for ch in n.children(&mut w) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }
        let range_node = range_node.ok_or_eyre("missing range_action")?;
        let scope = root_scope();
        let origins = super::guard_selectors_for_action(range_node, src, &scope);
        let base = origins
            .into_iter()
            .find_map(|o| match o {
                ExprOrigin::Selector(v) => Some(ExprOrigin::Selector(v)),
                _ => None,
            })
            .ok_or_eyre("missing selector")?;
        assert_that!(vp(&base), some(eq("persistentVolumeClaims")));

        let mut body_scope = root_scope();
        body_scope.dot = base;
        let got = body_scope.resolve_selector(&segs(&[".", "size"]));
        assert_that!(vp(&got), some(eq("persistentVolumeClaims.size")));
        Ok(())
    }

    #[test]
    fn with_scalar_dot_emits_value_use() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            spec:
              {{- with .Values.persistentVolumeClaims.storageClassName }}
              storageClassName: {{ . }}
              {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?; // whatever helper you use
        dbg!(&uses);

        assert_that!(
            &uses,
            unordered_elements_are![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("persistentVolumeClaims"),
                    yaml_path: none(),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("persistentVolumeClaims.storageClassName"),
                    yaml_path: none(),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("persistentVolumeClaims.storageClassName"),
                    yaml_path: some(displays_as(eq("spec.storageClassName"))),
                    ..
                }),
            ]
        );

        Ok(())
    }

    #[test]
    fn with_list_range_over_dot_suppresses_extra_guard() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            spec:
              {{- with .Values.config.accessModes }}
              accessModes:
                {{- range . }}
                - {{ . }}
                {{- end }}
              {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        dbg!(&uses);

        // Expect exactly:
        // - Guard on `config`
        // - Guard on `config.accessModes` (from the `with` condition)
        // - One scalar value use for the list items (no extra Guard from `range .`)
        assert_that!(
            &uses,
            unordered_elements_are![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("config"),
                    yaml_path: none(),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("config.accessModes"),
                    yaml_path: none(),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("config.accessModes"),
                    .. // yaml_path will be like spec.accessModes[0] (or [*])
                }),
            ]
        );

        Ok(())
    }

    #[test]
    fn with_list_range_over_nested_key_emits_guard() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            spec:
              {{- with .Values.config }}
              accessModes:
                {{- range .accessModes }}
                - {{ . }}
                {{- end }}
              {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        dbg!(&uses);

        // Here the `range .accessModes` should emit a Guard for `config.accessModes`.
        // (Still exactly 3 entries overall.)
        assert_that!(
            &uses,
            unordered_elements_are![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("config"),
                    yaml_path: none(),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("config.accessModes"),
                    yaml_path: none(),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("config.accessModes"),
                    .. // yaml_path will be like spec.accessModes[0] (or [*])
                }),
            ]
        );

        Ok(())
    }

    #[test]
    fn include_dict_argument_binds_ctx_and_config() -> eyre::Result<()> {
        Builder::default().build();
        // {{ include "common.pvc" (dict "ctx" $ "config" .Values.persistentVolumeClaims) }}
        let src =
            r#"{{ include "common.pvc" (dict "ctx" $ "config" .Values.persistentVolumeClaims) }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;

        let mut call = None;
        let mut stack = vec![parsed.tree.root_node()];
        while let Some(n) = stack.pop() {
            if n.kind() == "function_call" {
                call = Some(n);
                break;
            }
            let mut w = n.walk();
            for ch in n.children(&mut w) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }
        let call = call.ok_or_eyre("missing function_call")?;
        let (_name, arg) =
            super::parse_include_call(call, src).wrap_err("failed to parse include")?;

        let caller = root_scope();
        let mut callee = Scope {
            dot: caller.dot.clone(),
            dollar: caller.dollar.clone(),
            bindings: Default::default(),
        };

        match arg {
            ArgExpr::Dict(kvs) => {
                for (k, v) in kvs {
                    let origin = match v {
                        ArgExpr::Dot => caller.dot.clone(),
                        ArgExpr::Dollar => caller.dollar.clone(),
                        ArgExpr::Selector(s) => caller.resolve_selector(&s),
                        _ => ExprOrigin::Opaque,
                    };
                    callee.bind_key(&k, origin);
                }
            }
            _ => eyre::bail!("expected dict arg"),
        }

        // ctx should bind to root ($/.)
        let ctx = callee.bindings.get("ctx").ok_or_eyre("missing ctx")?;
        assert_that!(vp(ctx), some(eq("")));
        // config should bind to .Values.persistentVolumeClaims
        let cfg = callee.bindings.get("config").ok_or_eyre("config")?;
        assert_that!(vp(cfg), some(eq("persistentVolumeClaims")));

        // And resolving `.config.name` inside callee should hit PVC.name
        let got = callee.resolve_selector(&segs(&[".", "config", "name"]));
        assert_that!(vp(&got), some(eq("persistentVolumeClaims.name")));
        Ok(())
    }

    #[test]
    fn dollar_chart_does_not_produce_values_key_when_dollar_is_opaque() {
        Builder::default().build();
        // This asserts our analyzer ignores $.Chart.* (non-Values) when $ isn't modeled as .Values
        let mut s = root_scope();
        s.dollar = ExprOrigin::Opaque; // model `$` as non-Values for this unit test
        let got = s.resolve_selector(&segs(&["$", "Chart", "Name"]));
        assert_that!(vp(&got), none());
    }
}

#[cfg(test)]
mod include_call {
    use crate::analyze::ArgExpr;
    use color_eyre::eyre::{self, OptionExt, WrapErr};
    use helm_schema_template::parse::{Parsed, parse_gotmpl_document};
    use test_util::prelude::*;

    // Helper: get the function_call node directly under the template root.
    fn first_call<'tree>(parsed: &'tree Parsed) -> eyre::Result<tree_sitter::Node<'tree>> {
        parsed
            .tree
            .root_node()
            .child(1)
            .ok_or_eyre("missing function_call")
    }

    #[test]
    fn selector_handles_dot_ctx_and_values() -> eyre::Result<()> {
        Builder::default().build();

        // include "x" .ctx
        let src = r#"{{ include "x" .ctx }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        dbg!(&name, &args);
        assert_that!(&args, eq(&ArgExpr::selector([".", "ctx"])));

        // include "x" .Values
        let src = r#"{{ include "x" .Values }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        dbg!(&name, &args);
        assert_that!(&args, eq(&ArgExpr::selector([".", "Values"])));
        Ok(())
    }

    #[test]
    fn dot_and_dollar_arguments() -> eyre::Result<()> {
        Builder::default().build();

        // include "x" .
        let src = r#"{{ include "x" . }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(&args, eq(&ArgExpr::Dot));

        // include "x" $
        let src = r#"{{ include "x" $ }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(&args, eq(&ArgExpr::Dollar));

        Ok(())
    }

    #[test]
    fn selector_handles_nested_values_chains() -> eyre::Result<()> {
        Builder::default().build();

        // include "x" .Values.persistentVolumeClaims
        let src = r#"{{ include "x" .Values.persistentVolumeClaims }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(
            &args,
            eq(&ArgExpr::selector([
                ".",
                "Values",
                "persistentVolumeClaims"
            ]))
        );

        // include "x" .Values.foo.bar.baz
        let src = r#"{{ include "x" .Values.foo.bar.baz }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(
            &args,
            eq(&ArgExpr::selector([".", "Values", "foo", "bar", "baz"]))
        );

        Ok(())
    }

    #[test]
    fn dict_mixed_shapes_ctx_config_value_self() -> eyre::Result<()> {
        Builder::default().build();

        // include "x" (dict "ctx" $ "config" .Values.persistentVolumeClaims "value" $annotations "self" .)
        let src = r#"{{ include "x" (dict "ctx" $ "config" .Values.persistentVolumeClaims "value" $annotations "self" .) }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(
            &args,
            eq(&ArgExpr::Dict(vec![
                ("ctx".to_string(), ArgExpr::Dollar),
                (
                    "config".to_string(),
                    ArgExpr::selector([".", "Values", "persistentVolumeClaims"])
                ),
                // $annotations is NOT the bare '$' and should be opaque at parse time
                ("value".to_string(), ArgExpr::Opaque),
                ("self".to_string(), ArgExpr::Dot),
            ]))
        );

        Ok(())
    }

    #[test]
    fn dict_allows_field_variant_dot_ctx() -> eyre::Result<()> {
        Builder::default().build();

        // include "x" (dict "k" .ctx)
        let src = r#"{{ include "x" (dict "k" .ctx) }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(
            &args,
            eq(&ArgExpr::Dict(vec![(
                "k".to_string(),
                ArgExpr::selector([".", "ctx"])
            )]))
        );

        Ok(())
    }

    #[test]
    fn parenthesized_selector_is_handled() -> eyre::Result<()> {
        Builder::default().build();

        // include "x" (.Values)
        let src = r#"{{ include "x" (.Values) }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(&args, eq(&ArgExpr::selector([".", "Values"])));

        // include "x" (.Values.foo)
        let src = r#"{{ include "x" (.Values.foo) }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(&args, eq(&ArgExpr::selector([".", "Values", "foo"])));

        Ok(())
    }

    #[test]
    fn non_root_variable_argument_is_opaque() -> eyre::Result<()> {
        Builder::default().build();

        // include "x" $cfg   → not just '$' → opaque
        let src = r#"{{ include "x" $cfg }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(&args, eq(&ArgExpr::Opaque));

        Ok(())
    }

    #[test]
    fn dict_with_spacing_is_robust() -> eyre::Result<()> {
        Builder::default().build();

        let src = r#"{{ include "x" (dict  "a"   .Values.foo   "b"   $   "c"   .ctx ) }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let (_name, args) = super::parse_include_call(first_call(&parsed)?, src)?;
        assert_that!(
            &args,
            eq(&ArgExpr::Dict(vec![
                ("a".to_string(), ArgExpr::selector([".", "Values", "foo"])),
                ("b".to_string(), ArgExpr::Dollar),
                ("c".to_string(), ArgExpr::selector([".", "ctx"]))
            ]))
        );

        Ok(())
    }
}

#[cfg(test)]
mod sprig_and_pipelines {
    use super::*;
    use crate::analyze::{DefineIndex, Role, ValueUse};
    use color_eyre::eyre::{self, OptionExt};
    use indoc::indoc;
    use test_util::prelude::*;

    #[test]
    fn quote_keeps_scalar_mapping() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            spec:
              image: {{ quote .Values.image }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;

        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("image"),
                yaml_path: some(displays_as(eq("spec.image"))),
                ..
            }))
        );
        Ok(())
    }

    #[test]
    fn upper_and_b64enc_do_not_muddle_types() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            metadata:
              name: {{ upper .Values.name }}
            data:
              pass: {{ b64enc .Values.password }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            all![
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("name"),
                    yaml_path: some(displays_as(eq("metadata.name"))),
                    ..
                })),
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("password"),
                    yaml_path: some(displays_as(eq("data.pass"))),
                    ..
                }))
            ]
        );
        Ok(())
    }

    #[test]
    fn chained_functions_preserve_value_origin() -> eyre::Result<()> {
        Builder::default().build();
        // {{ default "nginx" (.Values.image | trim | lower) }}
        let src = indoc! {r#"
            spec:
              image: {{ default "nginx" (.Values.image | trim | lower) }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        // We at least need to see `.Values.image` as the scalar source for spec.image
        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("image"),
                yaml_path: some(displays_as(eq("spec.image"))),
                ..
            }))
        );
        Ok(())
    }
}

#[cfg(test)]
mod defaults_coalesce_ternary_required {
    use super::*;
    use crate::analyze::{DefineIndex, Role, ValueUse};
    use color_eyre::eyre::{self, OptionExt};
    use indoc::indoc;
    use test_util::prelude::*;

    #[test]
    fn default_propagates_value_path() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            spec:
              imagePullPolicy: {{ default "IfNotPresent" .Values.pullPolicy }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("pullPolicy"),
                yaml_path: some(displays_as(eq("spec.imagePullPolicy"))),
                ..
            }))
        );
        Ok(())
    }

    #[test]
    fn coalesce_registers_all_candidate_paths() -> eyre::Result<()> {
        Builder::default().build();
        // First non-empty among a, b, or "literal"
        let src = indoc! {r#"
            data:
              username: {{ coalesce .Values.primaryUser .Values.fallbackUser "guest" }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        // Both candidates should be recorded as possible sources to this slot
        assert_that!(
            &uses,
            all![
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("primaryUser"),
                    yaml_path: some(displays_as(eq("data.username"))),
                    ..
                })),
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("fallbackUser"),
                    yaml_path: some(displays_as(eq("data.username"))),
                    ..
                }))
            ]
        );
        Ok(())
    }

    #[test]
    fn ternary_tracks_condition_as_guard() -> eyre::Result<()> {
        Builder::default().build();
        // NOTE: we use .Values.prod only as the boolean condition; either branch is literal.
        let src = indoc! {r#"
            spec:
              profile: {{ ternary "prod" "dev" .Values.production }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;

        // Expect a Guard use for the boolean condition (even though the emitted value is literal).
        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: eq(&Role::Guard),
                value_path: eq("production"),
                yaml_path: none(),
                ..
            }))
        );
        Ok(())
    }

    #[test]
    fn required_emits_scalar_value_use() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            spec:
              host: {{ required "hostname is required" .Values.host }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("host"),
                yaml_path: some(displays_as(eq("spec.host"))),
                ..
            }))
        );
        Ok(())
    }

    #[test]
    fn if_required_acting_as_boolean_guard() -> eyre::Result<()> {
        Builder::default().build();
        // sometimes charts do: if required "msg" .Values.enabled
        let src = indoc! {r#"
            {{- if required "must set enabled" .Values.enabled }}
            flag: true
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;

        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: eq(&Role::Guard),
                value_path: eq("enabled"),
                yaml_path: none(),
                ..
            }))
        );
        Ok(())
    }
}

#[cfg(test)]
mod map_iteration_and_index {
    use super::*;
    use crate::analyze::{DefineIndex, Role, ValueUse};
    use color_eyre::eyre::{self, OptionExt};
    use indoc::indoc;
    use test_util::prelude::*;

    #[test]
    fn range_over_map_key_value_pairs() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            env:
              {{- range $k, $v := .Values.env }}
              - name: {{ $k }}
                value: {{ $v }}
              {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        // We should at least detect `.Values.env` as the source feeding env[].value.
        assert_that!(
            &uses,
            all![
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("env"),
                    yaml_path: none(),
                    ..
                })),
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("env"),
                    yaml_path: some(displays_as(any![eq("env[0].value"), eq("env[*].value")])),
                    ..
                }))
            ]
        );
        Ok(())
    }

    #[test]
    fn index_with_constant_key_expands_path() -> eyre::Result<()> {
        Builder::default().build();
        // {{ index .Values.extra "foo" }} -> extra.foo
        let src = indoc! {r#"
            data:
              thing: {{ index .Values.extra "foo" }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("extra.foo"),
                yaml_path: some(displays_as(eq("data.thing"))),
                ..
            }))
        );
        Ok(())
    }

    #[test]
    fn index_with_variable_key_falls_back_to_base_path() -> eyre::Result<()> {
        Builder::default().build();
        // dynamic key; we still want to attribute to `.Values.extra` somehow
        let src = indoc! {r#"
            {{- $k := "dynamic" }}
            data:
              chosen: {{ index .Values.extra $k }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;

        // Conservative expectation: we at least record `.Values.extra` being used as scalar here.
        // (Path mapping may be "data.chosen" or left unknown initially; assert the value path.)
        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("extra"),
                ..
            }))
        );
        Ok(())
    }

    #[test]
    fn range_kv_over_object_then_index_same_map() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            settings:
              {{- range $k, $v := .Values.settings }}
              {{ $k }}: {{ $v }}
              {{- end }}
              # also access a known key directly
              const: {{ index .Values.settings "const" }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;

        assert_that!(
            &uses,
            all![
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("settings"),
                    yaml_path: none(),
                    ..
                })),
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("settings"),
                    ..
                })),
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("settings.const"),
                    yaml_path: some(displays_as(eq("settings.const"))),
                    ..
                }))
            ]
        );
        Ok(())
    }
}

#[cfg(test)]
mod tpl_and_yaml_fragments {
    use super::*;
    use crate::analyze::{DefineIndex, Role, ValueUse};
    use color_eyre::eyre::{self, OptionExt};
    use indoc::indoc;
    use test_util::prelude::*;

    #[test]
    fn to_yaml_emits_fragment_on_mapping_slot() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            metadata:
              labels:
                {{- toYaml .Values.labels | nindent 4 }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;

        assert_that!(
            &uses,
            unordered_elements_are![matches_pattern!(ValueUse {
                role: eq(&Role::Fragment),
                value_path: eq("labels"),
                yaml_path: some(displays_as(eq("metadata.labels"))),
                ..
            })]
        );
        Ok(())
    }

    #[test]
    fn from_yaml_then_access_is_opaque_but_source_is_tracked() -> eyre::Result<()> {
        Builder::default().build();
        // We don't expect to follow fields of the object created by fromYaml,
        // but we should register that .Values.rawYaml was consumed.
        let src = indoc! {r#"
            {{- $o := fromYaml .Values.rawYaml }}
            result: {{ default "x" $o.someField }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;

        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: any!(
                    eq(&Role::Fragment),
                    eq(&Role::ScalarValue),
                    eq(&Role::Guard)
                ),
                value_path: eq("rawYaml"),
                ..
            }))
        );
        Ok(())
    }

    #[test]
    fn tpl_in_scalar_and_fragment_positions() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            command: {{ tpl .Values.commandTpl . }}
            metadata:
              annotations:
                {{- tpl .Values.annTpl . | nindent 4 }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;

        assert_that!(
            &uses,
            all![
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("commandTpl"),
                    yaml_path: some(displays_as(eq("command"))),
                    ..
                })),
                contains(matches_pattern!(ValueUse {
                    role: eq(&Role::Fragment),
                    value_path: eq("annTpl"),
                    yaml_path: some(displays_as(eq("metadata.annotations"))),
                    ..
                }))
            ]
        );
        Ok(())
    }
}
