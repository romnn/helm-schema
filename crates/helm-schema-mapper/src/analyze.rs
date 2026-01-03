use helm_schema_template::parse::parse_gotmpl_document;
use owo_colors::OwoColorize;
use std::collections::{BTreeMap, BTreeSet};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
use thiserror::Error;
use tree_sitter::Node;
use vfs::VfsPath;

use crate::values::{
    ValuePath as TmplValuePath, extract_values_paths, normalize_segments, parse_index_call,
    parse_selector_chain, parse_selector_expression,
};
use crate::yaml_path::{YamlPath, compute_yaml_paths_for_placeholders};
use crate::{
    analyze,
    sanitize::{Placeholder, Slot, ensure_current_line_indent},
};

pub mod diagnostics {
    use miette::{GraphicalReportHandler, LabeledSpan, NamedSource, SourceSpan, miette};

    /// Pretty print an error pointing at a byte span in `source`.
    pub fn pretty_error(
        source_name: &str,
        source: &str,
        range: std::ops::Range<usize>,
        msg: Option<&str>,
    ) -> String {
        let span = SourceSpan::new(range.start.into(), (range.end - range.start).into());

        // Create a diagnostic with a label for the span and attach the source text.
        let report = miette!(
            labels = vec![LabeledSpan::at(span, "here")],
            "{}",
            msg.unwrap_or_default()
        )
        .with_source_code(NamedSource::new(source_name.to_owned(), source.to_owned()));

        // Turn the diagnostic into a pretty string.
        let mut out = String::new();
        GraphicalReportHandler::new()
            .render_report(&mut out, report.as_ref())
            .expect("render failed");
        out
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

fn predict_slot_behind(buf: &str, target_indent: usize) -> Option<Slot> {
    for line in buf.lines().rev() {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            continue;
        }

        let indent = line.chars().take_while(|&c| c == ' ').count();
        if indent < target_indent {
            break;
        }
        if indent > target_indent {
            continue;
        }

        let after = &line[indent..];
        if after.starts_with("- ") {
            return Some(Slot::SequenceItem);
        }
        if trimmed_end.ends_with(':') {
            return Some(Slot::MappingValue);
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

fn collect_include_names(tree: &tree_sitter::Tree, src: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.is_named() {
            if n.kind() == "function_call" && is_include_call(n, src) {
                if let Ok((name, _)) = parse_include_call(n, src) {
                    names.insert(name);
                }
            }
            let mut w = n.walk();
            for ch in n.children(&mut w) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }
    }
    names
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
    let files: Vec<_> = dir
        .read_dir()?
        // TODO: defines can also be in .yaml template files
        .filter(|p| p.filename().ends_with(".tpl"))
        .collect();

    let define_index = collect_defines(&files)?;

    analyze_template(&parsed.tree, &source, define_index)
}

pub fn analyze_template(
    tree: &tree_sitter::Tree,
    source: &str,
    define_index: DefineIndex,
) -> Result<Vec<ValueUse>, Error> {
    let inline = InlineState {
        define_index: &define_index,
    };
    let mut guard = ExpansionGuard::new();

    // Hard-fail early if the template uses include() but the DefineIndex
    // doesn't have the referenced defines.
    let incs = collect_include_names(tree, source);
    if !incs.is_empty() {
        let mut missing: Vec<String> = Vec::new();
        for nm in incs {
            if inline.define_index.get(&nm).is_none() {
                missing.push(nm);
            }
        }
        if !missing.is_empty() {
            // Pick the first missing name (you can format the list if you want)
            return Err(Error::Include(IncludeError::UnknownDefine(
                missing.remove(0),
            )));
        }
    }

    // Start scope at chart root (treat dot/dollar as .Values by convention for analysis)
    let scope = Scope {
        dot: ExprOrigin::Selector(vec!["Values".into()]),
        dollar: ExprOrigin::Selector(vec!["Values".into()]),
        bindings: Default::default(),
    };

    // Inline + sanitize to a single YAML buffer with placeholders
    let mut out = InlineOut::default();
    inline_emit_tree(tree, &source, &scope, &inline, &mut guard, &mut out)?;
 
    flush_all_pending(&mut out);

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
        let role = if ph.is_fragment_output || out.force_fragment_role.contains(&ph.id) {
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
                    if let Some(tail) = parse_selector_expression(&n, src) {
                        if !tail.is_empty() {
                            out.insert(tail.join("."));
                        }
                    }
                }
            }
            "function_call" => {
                if let Some(segs) = parse_index_call(&n, src) {
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

fn extract_nindent_width(node: &Node, src: &str) -> Option<usize> {
    // direct:   (function_call nindent 2)
    if node.kind() == "function_call" {
        if function_name_of(node, src).as_deref() == Some("nindent") {
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() && ch.kind() == "argument_list" {
                    // First named child should be the width; accept int literals only
                    let mut ac = ch.walk();
                    for arg in ch.children(&mut ac) {
                        if arg.is_named() && arg.kind() == "int_literal" {
                            if let Ok(w) = arg.utf8_text(src.as_bytes()).ok()?.parse::<usize>() {
                                return Some(w);
                            }
                        }
                    }
                }
            }
        }
    }
    // chained:  (... | nindent 2)
    if node.kind() == "chained_pipeline" {
        let mut width: Option<usize> = None;
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.is_named() && ch.kind() == "function_call" {
                if function_name_of(&ch, src).as_deref() == Some("nindent") {
                    // last nindent wins
                    if let Some(w) = extract_nindent_width(&ch, src) {
                        width = Some(w);
                    }
                }
            }
        }
        return width;
    }
    None
}

// NEW: does this node (or any fn in a chained pipeline) contain any of the wanted function names?
fn pipeline_contains_any(node: &Node, src: &str, wanted: &[&str]) -> bool {
    fn has_name(call: &Node, src: &str, wanted: &[&str]) -> bool {
        if let Some(name) = function_name_of(call, src) {
            return wanted.iter().any(|w| *w == name);
        }
        false
    }

    match node.kind() {
        "function_call" => has_name(node, src, wanted),
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

fn pipeline_head_is_include<'tree>(node: Node<'tree>, src: &str) -> Option<Node<'tree>> {
    if let Some(head) = pipeline_head(node) {
        if head.kind() == "function_call" && is_include_call(head, src) {
            return Some(head);
        }
    }
    None
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
    pub values: BTreeSet<String>, // .Values.* used inside this define body
    pub includes: BTreeSet<String>, // nested {{ include "..." ... }} calls

                                  // NEW: fields the define reads from its local dot param (e.g. .customLabels, .context)
                                  // pub consumes: BTreeSet<String>,
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

// walk(doc.tree.root_node(), |node| {
// // Handle both direct include and include as pipeline head
// let inc_node = if node.kind() == "function_call" && is_include_call(node, src) {
//     Some(node)
// } else if node.kind() == "chained_pipeline" {
//     pipeline_head_is_include(node, src)
// } else {
//     None
// };
//
// if let Some(inc) = inc_node {
//     let (tpl, arg) = parse_include_call(inc, src)?;
//     if let Some(def) = defs.get_mut(&tpl) {
//         if let ArgExpr::Dict(kvs) = arg {
//             for (k, v) in kvs {
//                 // Only attribute keys the define actually consumes
//                 if !def.consumes.contains(&k) {
//                     continue;
//                 }
//                 if let Some(vp) = value_path_from_argexpr(&v) {
//                     // vp like "Values.commonLabels" → push "commonLabels"
//                     if let Some(stripped) = vp.strip_prefix("Values.") {
//                         def.values.insert(stripped.to_string());
//                     }
//                 }
//             }
//         }
//     }
// }
// Ok(())
// })?;

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
                    if let Some(tail) = parse_selector_expression(&n, src) {
                        if !tail.is_empty() {
                            values.insert(tail.join("."));
                        }
                    }
                }
            }
            "function_call" => {
                if let Some(segs) = parse_index_call(&n, src) {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineEntry {
    pub name: String,
    pub path: VfsPath,
    pub source: std::sync::Arc<String>,
    // Byte range within `source` for the body of the define.
    pub body_byte_range: std::ops::Range<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

pub fn collect_defines(files: &[VfsPath]) -> Result<DefineIndex, Error> {
    let mut idx = DefineIndex::default();
    for path in files {
        println!("\n======================= AST for helper {}", path.as_str());

        let src = path.read_to_string()?;
        let src = Arc::new(src);
        let parsed = parse_gotmpl_document(&src).ok_or(Error::ParseTemplate)?;

        for node in find_nodes(&parsed.tree, "define_action") {
            if let (Some(name), Some(body_range)) = extract_define_name_and_body(&src, node) {
                idx.insert(DefineEntry {
                    name,
                    path: path.clone(),
                    source: Arc::clone(&src),
                    body_byte_range: body_range,
                });
            }
        }
    }
    Ok(idx)
}

#[derive(Debug, Clone)]
pub enum ExprOrigin {
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

fn is_helm_builtin_root(name: &str) -> bool {
    matches!(name, "Capabilities" | "Chart" | "Release" | "Template")
}

#[derive(Debug, Clone)]
pub struct Scope {
    // What does `.` mean here?
    pub dot: ExprOrigin,
    // What does `$` mean here? (usually root; we carry it through)
    pub dollar: ExprOrigin,
    // Local bindings for paths inside the define, e.g. "config" => <call-site expr origin>
    pub bindings: HashMap<String, ExprOrigin>,
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
                // bare "$"
                if segments.len() == 1 {
                    return self.dollar.clone();
                }
                // Ignore "$.Capabilities.*" / "$.Chart.*" / etc.
                if is_helm_builtin_root(&segments[1]) {
                    return ExprOrigin::Opaque;
                }
                // "$.Values[.tail]" → extend from current `$` origin
                if segments[1].as_str() == "Values" {
                    return self.extend_origin(self.dollar.clone(), &segments[1..]);
                }

                // "$.<field>[.tail]" → root-dollar field: extend from `$`
                if segments[1].starts_with('.') {
                    return self.extend_origin(self.dollar.clone(), &segments[1..]);
                }

                // "$name[.tail]" → binding lookup
                let var = segments[1].as_str();
                if let Some(bound) = self.bindings.get(var) {
                    if segments.len() == 2 {
                        return bound.clone();
                    }
                    return self.extend_origin(bound.clone(), &segments[2..]);
                }

                // Unbound $var → degrade to opaque (do NOT panic)
                return ExprOrigin::Opaque;

                // // "$name[.tail]" → binding lookup
                // let var = segments[1].as_str();
                // if let Some(bound) = self.bindings.get(var) {
                //     if segments.len() == 2 {
                //         return bound.clone();
                //     }
                //     return self.extend_origin(bound.clone(), &segments[2..]);
                // }

                // // Unbound $var → hard error
                // panic!("unbound template variable ${var} in selector: {segments:?}");
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
            }
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
            "selector_expression" => {
                // Use our robust chain parser so we always get ["." | "$", ...] tokens.
                // This lets include-arg resolution rebase through local bindings correctly
                // (e.g., .config.inner → ["." ,"config","inner"]).
                if let Some(segs) = parse_selector_chain(node, src) {
                    ArgExpr::Selector(segs)
                } else {
                    ArgExpr::Opaque
                }
            }
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
pub struct ExpansionGuard {
    pub stack: Vec<String>,
}

impl ExpansionGuard {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }
}

/// This state sits next to your existing emitter state. We keep it tiny.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineState<'a> {
    pub define_index: &'a DefineIndex,
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

fn collect_bases_from_expr(base: Node, src: &str, scope: &Scope, env: &Env) -> Vec<String> {
    match base.kind() {
        // Straight selectors
        "selector_expression" => {
            if let Some(segs) =
                parse_selector_expression(&base, src).or_else(|| parse_selector_chain(base, src))
            {
                if let Some(vp) = origin_to_value_path(&scope.resolve_selector(&segs)) {
                    return vec![vp];
                }
            }
            vec![]
        }
        // Dot → current dot origin (if it maps into .Values)
        "dot" => origin_to_value_path(&scope.dot).into_iter().collect(),

        // $ or $name
        "variable" => {
            let raw = base.utf8_text(src.as_bytes()).unwrap_or("").trim();
            if raw == "$" {
                return origin_to_value_path(&scope.dollar).into_iter().collect();
            }
            let name = raw.trim_start_matches('$');
            if let Some(set) = env.get(name) {
                return set.iter().cloned().collect();
            }

            // Degrade gracefully: treat as opaque/unknown base
            eprintln!(
                "{}",
                diagnostics::pretty_error(
                    "todo",
                    src,
                    base.byte_range(),
                    Some(&format!(
                        "unbound template variable ${name} used as base; treating as opaque"
                    ))
                )
            );
            return vec![];

            // // Keep the same behavior as elsewhere: panic for unbound user vars
            // panic!(
            //     "unbound template variable ${name} used as base at bytes {:?}",
            //     base.byte_range()
            // );
        }

        // `.foo` in a bare field
        "field" => {
            let name = base
                .child_by_field_name("identifier")
                .or_else(|| base.child_by_field_name("field_identifier"))
                .and_then(|id| {
                    id.utf8_text(src.as_bytes())
                        .ok()
                        .map(|s| s.trim().to_string())
                })
                .unwrap_or_else(|| {
                    base.utf8_text(src.as_bytes())
                        .unwrap_or("")
                        .trim()
                        .trim_start_matches('.')
                        .to_string()
                });

            if let Some(bound) = scope.bindings.get(&name) {
                if let Some(vp) = origin_to_value_path(bound) {
                    return vec![vp];
                }
                // fall back: resolve `.name` relative to dot
                let segs = vec![".".to_string(), name];
                return origin_to_value_path(&scope.resolve_selector(&segs))
                    .into_iter()
                    .collect();
            } else {
                let segs = vec![".".to_string(), name];
                return origin_to_value_path(&scope.resolve_selector(&segs))
                    .into_iter()
                    .collect();
            }
        }

        // NEW: allow nested calls and parenthesized/chained pipelines as base
        "function_call" | "parenthesized_pipeline" | "chained_pipeline" => {
            // Delegate to the main collector. For something like
            //   (index .Values "a")
            // this returns {"a"} which we can then extend with outer keys.
            collect_values_with_scope(base, src, scope, env)
                .into_iter()
                .collect()
        }

        _ => vec![],
    }
}

/// Collect `.Values.*` keys reachable from `node` using the caller's `scope` and current `$var` env.
fn collect_values_with_scope(node: Node, src: &str, scope: &Scope, env: &Env) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![node];

    while let Some(n) = stack.pop() {
        match n.kind() {
            "variable" => {
                if let Some(var) = variable_ident_of(&n, src) {
                    // bare '$' used as a value is uncommon; ignore here.
                    if !var.is_empty() {
                        if let Some(bases) = env.get(&var) {
                            for b in bases {
                                if !b.is_empty() {
                                    out.insert(b.clone());
                                }
                            }
                        }
                        // (Do not hard-error here; we only hard-error for unbound variables in SELECTORS.)
                    }
                }
            }
            "dot" => {
                if let Some(vp) = origin_to_value_path(&scope.dot) {
                    if !vp.is_empty() {
                        out.insert(vp);
                    }
                }
            }
            // Handle bare `.foo` nodes (but NOT when it's part of a selector_expression)
            "field" => {
                if n.parent()
                    .map(|p| p.kind() == "selector_expression")
                    .unwrap_or(false)
                {
                    // The outer selector will be handled in the selector_expression arm.
                    // Avoid collecting ".foo" twice.
                    let mut c = n.walk();
                    for ch in n.children(&mut c) {
                        if ch.is_named() {
                            stack.push(ch);
                        }
                    }
                    continue;
                }
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
                dbg!(n.utf8_text(src.as_bytes()));

                // 0) First, if the left operand is a variable ($ or $name), expand via env — do this
                //    even if parse_selector_expression() returns None on this grammar.
                if let Some(op) = n.child_by_field_name("operand").or_else(|| n.child(0)) {
                    if op.kind() == "variable" {
                        if let Some(var) = variable_ident_of(&op, src) {
                            // if let Some(bases) = env.get(&var) {
                            if var.is_empty() {
                                // bare "$" — let fallback handle
                            } else if let Some(bases) = env.get(&var) {
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
                            } else {
                                eprintln!(
                                    "{}",
                                    diagnostics::pretty_error(
                                        "todo",
                                        src,
                                        n.byte_range(),
                                        Some(&format!(
                                            "unbound template variable ${var} in selector; treating as opaque"
                                        ))
                                    )
                                );
                                // Do NOT panic; just skip this fast-path and let generic resolution run
                                // (or effectively ignore this occurrence for Values collection).
                                continue;

                                // eprintln!(
                                //     "{}",
                                //     diagnostics::pretty_error("todo", src, n.byte_range(), None)
                                // );
                                // panic!(
                                //     "unbound template variable ${var} used in selector at bytes {:?}",
                                //     n.byte_range()
                                // );
                            }
                        }
                    }
                }

                // 1) Normal path: if we can parse the selector segments, use scope resolution
                if let Some(segs) = parse_selector_chain(n, src) {
                    let parent_is_selector = n
                        .parent()
                        .map(|p| p.kind() == "selector_expression")
                        .unwrap_or(false);

                    if parent_is_selector {
                        // We'll still traverse its children later; just don't record this node itself.
                    } else {
                        if !segs.is_empty() && segs[0] == "$" && segs.len() >= 2 {
                            let var = segs[1].trim_start_matches('.');

                            // NEW: handle Helm builtins rooted at '$'
                            if is_helm_builtin_root(var) {
                                // Either just ignore (no .Values key to collect) or fall through
                                // to scope.resolve_selector(segs) which will return Opaque.
                                // Ignoring is fine here:
                                continue;
                            }

                            if var == "Values" {
                                // treat as "$.Values..." — fall through to normal resolution below
                            } else if let Some(bases) = env.get(var) {
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
                            } else {
                                eprintln!(
                                    "{}",
                                    diagnostics::pretty_error(
                                        "todo",
                                        src,
                                        n.byte_range(),
                                        Some(&format!(
                                            "unbound template variable ${var} in selector; treating as opaque"
                                        ))
                                    )
                                );
                                // Do NOT panic; just skip this fast-path and let generic resolution run
                                // (or effectively ignore this occurrence for Values collection).
                                continue;

                                // eprintln!(
                                //     "{}",
                                //     diagnostics::pretty_error("todo", src, n.byte_range(), None)
                                // );
                                // panic!(
                                //     "unbound template variable ${var} used in selector at bytes {:?}",
                                //     n.byte_range()
                                // );
                            }
                        }
                        if let Some(first) = segs.first() {
                            if first.starts_with('$') {
                                let var = first.trim_start_matches('$');
                                if var.is_empty() {
                                    // bare "$" — fall through
                                } else if let Some(bases) = env.get(var) {
                                    // if let Some(bases) = env.get(var) {
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
                                } else {
                                    eprintln!(
                                        "{}",
                                        diagnostics::pretty_error(
                                            "todo",
                                            src,
                                            n.byte_range(),
                                            Some(&format!(
                                                "unbound template variable ${var} in selector; treating as opaque"
                                            ))
                                        )
                                    );
                                    // Do NOT panic; just skip this fast-path and let generic resolution run
                                    // (or effectively ignore this occurrence for Values collection).
                                    continue;

                                    // eprintln!(
                                    //     "{}",
                                    //     diagnostics::pretty_error(
                                    //         "todo",
                                    //         src,
                                    //         n.byte_range(),
                                    //         None,
                                    //     )
                                    // );
                                    // panic!(
                                    //     "unbound template variable ${var} used in selector at bytes {:?}",
                                    //     n.byte_range()
                                    // );
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
                        if let Some(segs) = parse_selector_chain(head, src) {
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
                if let Some(segs) = parse_index_call(&n, src) {
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
                // NEW: sprig get(base, "key") → base.key
                if function_name_of(&n, src).as_deref() == Some("get") {
                    if let Some(args) = arg_list(&n) {
                        // base = first named arg; remaining named string args are keys
                        let mut w = args.walk();
                        let mut named = args.named_children(&mut w);
                        if let Some(base) = named.next() {
                            // collect string keys
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
                            let mut bases: Vec<String> =
                                collect_bases_from_expr(base, src, scope, env);

                            // combine base + keys
                            for mut b in bases {
                                if b == "." {
                                    b.clear();
                                }
                                if keys.is_empty() {
                                    // conservative fallback
                                    if !b.is_empty() {
                                        out.insert(b);
                                    }
                                } else if b.is_empty() {
                                    // root-of-.Values case
                                    out.insert(keys.join("."));
                                } else {
                                    out.insert(format!("{b}.{}", keys.join(".")));
                                }
                            }
                        }
                    }
                    // fully handled; don't descend (prevents duplicate base collection)
                    continue;
                }
                // NEW: fromYaml consumes YAML text → record the input origin(s), but do not
                // attempt to treat its result as a selector-capable value.
                if function_name_of(&n, src).as_deref() == Some("fromYaml") {
                    if let Some(args) = arg_list(&n) {
                        let mut w = args.walk();
                        if let Some(first) = args.named_children(&mut w).next() {
                            // Reuse your existing selector resolution helpers:
                            // - selector_expression / field / dot / variable
                            if let Some(segs) = parse_selector_chain(first, src) {
                                if let Some(vp) =
                                    origin_to_value_path(&scope.resolve_selector(&segs))
                                {
                                    out.insert(vp); // <- record ".Values.rawYaml"
                                }
                            } else if first.kind() == "dot" {
                                if let Some(vp) = origin_to_value_path(&scope.dot) {
                                    out.insert(vp);
                                }
                            } else if first.kind() == "variable" {
                                let raw = first.utf8_text(src.as_bytes()).unwrap_or("").trim();
                                if raw == "$" {
                                    if let Some(vp) = origin_to_value_path(&scope.dollar) {
                                        out.insert(vp);
                                    }
                                } else if let Some(set) = env.get(raw.trim_start_matches('$')) {
                                    out.extend(set.iter().cloned());
                                }
                            }
                        }
                    }
                    // fully handled
                    continue;
                }
                // NEW: sprig pluck("key", base[, ...]) → base.key (for each base)
                if function_name_of(&n, src).as_deref() == Some("pluck") {
                    if let Some(args) = arg_list(&n) {
                        // collect named children once
                        let mut named_nodes: Vec<Node> = Vec::new();
                        let mut w = args.walk();
                        for ch in args.named_children(&mut w) {
                            named_nodes.push(ch);
                        }

                        // first named must be a string key
                        if let Some(first) = named_nodes.first() {
                            if matches!(
                                first.kind(),
                                "interpreted_string_literal" | "raw_string_literal" | "string"
                            ) {
                                let key = first
                                    .utf8_text(src.as_bytes())
                                    .unwrap_or("")
                                    .trim()
                                    .trim_matches('"')
                                    .trim_matches('`')
                                    .to_string();

                                if !key.is_empty() {
                                    for base in named_nodes.into_iter().skip(1) {
                                        // resolve base -> list of base paths
                                        let mut bases: Vec<String> =
                                            collect_bases_from_expr(base, src, scope, env);

                                        for mut b in bases {
                                            if b == "." {
                                                b.clear();
                                            }
                                            if b.is_empty() {
                                                // root-of-.Values → just the key
                                                out.insert(key.clone());
                                            } else {
                                                out.insert(format!("{b}.{}", key));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // fully handled
                    continue;
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
                            let mut bases: Vec<String> =
                                collect_bases_from_expr(base, src, scope, env);

                            // And finally insert `{base}.{joined keys}`
                            // If no constant keys, fall back to the bare base (dynamic key case)
                            for mut b in bases {
                                if b == "." {
                                    b.clear();
                                }
                                if keys.is_empty() {
                                    // dynamic key → at least attribute to the base value
                                    if !b.is_empty() {
                                        out.insert(b);
                                    }
                                } else if b.is_empty() {
                                    // if keys.is_empty() {
                                    //     // no-op: do not insert a bare base for index()
                                    // } else if b.is_empty() {
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
                        ArgExpr::Selector(sel) => callee.dot = scope.resolve_selector(&sel),
                        ArgExpr::Dict(kvs) => {
                            for (k, v) in kvs {
                                let origin = match v {
                                    ArgExpr::Dot => scope.dot.clone(),
                                    ArgExpr::Dollar => scope.dollar.clone(),
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

// NEW: remember the parent container (mapping/sequence) at a given indent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FragmentParent {
    slot: Slot, // MappingValue or SequenceItem
    indent: usize,
}

fn line_indent(buf: &str) -> usize {
    let start = buf.rfind('\n').map(|i| i + 1).unwrap_or(0);
    buf[start..].chars().take_while(|c| *c == ' ').count()
}

// NEW: update the sticky parent hint when we append raw YAML text.
// - If a non-empty line ends with ":" → mapping parent at that indent
// - If a line (after indent) starts with "- " → sequence item context
// - Any other non-whitespace content clears the hint (conservatively)
fn update_fragment_parent_from_text(
    out: &mut InlineOut,
    // hint: &mut Option<FragmentParent>,
    text: &str,
) {
    // for line in text.split('\n') {
    //     let trimmed_end = line.trim_end();
    //     if trimmed_end.is_empty() {
    //         continue;
    //     }
    //     let indent = line.chars().take_while(|&c| c == ' ').count();
    //     let after = &line[indent..];
    //
    //     if trimmed_end.ends_with(':') {
    //         out.last_fragment_parent = Some(FragmentParent {
    //             slot: Slot::MappingValue,
    //             indent,
    //         });
    //     } else if after.starts_with("- ") {
    //         // Sequence context detected at this indent.
    //         out.last_fragment_parent = Some(FragmentParent {
    //             slot: Slot::SequenceItem,
    //             indent,
    //         });
    //
    //         // 🔧 Retro-fix: if the last placeholder at this indent was emitted as a mapping value,
    //         // rewrite that line into a list item.
    //         if let Some(&(line_start, ph_id, ph_slot)) = out.last_ph_line_by_indent.get(&indent) {
    //             if matches!(ph_slot, Slot::MappingValue) {
    //                 // Locate end of that line
    //                 let line_end = out.buf[line_start..]
    //                     .find('\n')
    //                     .map(|k| line_start + k)
    //                     .unwrap_or_else(|| out.buf.len());
    //
    //                 // The old line is: <indent>"__TSG_PLACEHOLDER_{id}__": 0
    //                 // Replace with:    <indent>- { "__TSG_PLACEHOLDER_{id}__": 0 }
    //                 let replacement = format!(
    //                     "{}- {{ \"__TSG_PLACEHOLDER_{}__\": 0 }}",
    //                     " ".repeat(indent),
    //                     ph_id
    //                 );
    //
    //                 out.buf.replace_range(line_start..line_end, &replacement);
    //
    //                 // Update the cached slot for this indent so we don't rewrite again.
    //                 out.last_ph_line_by_indent
    //                     .insert(indent, (line_start, ph_id, Slot::SequenceItem));
    //             }
    //         }
    //     } else if after.chars().any(|ch| !ch.is_whitespace()) {
    //         out.last_fragment_parent = None;
    //     }
    // }
    for line in text.split('\n') {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|&c| c == ' ').count();
        let after = &line[indent..];
        if trimmed_end.ends_with(':') {
            out.last_fragment_parent = Some(FragmentParent {
                slot: Slot::MappingValue,
                indent,
            });
        } else if after.starts_with("- ") {
            out.last_fragment_parent = Some(FragmentParent {
                slot: Slot::SequenceItem,
                indent,
            });
        } else if after.chars().any(|ch| !ch.is_whitespace()) {
            // Some real content that isn't a key/dash → drop the old hint
            out.last_fragment_parent = None;
        }
    }
}

#[cfg(false)]
mod guess {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum BlockKind {
        Seq,
        Map,
        Unknown,
    }

    /// Scan around the insertion line to guess whether siblings at `indent` are a sequence or a map.
    /// We look forward first, then backward; only consider non-empty lines at the same indent.
    fn guess_block_kind(lines: &[&str], cur_idx: usize, indent: usize) -> BlockKind {
        // look forward up to ~16 logical lines
        let fwd_limit = lines.len().min(cur_idx + 16);
        for j in (cur_idx + 1)..fwd_limit {
            let l = lines[j];
            let t = l.trim_start();
            if t.is_empty() {
                continue;
            }
            let ind = l.len() - t.len();
            if ind < indent {
                break;
            } // left the block
            if ind == indent {
                if t.starts_with("- ") || t == "-" {
                    return BlockKind::Seq;
                }
                if t.ends_with(':') || t.contains(": ") {
                    return BlockKind::Map;
                }
            }
        }
        // look backward up to ~16 logical lines
        let start = cur_idx.saturating_sub(16);
        for j in (start..cur_idx).rev() {
            let l = lines[j];
            let t = l.trim_start();
            if t.is_empty() {
                continue;
            }
            let ind = l.len() - t.len();
            if ind < indent {
                break;
            }
            if ind == indent {
                if t.starts_with("- ") || t == "-" {
                    return BlockKind::Seq;
                }
                if t.ends_with(':') || t.contains(": ") {
                    return BlockKind::Map;
                }
            }
        }
        BlockKind::Unknown
    }

    /// Count the number of '\n' before `byte` to get the 0-based source line index.
    fn source_line_index_at(src: &str, byte: usize) -> usize {
        src[..byte]
            .as_bytes()
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
    }

    /// Emit a single *line* placeholder adjusted to the surrounding container.
    /// IMPORTANT: This writes at the *current* line position (so call `ensure_current_line_indent` first).
    /// Returns the kind it chose (Seq/Map/Unknown) so callers can update sticky parent hints.
    fn emit_contextual_placeholder_line(
        out: &mut String,
        id: usize,
        target_indent: usize,
        src: &str,
        node: &tree_sitter::Node,
    ) -> BlockKind {
        let lines: Vec<&str> = src.split('\n').collect();
        let cur_idx = source_line_index_at(src, node.start_byte());
        let kind = guess_block_kind(&lines, cur_idx, target_indent);

        use std::fmt::Write as _;
        let tag = format!("__TSG_PLACEHOLDER_{}__", id);

        match kind {
            BlockKind::Seq => {
                // Emit a *list item* that is a tiny map: - { "__TSG_PLACEHOLDER_#__": 0 }
                let _ = writeln!(out, r#"- {{ "{tag}": 0 }}"#);
            }
            BlockKind::Map => {
                // Emit a mapping entry under the current value position: "__TSG_PLACEHOLDER_#__": 0
                let _ = writeln!(out, r#""{tag}": 0"#);
            }
            BlockKind::Unknown => {
                // Stay safe: a comment never breaks YAML and still leaves a breadcrumb
                let _ = writeln!(out, r#"# {tag}"#);
            }
        }

        kind
    }
}

#[derive(Clone, Debug)]
struct PendingFragment {
    id: usize,
    indent: usize,
}

fn flush_pending_for_indent(out: &mut InlineOut, indent: usize, slot: Slot) {
    use std::fmt::Write as _;
    if let Some(mut ids) = out.pending.remove(&indent) {
        if !out.buf.ends_with('\n') {
            out.buf.push('\n');
        }
        for id in ids.drain(..) {
            ensure_current_line_indent(&mut out.buf, indent);
            write_fragment_placeholder_line(&mut out.buf, id, slot);
        }
        // record parent once we actually wrote something
        out.last_fragment_parent = Some(FragmentParent { slot, indent });
    }
}

fn flush_all_pending(out: &mut InlineOut) {
    // if we never got a decisive line, default to mapping at that indent
    let indents: Vec<usize> = out.pending.keys().copied().collect();
    for i in indents {
        flush_pending_for_indent(out, i, Slot::MappingValue);
    }
}

// fn flush_pending_for_indent(out: &mut InlineOut, indent: usize, slot: Slot) {
//     use std::fmt::Write as _;
//     if let Some(mut ids) = out.pending.remove(&indent) {
//         if !out.buf.ends_with('\n') {
//             out.buf.push('\n');
//         }
//         for id in ids.drain(..) {
//             ensure_current_line_indent(&mut out.buf, indent);
//             write_fragment_placeholder_line(&mut out.buf, id, slot);
//         }
//         // update sticky parent after actually writing
//         out.last_fragment_parent = Some(FragmentParent { slot, indent });
//     }
// }
//
// fn flush_all_pending(out: &mut InlineOut) {
//     // default to mapping value when we never saw a disambiguating line
//     let indents: Vec<usize> = out.pending.keys().copied().collect();
//     for i in indents {
//         flush_pending_for_indent(out, i, Slot::MappingValue);
//     }
// }

#[derive(Default, Clone, PartialEq, Eq)]
pub struct InlineOut {
    pub buf: String,
    pub placeholders: Vec<crate::sanitize::Placeholder>,
    pub guards: Vec<ValueUse>,
    pub next_id: usize,
    pub env: Env, // $var -> set of .Values.* it refers to
    // NEW: force Role::Fragment at conversion time (without changing how we wrote the placeholder)
    pub force_fragment_role: HashSet<usize>,
    // NEW: sticky parent container hint (mapping or sequence) at a specific indent
    pub last_fragment_parent: Option<FragmentParent>,

    // NEW: defer fragment placeholders until we know the container kind at this indent
    pub pending: std::collections::BTreeMap<usize, Vec<usize>>,

    // // NEW: deferred fragment placeholders waiting for disambiguation at an indent
    // pub pending: std::collections::BTreeMap<usize, Vec<usize>>,
    // NEW: remember the last fragment placeholder line we emitted at a given indent
    // pub last_ph_line_by_indent: BTreeMap<
    //     usize,
    //     (
    //         usize, /*line_start*/
    //         usize, /*id*/
    //         Slot,  /*slot we used*/
    //     ),
    // >,
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
    let mut it = buf.rsplit('\n');
    let cur = it.next().unwrap_or("");
    let cur_trim_end = cur.trim_end();

    // NEW: if the *current* line ends with ':', we're about to emit its value
    if cur_trim_end.ends_with(':') {
        return Slot::MappingValue;
    }

    let cur_trim = cur.trim_start();
    if cur_trim.starts_with("- ") {
        return Slot::SequenceItem;
    }

    let prev_non_empty = it.find(|l| !l.trim().is_empty()).unwrap_or("");
    if prev_non_empty.trim_end().ends_with(':') {
        return Slot::MappingValue;
    }
    Slot::Plain
}

// fn current_slot_in_buf(buf: &str) -> Slot {
//     // Look at the current and the previous non-empty line
//     let mut it = buf.rsplit('\n');
//
//     let cur = it.next().unwrap_or(""); // current (possibly empty) line
//     let cur_trim = cur.trim_start();
//
//     // If current line already starts with "- " → list item context
//     if cur_trim.starts_with("- ") {
//         return Slot::SequenceItem;
//     }
//
//     let prev_non_empty = it.find(|l| !l.trim().is_empty()).unwrap_or("");
//
//     let prev_trim = prev_non_empty.trim_end();
//
//     // Definitely mapping value if the previous non-empty line ends with ':'
//     if prev_trim.ends_with(':') {
//         return Slot::MappingValue;
//     }
//
//     // // NEW: stay in mapping if the previous non-empty line is a placeholder mapping entry
//     // // e.g.   "  "__TSG_PLACEHOLDER_3__": 0"
//     // if prev_trim.contains("__TSG_PLACEHOLDER_") && prev_trim.contains("\": 0") {
//     //     return Slot::MappingValue;
//     // }
//
//     Slot::Plain
// }

fn write_fragment_placeholder_line(buf: &mut String, id: usize, slot: Slot) {
    use std::fmt::Write as _;
    match slot {
        Slot::MappingValue => {
            let _ = writeln!(buf, "\"__TSG_PLACEHOLDER_{id}__\": 0");
        }
        Slot::SequenceItem => {
            // // Write a *list item* that is a one-line mapping
            // let _ = writeln!(buf, "- {{ \"__TSG_PLACEHOLDER_{id}__\": 0 }}");
            // IMPORTANT: a sequence item must start with "- "
            let _ = writeln!(buf, "- {{ \"__TSG_PLACEHOLDER_{id}__\": 0 }}");
        }
        Slot::Plain => {
            let _ = write!(buf, "\"__TSG_PLACEHOLDER_{id}__\": 0\n");
        }
    }
}

fn emit_text_and_update(out: &mut InlineOut, text: &str) {
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches('\n');
        let indent = line.chars().take_while(|&c| c == ' ').count();
        let after = &line[indent..];

        let is_template = after.trim_start().starts_with("{{");
        let is_blank = after.trim().is_empty();

        if !is_template && !is_blank {
            let deeper: Vec<usize> = out
                .pending
                .keys()
                .copied()
                .filter(|&i| i > indent)
                .collect();
            for i in deeper {
                let slot = predict_slot_behind(&out.buf, i).unwrap_or(Slot::MappingValue);
                flush_pending_for_indent(out, i, slot);
            }
        }

        // If the next line at this indent is decisive (non-template, non-blank), flush pending first
        if out.pending.contains_key(&indent) && !is_template && !is_blank {
            if after.starts_with("- ") {
                flush_pending_for_indent(out, indent, Slot::SequenceItem);
            } else if after.contains(':') {
                flush_pending_for_indent(out, indent, Slot::MappingValue);
            }
            // else keep waiting (e.g., comments)
        }

        // Now append the actual text and keep the simple sticky hint
        out.buf.push_str(raw);
        update_fragment_parent_from_text(out, raw);
    }
}

// fn emit_text_and_update(out: &mut InlineOut, text: &str) {
//     // Process line-by-line, so we can flush before appending the decisive line
//     for raw in text.split_inclusive('\n') {
//         let line = raw.trim_end_matches('\n');
//         let indent = line.chars().take_while(|&c| c == ' ').count();
//         let after = &line[indent..];
//
//         // 1) If we dedent, flush any pending deeper-than-current as mapping
//         {
//             let deeper: Vec<usize> = out
//                 .pending
//                 .keys()
//                 .copied()
//                 .filter(|&i| i > indent)
//                 .collect();
//             for i in deeper {
//                 flush_pending_for_indent(out, i, Slot::MappingValue);
//             }
//         }
//
//         // 2) If this line is a real YAML token at this indent, flush pending at this indent
//         //    BEFORE writing the line
//         let is_template = after.trim_start().starts_with("{{");
//         let is_blank = after.trim().is_empty();
//
//         if out.pending.contains_key(&indent) && !is_template && !is_blank {
//             if after.starts_with("- ") {
//                 flush_pending_for_indent(out, indent, Slot::SequenceItem);
//             } else if after.contains(':') {
//                 // "key:" (or "key: value") → mapping value at this indent
//                 flush_pending_for_indent(out, indent, Slot::MappingValue);
//             }
//             // else: not decisive, keep waiting
//         }
//
//         // 3) Append the actual text line
//         out.buf.push_str(raw);
//
//         // 4) Maintain the sticky parent hint from what we just wrote
//         update_fragment_parent_from_text(out, raw);
//     }
// }

// fn write_fragment_placeholder_line(buf: &mut String, id: usize, slot: Slot) {
//     use std::fmt::Write as _;
//     match slot {
//         Slot::MappingValue => {
//             let _ = writeln!(buf, "\"__TSG_PLACEHOLDER_{id}__\": 0");
//         }
//         Slot::SequenceItem => {
//             // Treat fragment as a mapping node under the list item
//             let _ = writeln!(buf, "\"__TSG_PLACEHOLDER_{id}__\": 0");
//         }
//         Slot::Plain => {
//             let _ = write!(buf, "\"__TSG_PLACEHOLDER_{id}__\": 0\n");
//         }
//     }
// }

// fn write_fragment_placeholder_line(buf: &mut String, id: usize, slot: Slot) {
//     use std::fmt::Write as _;
//     match slot {
//         Slot::MappingValue => {
//             let _ = writeln!(buf, "\"__TSG_PLACEHOLDER_{id}__\": 0");
//         }
//         Slot::SequenceItem => {
//             // List item already started with "- "
//             let _ = writeln!(buf, "\"__TSG_PLACEHOLDER_{id}__\"");
//         }
//         Slot::Plain => {
//             // let _ = writeln!(buf, "\"__TSG_PLACEHOLDER_{id}__\"");
//             // TOP-LEVEL or “plain” site for *fragments* → use a mapping stub
//             // so the overall YAML document remains a mapping, not a scalar.
//             let _ = write!(buf, "\"__TSG_PLACEHOLDER_{id}__\": 0\n");
//         }
//     }
// }

/// Scan forward in `src` from byte offset `from` to find the next non-empty *text* line
/// at exactly `target_indent`. Ignore template actions like `{{ ... }}`.
/// If such a line starts with "- " => sequence item; if it looks like `key:` => mapping value.
/// Otherwise return None (can’t decide).
fn predict_slot_ahead(src: &str, from: usize, target_indent: usize) -> Option<Slot> {
    let bytes = src.as_bytes();
    let mut i = from;

    // `from` can point into the middle of the current line (e.g., `node.end_byte()` inside `{{ ... }}`).
    // Slot prediction is based on line indentation, so start scanning at the next line boundary.
    if i < bytes.len() {
        while i < bytes.len() && bytes[i] != b'\n' {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'\n' {
            i += 1;
        }
    }

    while i < bytes.len() {
        // line bounds
        let line_start = i;
        let mut j = i;
        while j < bytes.len() && bytes[j] != b'\n' {
            j += 1;
        }
        let line = &src[line_start..j];
        i = if j < bytes.len() { j + 1 } else { j };

        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }

        let indent = line.chars().take_while(|&c| c == ' ').count();
        let after = &line[indent..];

        // Skip pure template actions
        if after.starts_with("{{") || after.starts_with("{%-") || after.starts_with("{%") {
            continue;
        }

        // If we bubbled out, stop — parent changed
        if indent < target_indent {
            return None;
        }
        // If deeper, not informative for this level — keep scanning
        if indent > target_indent {
            continue;
        }

        // indent == target_indent
        if after.starts_with("- ") {
            return Some(Slot::SequenceItem);
        }
        // treat `key:`/`"key":` as mapping; cheap heuristic is fine here
        if after.contains(':') {
            return Some(Slot::MappingValue);
        }

        // some other token: keep looking
    }
    None
}

fn write_placeholder(
    out: &mut InlineOut,
    node: Node,
    src: &str,
    values: BTreeSet<String>,
    is_fragment_output: bool,
    force_fragment_role: bool,
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
        // Decide the container target indent (same logic you already compute)
        let nin = extract_nindent_width(&node, src).unwrap_or(0);
        let mut slot = current_slot_in_buf(&out.buf);
        let before_indent = line_indent(&out.buf);

        // Inherit parent if we looked "Plain" but have a sticky parent recorded at this indent
        if matches!(slot, Slot::Plain) {
            if let Some(h) = out.last_fragment_parent {
                let target_indent = if nin > 0 { nin } else { before_indent };
                if h.indent == target_indent {
                    slot = h.slot;
                }
            }
        }

        // Unified line break & target indent (we still want the fragment on its own line)
        let (need_nl, target_indent) = match slot {
            Slot::MappingValue => (true, if nin > 0 { nin } else { before_indent + 2 }),
            Slot::SequenceItem => (true, if nin > 0 { nin } else { before_indent + 2 }),
            Slot::Plain => (false, if nin > 0 { nin } else { before_indent }),
        };

        if need_nl && !out.buf.ends_with('\n') {
            out.buf.push('\n');
        }

        let predicted = predict_slot_ahead(src, node.end_byte(), target_indent)
            .or_else(|| predict_slot_behind(&out.buf, target_indent));

        if matches!(predicted, Some(Slot::SequenceItem)) {
            slot = Slot::SequenceItem;
        } else if matches!(slot, Slot::Plain) {
            slot = predicted.unwrap_or(Slot::Plain);
        }

        if matches!(slot, Slot::Plain) {
            out.pending.entry(target_indent).or_default().push(id);
        } else {
            ensure_current_line_indent(&mut out.buf, target_indent);
            write_fragment_placeholder_line(&mut out.buf, id, slot);
            out.last_fragment_parent = Some(FragmentParent {
                slot,
                indent: target_indent,
            });
        }

        // let nin = extract_nindent_width(&node, src).unwrap_or(0);
        // let mut slot = current_slot_in_buf(&out.buf);
        // let before_indent = line_indent(&out.buf);
        //
        // // If we think we're Plain but we have a sticky parent at the same indent, inherit it
        // if matches!(slot, Slot::Plain) {
        //     if let Some(h) = out.last_fragment_parent {
        //         let target_indent = if nin > 0 { nin } else { before_indent };
        //         if h.indent == target_indent {
        //             slot = h.slot;
        //         }
        //     }
        // }
        //
        // // --- NEW: unified line-break & indent policy for container sites ---
        // let (need_nl, target_indent) = match slot {
        //     Slot::MappingValue => (true, if nin > 0 { nin } else { before_indent + 2 }),
        //     Slot::SequenceItem => (true, if nin > 0 { nin } else { before_indent + 2 }),
        //     Slot::Plain => (false, if nin > 0 { nin } else { before_indent }),
        // };
        //
        // // NEW: look ahead to disambiguate container at this indent
        // let predicted = predict_slot_ahead(src, node.end_byte(), target_indent);
        // if let Some(p) = predicted {
        //     // Only allow Seq/Map override; never force Plain here
        //     match p {
        //         Slot::SequenceItem | Slot::MappingValue => {
        //             slot = p;
        //         }
        //         Slot::Plain => {}
        //     }
        // }
        //
        // let line_start = out.buf.rfind('\n').map(|i| i + 1).unwrap_or(0);
        //
        // if need_nl && !out.buf.ends_with('\n') {
        //     out.buf.push('\n');
        // }
        // if target_indent > 0 {
        //     ensure_current_line_indent(&mut out.buf, target_indent);
        // }
        //
        // // // remember where this placeholder line begins, and with which slot we *think* we’re in
        // // out.last_ph_line_by_indent
        // //     .insert(target_indent, (line_start, id, slot));
        //
        // // actually write the placeholder line
        // write_fragment_placeholder_line(&mut out.buf, id, slot);
        //
        // // // Write the container-safe placeholder line (no leading spaces; we already set indent)
        // // let kind = emit_contextual_placeholder_line(&mut out.buf, id, target_indent, src, &node);
        // //
        // // // Update sticky parent based on what we actually emitted
        // // out.last_fragment_parent = match kind {
        // //     BlockKind::Seq => Some(FragmentParent {
        // //         slot: Slot::SequenceItem,
        // //         indent: target_indent,
        // //     }),
        // //     BlockKind::Map => Some(FragmentParent {
        // //         slot: Slot::MappingValue,
        // //         indent: target_indent,
        // //     }),
        // //     BlockKind::Unknown => None,
        // // };
        // //
        // // write_fragment_placeholder_line(&mut out.buf, id, slot);
        //
        // // Sticky parent remembers the indent we actually used
        // match slot {
        //     Slot::MappingValue | Slot::SequenceItem => {
        //         out.last_fragment_parent = Some(FragmentParent {
        //             slot,
        //             indent: target_indent,
        //         });
        //     }
        //     Slot::Plain => {
        //         out.last_fragment_parent = None;
        //     }
        // }

        // record placeholder metadata (unchanged) ...
        out.placeholders.push(Placeholder {
            id,
            role: Role::Unknown,
            action_span: node.byte_range(),
            values: values.into_iter().collect(),
            is_fragment_output: true,
        });
        if force_fragment_role {
            out.force_fragment_role.insert(id);
        }
        return;

        // -------- 

        // let nin = extract_nindent_width(&node, src).unwrap_or(0);
        // let mut slot = current_slot_in_buf(&out.buf);
        // let before_indent = line_indent(&out.buf);
        //
        // // If we currently think we're "Plain", but we have a remembered parent at this indent,
        // // inherit it so successive fragment lines stay under the same mapping/sequence.
        // if matches!(slot, Slot::Plain) {
        //     if let Some(h) = out.last_fragment_parent {
        //         let target_indent = if nin > 0 { nin } else { before_indent };
        //         if h.indent == target_indent {
        //             slot = h.slot;
        //         }
        //     }
        // }
        //
        // // ✅ NEW logic for mapping-value sites
        // if matches!(slot, Slot::MappingValue) {
        //     // Use explicit nindent if present, else indent one YAML level under the key
        //     let target_indent = if nin > 0 { nin } else { before_indent + 2 };
        //     ensure_current_line_indent(&mut out.buf, target_indent);
        // } else if matches!(slot, Slot::Plain) && nin > 0 {
        //     // Plain site with explicit indentation → reflect it
        //     ensure_current_line_indent(&mut out.buf, nin);
        // }
        //
        // write_fragment_placeholder_line(&mut out.buf, id, slot);
        //
        // // Update sticky parent hint with the *same* indent we just used
        // match slot {
        //     Slot::MappingValue | Slot::SequenceItem => {
        //         let parent_indent = if nin > 0 { nin } else { before_indent + 2 }; // ← changed
        //         out.last_fragment_parent = Some(FragmentParent {
        //             slot,
        //             indent: parent_indent,
        //         });
        //     }
        //     Slot::Plain => {
        //         out.last_fragment_parent = None;
        //     }
        // }

        // // Decide slot with nindent awareness + sticky parent hint
        // let nin = extract_nindent_width(&node, src).unwrap_or(0);
        // let mut slot = current_slot_in_buf(&out.buf);
        // let before_indent = line_indent(&out.buf);
        //
        // // If we currently think we're "Plain", but we have a remembered parent at this indent,
        // // inherit it so successive fragment lines stay under the same mapping/sequence.
        // if matches!(slot, Slot::Plain) {
        //     if let Some(h) = out.last_fragment_parent {
        //         let target_indent = if nin > 0 { nin } else { before_indent };
        //         if h.indent == target_indent {
        //             slot = h.slot;
        //         }
        //     }
        // }
        //
        // // --- NEW: always break the line before emitting a fragment into a mapping value
        // // (and also when an explicit nindent/indent width is present).
        // let must_break_line = matches!(slot, Slot::MappingValue) || nin > 0;
        // if must_break_line && !out.buf.ends_with('\n') {
        //     out.buf.push('\n');
        // }
        //
        // // Apply indent *after* the break. If nin==0 at a mapping site, escape to Plain.
        // if matches!(slot, Slot::MappingValue) {
        //     if nin == 0 {
        //         slot = Slot::Plain; // treat nindent 0 as escaping to top-level
        //     } else {
        //         ensure_current_line_indent(&mut out.buf, nin);
        //     }
        // } else if matches!(slot, Slot::Plain) && nin > 0 {
        //     ensure_current_line_indent(&mut out.buf, nin);
        // }
        //
        // // Actually write the placeholder line for fragments
        // write_fragment_placeholder_line(&mut out.buf, id, slot);
        //
        // // Update sticky parent hint
        // match slot {
        //     Slot::MappingValue | Slot::SequenceItem => {
        //         let parent_indent = if nin > 0 { nin } else { before_indent };
        //         out.last_fragment_parent = Some(FragmentParent {
        //             slot,
        //             indent: parent_indent,
        //         });
        //     }
        //     Slot::Plain => out.last_fragment_parent = None,
        // }

        // // Decide slot with nindent awareness + sticky parent hint
        // let nin = extract_nindent_width(&node, src).unwrap_or(0);
        // let mut slot = current_slot_in_buf(&out.buf);
        // let before_indent = line_indent(&out.buf);
        //
        // // If we currently think we're "Plain", but we have a remembered parent at this indent,
        // // inherit it so successive fragment lines stay under the same mapping/sequence.
        // if matches!(slot, Slot::Plain) {
        //     if let Some(h) = out.last_fragment_parent {
        //         let target_indent = if nin > 0 { nin } else { before_indent };
        //         if h.indent == target_indent {
        //             slot = h.slot;
        //         }
        //     }
        // }
        //
        // // Apply indent effects for mapping sites when nindent > 0
        // if matches!(slot, Slot::MappingValue) {
        //     if nin == 0 {
        //         // nindent(0) escapes to top-level → treat as Plain
        //         slot = Slot::Plain;
        //     } else {
        //         ensure_current_line_indent(&mut out.buf, nin);
        //     }
        // } else if matches!(slot, Slot::Plain) && nin > 0 {
        //     // Plain site with explicit indentation → reflect it for YAML cleanliness
        //     ensure_current_line_indent(&mut out.buf, nin);
        // }
        //
        // // Actually write the placeholder line for fragments
        // write_fragment_placeholder_line(&mut out.buf, id, slot);
        //
        // // Update the sticky parent hint for subsequent fragments at the same indent
        // match slot {
        //     Slot::MappingValue | Slot::SequenceItem => {
        //         let parent_indent = if nin > 0 { nin } else { before_indent };
        //         out.last_fragment_parent = Some(FragmentParent {
        //             slot,
        //             indent: parent_indent,
        //         });
        //     }
        //     Slot::Plain => {
        //         out.last_fragment_parent = None;
        //     }
        // }

        // // Decide slot with nindent awareness
        // let nin = extract_nindent_width(&node, src).unwrap_or(0);
        // let mut slot = current_slot_in_buf(&out.buf);
        //
        // eprintln!(
        //     "[frag-ph] id={id} slot={:?} after='{}'",
        //     slot,
        //     last_non_empty_line(&out.buf).trim_end()
        // );
        //
        // // If previous non-empty line ended with ":" we *usually* are in the mapping value.
        // // But `nindent 0` escapes back to top-level → force Plain.
        // if matches!(slot, Slot::MappingValue) {
        //     if nin == 0 {
        //         slot = Slot::Plain;
        //     } else {
        //         // We remain under the mapping; make sure the line has at least `nin` spaces.
        //         ensure_current_line_indent(&mut out.buf, nin);
        //     }
        // } else if matches!(slot, Slot::Plain) && nin > 0 {
        //     // Even in "plain" sites (e.g. right after a newline), rendering will start
        //     // with `nin` spaces. Reflect it for YAML cleanliness.
        //     ensure_current_line_indent(&mut out.buf, nin);
        // }
        //
        // write_fragment_placeholder_line(&mut out.buf, id, slot);
    } else {
        // Scalar placeholders must end with a newline, or YAML can get glued together.
        out.buf.push('"');
        out.buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
        out.buf.push('"');
        // out.buf.push('\n'); // <<< important
    }

    out.placeholders.push(Placeholder {
        id,
        role: Role::Unknown,
        action_span: node.byte_range(),
        values: values.into_iter().collect(),
        is_fragment_output,
    });

    if force_fragment_role {
        out.force_fragment_role.insert(id);
    }
}

// Walk a template/define body and inline includes when appropriate.
pub fn inline_emit_tree(
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
        // Do not emit or traverse into {{ define }} bodies in the top-level pass.
        // We'll analyze define bodies only when they are included (we parse the body_src then).
        if container.kind() == "define_action" {
            return Ok(());
        }

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
                    let txt = &src[ch.byte_range()];
                    emit_text_and_update(out, txt);

                    // let txt = &src[ch.byte_range()];
                    // // NEW: keep parent container hint in sync with the text we just appended
                    // update_fragment_parent_from_text(out, txt);
                    // out.buf.push_str(txt);
                }
                // if !parent_is_define {
                //     out.buf.push_str(&src[ch.byte_range()]);
                // }
                continue;
            }

            // GUARD: record and continue (must use the *original* scope here)
            if is_guard_piece(&ch) {
                if container_is_cf {
                    // NEW: capture "$var := ..." inside guard so the body can resolve $var.something
                    if crate::sanitize::is_assignment_kind(ch.kind()) {
                        let vals = collect_values_following_includes(
                            ch, src, scope, inline, guard, &out.env,
                        );
                        // Bind $var -> set of bases
                        let mut vc = ch.walk();
                        for sub in ch.children(&mut vc) {
                            if sub.is_named() && sub.kind() == "variable" {
                                if let Some(name) = variable_ident_of(&sub, src) {
                                    out.env.insert(name, vals.clone());
                                }
                                break;
                            }
                        }
                        // Do not emit a placeholder for guard assignments.
                        // Fall through to emit the guard ValueUse rows once per container.
                    }

                    // NEW: bind "$k, $v := ..." for range guards
                    if ch.kind() == "range_variable_definition" {
                        // Collect the base(s) from the guard (e.g., ".Values.env" → {"env"})
                        let vals = collect_values_following_includes(
                            ch, src, scope, inline, guard, &out.env,
                        );
                        // Bind BOTH $k and $v to the same base set (env)
                        let mut vc = ch.walk();
                        for sub in ch.children(&mut vc) {
                            if sub.is_named() && sub.kind() == "variable" {
                                if let Some(name) = variable_ident_of(&sub, src) {
                                    out.env.insert(name, vals.clone());
                                }
                            }
                        }
                    }

                    if !guards_emitted_for_container {
                        let origins = guard_selectors_for_action(container, src, scope);

                        // Range vs With:
                        //  - range: emit ONLY the exact guard key; and if it's just '.', suppress entirely
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
                        ArgExpr::Selector(sel) => callee.dot = use_scope.resolve_selector(&sel),
                        ArgExpr::Dict(kvs) => {
                            for (k, v) in kvs {
                                let origin = match v {
                                    ArgExpr::Dot => use_scope.dot.clone(),
                                    ArgExpr::Dollar => use_scope.dollar.clone(),
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
                                /*force_fragment_output=*/ false,
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

                // Include at the head of a pipeline (e.g., {{ include "x" . | nindent 2 }})
                if ch.kind() == "chained_pipeline" {
                    if let Some(head_inc) = pipeline_head_is_include(ch, src) {
                        // Classify by the pipeline site, not just the head
                        let ctx = classify_action(src, &ch.byte_range());
                        let (tpl, arg) = parse_include_call(head_inc, src)?;

                        if guard.stack.iter().any(|n| n == &tpl) {
                            continue;
                        }
                        let entry = inline
                            .define_index
                            .get(&tpl)
                            .ok_or_else(|| IncludeError::UnknownDefine(tpl.clone()))?;

                        // Build callee scope using use_scope (same as the direct-include path)
                        let mut callee = Scope {
                            dot: use_scope.dot.clone(),
                            dollar: use_scope.dollar.clone(),
                            bindings: Default::default(),
                        };
                        match arg {
                            ArgExpr::Dot => callee.dot = use_scope.dot.clone(),
                            ArgExpr::Dollar => callee.dot = use_scope.dollar.clone(),
                            ArgExpr::Selector(sel) => callee.dot = use_scope.resolve_selector(&sel),
                            ArgExpr::Dict(kvs) => {
                                for (k, v) in kvs {
                                    let origin = match v {
                                        ArgExpr::Dot => use_scope.dot.clone(),
                                        ArgExpr::Dollar => use_scope.dollar.clone(),
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

                        let slot = current_slot_in_buf(&out.buf);
                        let has_indent = pipeline_contains_any(&ch, src, &["nindent", "indent"]);
                        let fragmentish_pipeline =
                            pipeline_contains_any(&ch, src, &["toYaml", "fromYaml"])
                                || (has_indent
                                    && pipeline_contains_any(&ch, src, &["tpl", "include"]));

                        // Treat as fragment when we're under a container slot and the pipeline is fragment-ish
                        // Also treat `fromYaml` as fragment even at top-level (bridges to a structure)
                        let force_fragment_here =
                            matches!(slot, Slot::MappingValue | Slot::SequenceItem)
                                && fragmentish_pipeline
                                || pipeline_contains_any(&ch, src, &["fromYaml"]);

                        match (ctx, force_fragment_here) {
                            (Role::ScalarValue, false) => {
                                // current SCALAR path (unchanged)
                                let mut tmp = InlineOut::default();
                                inline_emit_tree(
                                    &body.tree, body_src, &callee, inline, guard, &mut tmp,
                                )?;
                                let vals = collect_values_following_includes(
                                    body.tree.root_node(),
                                    body_src,
                                    &callee,
                                    inline,
                                    guard,
                                    &tmp.env,
                                );
                                write_placeholder(
                                    out, ch, src, vals, /*is_fragment_output=*/ false,
                                    /*force_fragment_role=*/ false,
                                );
                                out.guards.extend(tmp.guards.into_iter());
                            }
                            _ => {
                                // FRAGMENT path (reuse your existing non-ScalarValue branch)
                                let mut tmp = InlineOut::default();
                                inline_emit_tree(
                                    &body.tree, body_src, &callee, inline, guard, &mut tmp,
                                )?;
                                let vals = collect_values_following_includes(
                                    body.tree.root_node(),
                                    body_src,
                                    &callee,
                                    inline,
                                    guard,
                                    &tmp.env,
                                );
                                write_placeholder(
                                    out, ch, src, vals, /*is_fragment_output=*/ true,
                                    /*force_fragment_role=*/ false,
                                );
                                out.guards.extend(tmp.guards.into_iter());
                            } //     // Same 4 steps as the direct-include branch
                              //     let mut tmp = InlineOut::default();
                              //     inline_emit_tree(
                              //         &body.tree, body_src, &callee, inline, guard, &mut tmp,
                              //     )?;
                              //
                              //     let vals = collect_values_following_includes(
                              //         body.tree.root_node(),
                              //         body_src,
                              //         &callee,
                              //         inline,
                              //         guard,
                              //         &tmp.env,
                              //     );
                              //
                              //     write_placeholder(
                              //         out, ch, // <- note: use the pipeline node
                              //         src, vals, /*is_fragment_output=*/ false,
                              //         /*force_fragment_role=*/ false,
                              //     );
                              //
                              //     out.guards.extend(tmp.guards.into_iter());
                              // }
                              // _ => {
                              //     // Treat include|... in non-scalar position as a YAML fragment emission.
                              //     // We do NOT inline the callee body text here; we emit one fragment placeholder
                              //     // so nindent/indent is respected and yaml_path is correct.
                              //
                              //     // 1) Walk the callee into a temporary buffer to capture guards and $var bindings.
                              //     let mut tmp = InlineOut::default();
                              //     inline_emit_tree(
                              //         &body.tree, body_src, &callee, inline, guard, &mut tmp,
                              //     )?;
                              //
                              //     // 2) Collect the .Values.* origins from the callee (follow its nested includes)
                              //     let vals = collect_values_following_includes(
                              //         body.tree.root_node(),
                              //         body_src,
                              //         &callee,
                              //         inline,
                              //         guard,
                              //         &tmp.env,
                              //     );
                              //
                              //     // 3) Emit a single FRAGMENT placeholder at the pipeline node.
                              //     //    write_placeholder() will look at the current slot and apply nindent width.
                              //     write_placeholder(
                              //         out, ch, // use the pipeline node span
                              //         src, vals, /*is_fragment_output=*/ true,
                              //         /*force_fragment_role=*/ false,
                              //     );
                              //
                              //     // 4) Keep any guards we discovered while walking the callee
                              //     out.guards.extend(tmp.guards.into_iter());
                              // }
                        }
                        continue;
                    }
                }

                // Non-include output
                {
                    // Handle sprig ternary specially so the condition becomes a guard
                    if let Some(head) = pipeline_head(ch) {
                        if head.kind() == "function_call"
                            && function_name_of(&head, src).as_deref() == Some("ternary")
                        {
                            if let Some(args) = arg_list(&head) {
                                // Gather named args: ternary <a> <b> <cond>
                                let mut named: Vec<Node> = Vec::new();
                                let mut w = args.walk();
                                for a in args.named_children(&mut w) {
                                    named.push(a);
                                }
                                if named.len() >= 3 {
                                    let then_node = named[0];
                                    let else_node = named[1];
                                    let cond_node = named[2];

                                    // 1) The condition is a Guard
                                    let cond_vals = collect_values_with_scope(
                                        cond_node, src, use_scope, &out.env,
                                    );
                                    for v in cond_vals {
                                        if !v.is_empty() {
                                            out.guards.push(ValueUse {
                                                value_path: v,
                                                role: Role::Guard,
                                                action_span: ch.byte_range(),
                                                yaml_path: None,
                                            });
                                        }
                                    }

                                    // 2) Only branch value sources feed the placeholder's "values" set
                                    use std::collections::BTreeSet;
                                    let mut branch_vals: BTreeSet<String> = BTreeSet::new();
                                    for br in [then_node, else_node] {
                                        branch_vals.extend(collect_values_following_includes(
                                            br, src, use_scope, inline, guard, &out.env,
                                        ));
                                    }

                                    // 3) Keep your fragment/indent logic as-is
                                    let slot = current_slot_in_buf(&out.buf);
                                    let has_indent =
                                        pipeline_contains_any(&ch, src, &["nindent", "indent"]);
                                    let head_is_fragment_producer =
                                        pipeline_contains_any(&ch, src, &["toYaml", "fromYaml"])
                                            || pipeline_contains_any(&ch, src, &["tpl", "include"]);
                                    let fragmentish_pipeline =
                                        pipeline_contains_any(&ch, src, &["toYaml", "fromYaml"])
                                            || (has_indent
                                                && pipeline_contains_any(
                                                    &ch,
                                                    src,
                                                    &["tpl", "include"],
                                                ));
                                    let is_frag_output =
                                        (matches!(slot, Slot::MappingValue | Slot::SequenceItem)
                                            && fragmentish_pipeline)
                                            || (has_indent && head_is_fragment_producer);
                                    let force_fragment_role =
                                        pipeline_contains_any(&ch, src, &["fromYaml"]);

                                    write_placeholder(
                                        out,
                                        ch,
                                        src,
                                        branch_vals,
                                        is_frag_output,
                                        force_fragment_role,
                                    );
                                    continue; // <<< important: skip the generic path
                                }
                            }
                        }
                    }

                    // Compute the set of .Values sources feeding this action.
                    let vals: BTreeSet<String> = if let Some(head) = pipeline_head(ch) {
                        let head_keys = collect_values_with_scope(head, src, use_scope, &out.env);
                        if head_keys.len() == 1 {
                            head_keys
                        } else {
                            collect_values_following_includes(
                                ch, src, use_scope, inline, guard, &out.env,
                            )
                        }
                    } else {
                        collect_values_following_includes(
                            ch, src, use_scope, inline, guard, &out.env,
                        )
                    };

                    // Where are we about to write?
                    let slot = current_slot_in_buf(&out.buf);
                    let has_indent = pipeline_contains_any(&ch, src, &["nindent", "indent"]);

                    // Which kinds of heads tend to produce YAML fragments?
                    let head_is_fragment_producer =
                        pipeline_contains_any(&ch, src, &["toYaml", "fromYaml"])
                            || pipeline_contains_any(&ch, src, &["tpl", "include"]);

                    // Treat as fragment if:
                    //  - it's toYaml/fromYaml, OR
                    //  - it's tpl/include piped through (n)indent
                    let fragmentish_pipeline =
                        pipeline_contains_any(&ch, src, &["toYaml", "fromYaml"])
                            || (has_indent && pipeline_contains_any(&ch, src, &["tpl", "include"]));

                    // Slot-based decision with a fallback when (n)indent is present:
                    // Sometimes slot detection lags; if (n)indent is present and the head
                    // is a fragment producer, emit a fragment placeholder anyway.
                    let is_frag_output = (matches!(slot, Slot::MappingValue | Slot::SequenceItem)
                        && fragmentish_pipeline)
                        || (has_indent && head_is_fragment_producer);

                    // Force role=Fragment for fromYaml even if we kept an inline scalar placeholder.
                    let force_fragment_role = pipeline_contains_any(&ch, src, &["fromYaml"]);

                    write_placeholder(out, ch, src, vals, is_frag_output, force_fragment_role);
                    continue;
                }
            }

            // Assignments ($x := ...): record bindings (no placeholders)
            // Also handle the special case: $o := fromYaml .Values.rawYaml  → bind $o as OPAQUE and
            // record a Guard use for the consumed source(s).
            if crate::sanitize::is_assignment_kind(ch.kind()) {
                // Detect whether RHS contains a fromYaml() call
                let mut rhs_is_from_yaml = false;
                {
                    let mut w = ch.walk();
                    for sub in ch.children(&mut w) {
                        if sub.is_named() && sub.kind() == "function_call" {
                            if function_name_of(&sub, src).as_deref() == Some("fromYaml") {
                                rhs_is_from_yaml = true;
                                break;
                            }
                        }
                    }
                }

                // Collect value origins on RHS (includes fromYaml arg via our collector)
                let vals =
                    collect_values_following_includes(ch, src, use_scope, inline, guard, &out.env);

                // Bind the LHS variable
                let mut vc = ch.walk();
                for sub in ch.children(&mut vc) {
                    if sub.is_named() && sub.kind() == "variable" {
                        if let Some(name) = variable_ident_of(&sub, src) {
                            if rhs_is_from_yaml {
                                // OPAQUE binding: prevent $o.someField → rawYaml.someField propagation
                                out.env.insert(name, BTreeSet::new());

                                // Also *record* that the source was consumed (as a non-emitting use).
                                // We use Guard role (no yaml_path), which the tests accept.
                                for v in &vals {
                                    if !v.is_empty() {
                                        out.guards.push(ValueUse {
                                            value_path: v.clone(),
                                            role: Role::Guard,
                                            action_span: ch.byte_range(),
                                            yaml_path: None,
                                        });
                                    }
                                }
                            } else {
                                out.env.insert(name, vals.clone());
                            }
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
        dbg!(&uses);

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
        dbg!(&uses);

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
        dbg!(&uses);

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
        dbg!(&uses);

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
    fn include_pipeline_on_key_line_is_fragment() -> eyre::Result<()> {
        Builder::default().build();
        let root = VfsPath::new(vfs::MemoryFS::new());

        // helper renders a mapping fragment
        let helper = write(
            &root.join("_labels.tpl")?,
            indoc! {r#"
                {{- define "labels.helper" -}}
                extra: true
                {{- end -}}
            "#},
        )?;

        let src_path = write(
            &root.join("t.yaml")?,
            indoc! {r#"
                metadata:
                  labels: {{ include "labels.helper" . | nindent 4 }}
                    # static siblings must still parse
                    app.kubernetes.io/name: app
            "#},
        )?;

        let src = src_path.read_to_string()?;
        let define_index = collect_defines(&[helper])?;
        let parsed = parse_gotmpl_document(&src).ok_or_eyre("parse failed")?;
        let uses = analyze_template(&parsed.tree, &src, define_index)?;
        dbg!(&uses);
        assert_that!(uses, is_empty());
        // assert!(uses.iter().any(|u| u.role == Role::Fragment
        //     && u.value_path == "/* whatever the helper reads, or empty if none */"));
        Ok(())
    }

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
        dbg!(&uses);

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
    fn from_yaml_bridges_value_to_yaml_path() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            ingress: {{ fromYaml .Values.ingress }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = crate::analyze::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        dbg!(&uses);

        assert_that!(
            &uses,
            contains(matches_pattern!(ValueUse {
                role: eq(&Role::Fragment),
                value_path: eq("ingress"),
                yaml_path: some(displays_as(eq("ingress"))),
                ..
            }))
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
        dbg!(&uses);

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

#[cfg(test)]
mod scope_rebinding {
    use super::*;
    use color_eyre::eyre::{self, OptionExt, WrapErr};
    use helm_schema_template::parse::parse_gotmpl_document;
    use indoc::indoc;
    use std::collections::BTreeSet;
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
    fn dollar_rebinding_to_values_then_shadow_to_other_values() {
        Builder::default().build();

        // Simulate: {{$cfg := .Values.a}} ... {{$cfg := .Values.b}} ... {{$cfg.c}}
        let mut scope = root_scope();
        scope.bind_key(
            "cfg",
            ExprOrigin::Selector(vec!["Values".into(), "a".into()]),
        );
        // shadow with a new binding in a nested scope:
        let mut inner_scope = scope.clone();
        inner_scope.bind_key(
            "cfg",
            ExprOrigin::Selector(vec!["Values".into(), "b".into()]),
        );

        let got = inner_scope.resolve_selector(&segs(&["$cfg", "c"]));
        assert_that!(vp(&got), some(eq("b.c"))); // inner shadowing wins

        // outer scope still points to a.c
        let got_outer = scope.resolve_selector(&segs(&["$cfg", "c"]));
        assert_that!(vp(&got_outer), some(eq("a.c")));
    }

    #[test]
    fn rebinding_dot_with_with_action_body_changes_relative_selectors() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            {{- with .Values.server }}
            {{- $cfg := . }}
            # both `.port` and `$cfg.port` should resolve to server.port
            {{ .port }} {{ $cfg.port }}
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        // find `with_action`
        let with_node = {
            let mut stack = vec![parsed.tree.root_node()];
            let mut found = None;
            while let Some(n) = stack.pop() {
                if n.kind() == "with_action" {
                    found = Some(n);
                    break;
                }
                let mut w = n.walk();
                for ch in n.children(&mut w) {
                    if ch.is_named() {
                        stack.push(ch);
                    }
                }
            }
            found.ok_or_eyre("missing with_action")?
        };
        let scope = root_scope();
        let origins = super::guard_selectors_for_action(with_node, src, &scope);
        let base = origins
            .into_iter()
            .find_map(|o| {
                if matches!(o, ExprOrigin::Selector(_)) {
                    Some(o)
                } else {
                    None
                }
            })
            .ok_or_eyre("missing selector")?;

        // simulate `$cfg := .` inside the with-body
        let mut body_scope = root_scope();
        body_scope.dot = base.clone();
        body_scope.bind_key("cfg", base);

        // `.port`
        let got1 = body_scope.resolve_selector(&segs(&[".", "port"]));
        assert_that!(vp(&got1), some(eq("server.port")));
        // `$cfg.port`
        let got2 = body_scope.resolve_selector(&segs(&["$cfg", "port"]));
        assert_that!(vp(&got2), some(eq("server.port")));
        Ok(())
    }

    #[test]
    fn rebind_root_dollar_to_opaque_makes_future_uses_non_values() {
        Builder::default().build();
        let mut s = root_scope();
        // simulate `$ := dict` (often used in Helm helpers)
        s.dollar = ExprOrigin::Opaque;
        let got = s.resolve_selector(&segs(&["$", "Values", "x"]));
        assert_that!(vp(&got), none()); // once $ stops being .Values, it no longer yields a values path
    }

    #[test]
    fn range_kv_binding_prefers_element_for_value_paths() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            {{- range $i, $e := .Values.items }}
            {{ $e.name }}
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            unordered_elements_are![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("items"),
                    yaml_path: none(),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("items.name"),
                    ..
                })
            ]
        );
        Ok(())
    }

    #[test]
    fn range_assignment_with_body_dot_and_element_var_both_resolve() -> eyre::Result<()> {
        Builder::default().build();

        // Note that `.key` here is equivalent to `$v.key` (not `$k`)
        let src = indoc! {r#"
            {{- range $k, $v := .Values.env }}
            {{ .key }}: {{ $v.value }}
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        // we expect a guard on env, and scalar uses for env.key and env.value
        assert_that!(
            &uses,
            contains_each![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("env"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    // Accept Fragment until we tighten classification
                    role: any!(eq(&Role::MappingKey), eq(&Role::Fragment)),
                    value_path: eq("env.key"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("env.value"),
                    ..
                })
            ]
        );
        Ok(())
    }

    #[test]
    fn if_with_variable_assignment_tracks_both_binding_and_body() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            {{- if $x := .Values.feature }}
            {{ $x.enabled }}
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        // guard on `feature` from if, then scalar for `$x.enabled` → feature.enabled
        assert_that!(
            &uses,
            unordered_elements_are![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("feature"),
                    yaml_path: none(),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("feature.enabled"),
                    ..
                })
            ]
        );
        Ok(())
    }
}

#[cfg(test)]
mod pipelines_and_functions {
    use super::*;
    use color_eyre::eyre::{self, OptionExt, WrapErr};
    use helm_schema_template::parse::parse_gotmpl_document;
    use indoc::indoc;
    use test_util::prelude::*;

    #[test]
    fn default_collects_both_candidate_sources() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            metadata:
              name: {{ default .Values.nameOverride .Values.name }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        // We don't know which wins statically, so both should be tracked to the same YAML path.
        assert_that!(
            &uses,
            contains_each![
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("nameOverride"),
                    yaml_path: some(displays_as(eq("metadata.name"))),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("name"),
                    yaml_path: some(displays_as(eq("metadata.name"))),
                    ..
                })
            ]
        );
        Ok(())
    }

    #[test]
    fn coalesce_collects_all_args_that_are_values() -> eyre::Result<()> {
        Builder::default().build();
        // coalesce is a sprig func; for now we at least expect we collect all .Values.* arguments
        let src = indoc! {r#"
            image:
              tag: {{ coalesce .Values.image.tag .Values.appVersion .Values.fallbackTag }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains_each![
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("image.tag"),
                    yaml_path: some(displays_as(eq("image.tag"))),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("appVersion"),
                    yaml_path: some(displays_as(eq("image.tag"))),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("fallbackTag"),
                    yaml_path: some(displays_as(eq("image.tag"))),
                    ..
                })
            ]
        );
        Ok(())
    }

    #[test]
    fn index_with_nested_index_is_traced() -> eyre::Result<()> {
        Builder::default().build();
        // {{ index (index .Values "a") "b" }}  → a.b
        let src = r#"{{ index (index .Values "a") "b" }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        // find the single function_call (outer)
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
        let scope = Scope {
            dot: ExprOrigin::Selector(vec!["Values".into()]),
            dollar: ExprOrigin::Selector(vec!["Values".into()]),
            bindings: Default::default(),
        };
        let env: super::Env = Default::default();
        let vals = super::collect_values_with_scope(node, src, &scope, &env);
        dbg!(&vals);
        assert_that!(&vals, unordered_elements_are![eq(&"a.b")]);
        Ok(())
    }

    #[test]
    fn and_or_chains_collect_selectors_from_all_sides() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            {{- if and .Values.enable (or .Values.a .Values.b) }}
            {{ .Values.payload }}
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        // Guards on enable, a, b; scalar on payload
        assert_that!(
            &uses,
            contains_each![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("enable"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("a"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("b"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("payload"),
                    ..
                })
            ]
        );
        Ok(())
    }

    #[test]
    fn pipe_chain_preserves_source_value() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            spec:
              replicas: {{ .Values.replicas | int | max 1 }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains_each![matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("replicas"),
                yaml_path: some(displays_as(eq("spec.replicas"))),
                ..
            }),]
        );
        Ok(())
    }
}

#[cfg(test)]
mod define_include_integration {
    use super::*;
    use color_eyre::eyre::{self, OptionExt, WrapErr};
    use helm_schema_template::parse::parse_gotmpl_document;
    use indoc::indoc;
    use test_util::prelude::*;

    #[test]
    fn nested_include_passes_config_through_parent() -> eyre::Result<()> {
        Builder::default().build();
        let root = VfsPath::new(vfs::MemoryFS::new());

        // child reads .config.child
        let child = write(
            &root.join("_child.tpl")?,
            indoc! {r#"
                {{- define "child" -}}
                child: {{ .config.child }}
                {{- end -}}
            "#},
        )?;
        // parent passes .Values.parent as config to child
        let parent = write(
            &root.join("_parent.tpl")?,
            indoc! {r#"
                {{- define "parent" -}}
                {{ include "child" (dict "config" .Values.parent) }}
                {{- end -}}
            "#},
        )?;

        let define_index = collect_defines(&[child, parent])?;
        let src = r#"{{ include "parent" . }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, define_index)?;
        dbg!(&uses);

        assert_that!(
            &uses,
            contains_each![matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("parent.child"),
                ..
            })]
        );
        Ok(())
    }

    #[test]
    fn include_with_ctx_and_local_overrides_dot() -> eyre::Result<()> {
        Builder::default().build();
        let root = VfsPath::new(vfs::MemoryFS::new());

        // helper expects `.ctx` to be the chart root and `.config` to a subtree
        let helper = write(
            &root.join("_helper.tpl")?,
            indoc! {r#"
                {{- define "helper" -}}
                name: {{ include "fullname" .ctx }}
                leaf: {{ .config.leaf }}
                {{- end -}}
            "#},
        )?;
        let fullname = write(
            &root.join("_fullname.tpl")?,
            indoc! {r#"
                {{- define "fullname" -}}
                {{- if .Values.fullnameOverride -}}
                {{- .Values.fullnameOverride -}}
                {{- else -}}
                {{- printf "%s-%s" .Release.Name .Chart.Name -}}
                {{- end -}}
                {{- end -}}
            "#},
        )?;

        let define_index = collect_defines(&[helper, fullname])?;
        // include helper with (ctx=$, config=.Values.node)
        let src = r#"{{ include "helper" (dict "ctx" $ "config" .Values.node) }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, define_index)?;
        dbg!(&uses);

        // We care only about .Values.* surfaces:
        assert_that!(
            &uses,
            contains_each![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("fullnameOverride"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("node.leaf"),
                    ..
                })
            ]
        );
        Ok(())
    }

    #[test]
    fn include_chain_three_levels_deep_carries_bindings() -> eyre::Result<()> {
        Builder::default().build();
        let root = VfsPath::new(vfs::MemoryFS::new());

        let c = write(
            &root.join("_c.tpl")?,
            indoc! {r#"
                {{- define "C" -}}
                field: {{ .config.final }}
                {{- end -}}
            "#},
        )?;
        let b = write(
            &root.join("b.tpl")?,
            indoc! {r#"
                {{- define "B" -}}
                {{ include "C" (dict "config" .config.inner) }}
                {{- end -}}
            "#},
        )?;
        let a = write(
            &root.join("a.tpl")?,
            indoc! {r#"
                {{- define "A" -}}
                {{ include "B" (dict "config" .Values.A) }}
                {{- end -}}
            "#},
        )?;

        let define_index = collect_defines(&[a, b, c])?;
        let src = r#"{{ include "A" . }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, define_index)?;
        dbg!(&uses);

        assert_that!(
            &uses,
            contains_each![matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("A.inner.final"),
                ..
            })]
        );
        Ok(())
    }
}

#[cfg(test)]
mod guards_and_yaml_paths {
    use super::*;
    use color_eyre::eyre::{self, OptionExt};
    use helm_schema_template::parse::parse_gotmpl_document;
    use indoc::indoc;
    use test_util::prelude::*;

    #[test]
    fn haskey_if_emits_minimal_guard_on_container() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            {{- if hasKey .Values.labels "app" }}
            app.kubernetes.io/name: {{ .Values.labels.app }}
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        // at minimum we should see a guard on `labels` and a scalar on `labels.app`
        assert_that!(
            &uses,
            contains_each![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("labels"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("labels.app"),
                    ..
                })
            ]
        );
        Ok(())
    }

    #[test]
    fn fragment_block_to_yaml_is_recorded() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            metadata:
              labels:
                {{- with .Values.extraLabels }}
                {{- toYaml . | nindent 8 }}
                {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains_each![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("extraLabels"),
                    yaml_path: none(),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::Fragment),
                    value_path: eq("extraLabels"),
                    yaml_path: some(displays_as(eq("metadata.labels"))),
                    ..
                })
            ]
        );
        Ok(())
    }

    #[test]
    fn list_index_or_star_is_normalized_for_first_item() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            spec:
              accessModes:
                {{- range .Values.pvc.accessModes }}
                - {{ . }}
                {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        dbg!(&uses);
        // Expect a Guard on pvc and pvc.accessModes and a single ScalarValue for at least one list item.
        assert_that!(
            &uses,
            contains_each![
                // matches_pattern!(ValueUse {
                //     role: eq(&Role::Guard),
                //     value_path: eq("pvc"),
                //     ..
                // }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("pvc.accessModes"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("pvc.accessModes"),
                    yaml_path: some(displays_as(any![
                        eq("spec.accessModes[0]"),
                        eq("spec.accessModes[*]")
                    ])),
                    ..
                }),
            ]
        );
        Ok(())
    }

    #[test]
    fn nested_with_inside_if_emits_both_guards() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            {{- if .Values.ingress }}
            {{- with .Values.ingress.tls }}
            secretName: {{ .secretName }}
            {{- end }}
            {{- end }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains_each![
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("ingress"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::Guard),
                    value_path: eq("ingress.tls"),
                    ..
                }),
                matches_pattern!(ValueUse {
                    role: eq(&Role::ScalarValue),
                    value_path: eq("ingress.tls.secretName"),
                    ..
                })
            ]
        );
        Ok(())
    }
}

#[cfg(test)]
mod regressions {
    use super::*;
    use color_eyre::eyre::{self, OptionExt};
    use helm_schema_template::parse::parse_gotmpl_document;
    use indoc::indoc;
    use test_util::prelude::*;

    #[test]
    fn required_function_still_collects_underlying_selector() -> eyre::Result<()> {
        Builder::default().build();
        let src = indoc! {r#"
            image:
              repository: {{ required "repo is required" .Values.image.repository }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains_each![matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("image.repository"),
                yaml_path: some(displays_as(eq("image.repository"))),
                ..
            })]
        );
        Ok(())
    }

    #[test]
    fn tpl_on_values_string_minimally_records_value_origin() -> eyre::Result<()> {
        Builder::default().build();
        // Even if we don't expand tpl deeply yet, we should (at least) record the source value that feeds tpl.
        let src = indoc! {r#"
            {{ tpl .Values.rawTemplate $ }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains_each![matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("rawTemplate"),
                ..
            })]
        );
        Ok(())
    }

    #[test]
    fn index_then_get_field_after_binding() -> eyre::Result<()> {
        Builder::default().build();
        // {{$cfg := index .Values "server"}} {{ $cfg.port }}
        let src = indoc! {r#"
            {{- $cfg := index .Values "server" -}}
            listen: {{ $cfg.port }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        // We want to see "server.port" end up as a ScalarValue
        assert_that!(
            &uses,
            contains_each![matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("server.port"),
                yaml_path: some(displays_as(eq("listen"))),
                ..
            })]
        );
        Ok(())
    }

    #[test]
    fn pluck_then_first_is_traced_to_key() -> eyre::Result<()> {
        Builder::default().build();
        // {{ (pluck "nodeSelector" .Values.podSpec | first) | toYaml }}
        // Ambitious: we expect at least `.Values.podSpec.nodeSelector` to be registered
        let src = indoc! {r#"
            {{- (pluck "nodeSelector" .Values.podSpec | first) | toYaml }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains_each![matches_pattern!(ValueUse {
                role: any!(eq(&Role::Fragment), eq(&Role::ScalarValue)),
                value_path: eq("podSpec.nodeSelector"),
                ..
            })]
        );
        Ok(())
    }

    #[test]
    fn get_function_with_default_like_pattern() -> eyre::Result<()> {
        Builder::default().build();
        // {{ default "Always" (get .Values.image "pullPolicy") }} → image.pullPolicy collected
        let src = indoc! {r#"
            imagePullPolicy: {{ default "Always" (get .Values.image "pullPolicy") }}
        "#};
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        assert_that!(
            &uses,
            contains_each![matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                value_path: eq("image.pullPolicy"),
                yaml_path: some(displays_as(eq("imagePullPolicy"))),
                ..
            })]
        );
        Ok(())
    }

    #[test]
    fn dollar_root_field_should_not_be_treated_as_named_variable() -> eyre::Result<()> {
        Builder::default().build();
        let root = VfsPath::new(vfs::MemoryFS::new());

        let src_path = write(
            &root.join("template.yaml")?,
            indoc! {r#"
                {{- define "u" -}}
                {{- end -}}

                {{- define "t" -}}
                {{- $x := dict "k" 1 -}}
                {{- include "u" (dict "scope" $.scope "context" $.context) -}}
                {{- end -}}
            "#},
        )?;
        let src = src_path.read_to_string()?;
        let parsed = parse_gotmpl_document(&src).ok_or_eyre("parse failed")?;
        let define_index = collect_defines(&[src_path])?;
        let uses = super::analyze_template(&parsed.tree, &src, define_index)?;
        dbg!(&uses);
        assert_that!(&uses, is_empty());
        // Walk to the (dict ...) and ensure $.scope is parsed as a selector rooted at "."
        // and, importantly, the resolver does NOT panic.
        // Depending on your API, assert we got an ArgExpr::Dict with a "scope" key and
        // the value is either a Selector starting at "." or Opaque — but no panic.
        Ok(())
    }

    #[test]
    fn dollar_root_in_if_does_not_panic() -> eyre::Result<()> {
        Builder::default().build();
        let src = r#"{{- if ($.Chart).AppVersion }}X{{- end }}"#;
        let parsed = parse_gotmpl_document(src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, src, DefineIndex::default())?;
        dbg!(&uses);
        assert_that!(&uses, is_empty());
        Ok(())
    }

    #[test]
    fn dollar_root_survives_through_include_scope() -> eyre::Result<()> {
        Builder::default().build();
        let root = VfsPath::new(vfs::MemoryFS::new());

        let src_path = write(
            &root.join("template.yaml")?,
            indoc! {r#"
                {{- define "t" -}}
                {{- if ($.Release).Name }}ok{{- end }}
                {{- end -}}
                {{ include "t" . }}
            "#},
        )?;

        let src = src_path.read_to_string()?;
        let define_index = collect_defines(&[src_path])?;
        let parsed = parse_gotmpl_document(&src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, &src, define_index)?;
        dbg!(&uses);
        assert_that!(&uses, is_empty());
        Ok(())
    }
}

#[cfg(test)]
mod nindent_parenting {
    use super::*;
    use crate::analyze::{Role, ValueUse};
    use color_eyre::eyre::{self, OptionExt};
    use indoc::indoc;
    use test_util::prelude::*;

    /// helper: pretty display of yaml_path for debugging
    fn path_display(p: &Option<YamlPath>) -> String {
        p.as_ref().map(|pp| pp.to_string()).unwrap_or_default()
    }

    #[test]
    fn nindent_2_places_fragment_under_parent_even_with_zero_ws() -> eyre::Result<()> {
        Builder::default().build();
        let root = VfsPath::new(vfs::MemoryFS::new());

        // `helper` renders YAML under `.Values.foo`
        // `top` renders YAML under `.Values.tl`
        // The first include is on the next line after `parent:`, but with no spaces in the template.
        // nindent(2) must make it land *under* `parent`.
        let src_path = write(
            &root.join("template.yaml")?,
            indoc! {r#"
                {{- define "helper" -}}
                foo: {{ .Values.foo }}
                {{- end -}}
                {{- define "top" -}}
                tl: {{ .Values.tl }}
                {{- end -}}

                parent:
                {{ include "helper" . | nindent 2 }}
                {{ include "top" . | nindent 0 }}
            "#},
        )?;

        let src = src_path.read_to_string()?;
        let define_index = collect_defines(&[src_path])?;
        let parsed = parse_gotmpl_document(&src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, &src, define_index)?;
        dbg!(&uses);

        // Expect `.Values.foo` to be attributed to yaml_path == "parent" as a Fragment
        let mut saw_parent = false;
        let mut saw_top = false;

        for u in &uses {
            if u.value_path == "foo" {
                assert_eq!(
                    u.role,
                    Role::Fragment,
                    "foo should be a fragment landing under a mapping slot"
                );
                assert_eq!(
                    path_display(&u.yaml_path),
                    "parent",
                    "foo should map to the 'parent' key"
                );
                saw_parent = true;
            }
            if u.value_path == "tl" {
                // This include is `nindent 0` at top-level; yaml_path can be empty/root.
                assert_eq!(u.role, Role::Fragment);
                assert!(
                    u.yaml_path.is_none() || path_display(&u.yaml_path).is_empty(),
                    "tl should map to the document root (top-level)"
                );
                saw_top = true;
            }
        }

        assert!(
            saw_parent,
            "did not see ValueUse for .Values.foo under parent"
        );
        assert!(saw_top, "did not see ValueUse for .Values.tl at top-level");
        Ok(())
    }

    #[test]
    fn two_successive_fragments_stay_under_same_parent_via_placeholder_heuristic()
    -> eyre::Result<()> {
        Builder::default().build();
        let root = VfsPath::new(vfs::MemoryFS::new());

        // This specifically exercises the `current_slot_in_buf` heuristic that treats
        // a previous placeholder mapping entry as keeping us in a mapping.
        let src_path = write(
            &root.join("template.yaml")?,
            indoc! {r#"
                {{- define "a" -}}
                a1: {{ .Values.a1 }}
                {{- end -}}
                {{- define "b" -}}
                b1: {{ .Values.b1 }}
                {{- end -}}

                parent:
                {{ include "a" . | nindent 2 }}
                {{ include "b" . | nindent 2 }}
            "#},
        )?;

        let src = src_path.read_to_string()?;
        let define_index = collect_defines(&[src_path])?;
        let parsed = parse_gotmpl_document(&src).ok_or_eyre("parse failed")?;
        let uses = super::analyze_template(&parsed.tree, &src, define_index)?;
        dbg!(&uses);

        let mut seen_a = false;
        let mut seen_b = false;
        for u in &uses {
            if u.value_path == "a1" {
                assert_eq!(u.role, Role::Fragment);
                assert_eq!(
                    u.yaml_path.as_ref().map(|p| p.to_string()).as_deref(),
                    Some("parent")
                );
                seen_a = true;
            }
            if u.value_path == "b1" {
                assert_eq!(u.role, Role::Fragment);
                assert_eq!(
                    u.yaml_path.as_ref().map(|p| p.to_string()).as_deref(),
                    Some("parent")
                );
                seen_b = true;
            }
        }
        assert!(
            seen_a && seen_b,
            "expected both .Values.a1 and .Values.b1 under 'parent'"
        );
        Ok(())
    }
}
