use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use helm_schema_ast::{DefineIndex, DefineSourceRole, TemplateExpr, parse_go_template};

use crate::abstract_value::AbstractValue;
use crate::symbolic::SymbolicPolicy;

pub(crate) struct ParsedHelperBody<'a> {
    pub(crate) source: &'a str,
    pub(crate) source_path: &'a str,
    pub(crate) body_offset: usize,
    pub(crate) tree: &'a tree_sitter::Tree,
    pub(crate) expressions: &'a [TemplateExpr],
}

/// Shared parsed helper programs for one immutable define index.
///
/// Define boundaries are discovered once. Each body tree and flattened
/// expression list remains lazy so unused helpers pay no body-parse cost.
#[derive(Clone)]
pub struct ParsedDefines {
    inner: Rc<ParsedDefinesInner>,
}

struct ParsedDefinesInner {
    define_bodies: HashMap<String, CachedDefineBody>,
    define_names: BTreeSet<String>,
    define_source_paths: BTreeMap<String, BTreeSet<String>>,
    file_sources: HashMap<String, String>,
}

impl ParsedDefines {
    /// Discovers define boundaries and prepares lazy per-body parse cells.
    #[must_use]
    pub fn new(defines: &DefineIndex) -> Self {
        let mut define_bodies = HashMap::new();
        let mut define_names = BTreeSet::new();
        let mut define_source_paths: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut file_sources = HashMap::new();
        for (path, role, src) in defines.file_sources() {
            file_sources.insert(path.to_string(), src.to_string());
            // A template is callable under the exact name Helm registers it
            // by, which is the index key. `.Files.Get` payloads share the
            // map but are never executable templates.
            if role == DefineSourceRole::Template {
                define_bodies.insert(path.to_string(), CachedDefineBody::new(src, path, 0));
            }
            for block in extract_define_blocks(src) {
                define_names.insert(block.name.clone());
                define_source_paths
                    .entry(block.name.clone())
                    .or_default()
                    .insert(path.to_string());
                define_bodies.insert(
                    block.name,
                    CachedDefineBody::new(&block.body, path, block.body_offset),
                );
            }
        }
        Self {
            inner: Rc::new(ParsedDefinesInner {
                define_bodies,
                define_names,
                define_source_paths,
                file_sources,
            }),
        }
    }

    /// Iterates real define names in stable order, excluding implicit files.
    pub fn define_names(&self) -> impl Iterator<Item = &str> {
        self.inner.define_names.iter().map(String::as_str)
    }

    /// Returns the last-defined body Helm resolves for `name`.
    #[must_use]
    pub fn define_body(&self, name: &str) -> Option<&str> {
        self.inner
            .define_bodies
            .get(name)
            .map(|body| body.source.as_str())
    }

    /// Iterates every source path that defines `name` in stable order.
    pub fn define_source_paths(&self, name: &str) -> impl Iterator<Item = &str> {
        self.inner
            .define_source_paths
            .get(name)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    fn parsed_body(&self, name: &str) -> Option<ParsedHelperBody<'_>> {
        let body = self.inner.define_bodies.get(name)?;
        Some(ParsedHelperBody {
            source: body.source.as_str(),
            source_path: body.source_path.as_str(),
            body_offset: body.body_offset,
            tree: body.tree()?,
            expressions: body.expressions(),
        })
    }
}

pub(crate) struct IrAnalysisDb {
    parsed_defines: ParsedDefines,
    chart_default_strings: BTreeMap<String, String>,
    predicate_memo: Rc<helm_schema_core::PredicateMemo>,
    /// Initial Helm root fields supplied by render policy.
    static_root_fields: HashMap<String, AbstractValue>,
}

fn static_root_fields(strings: BTreeMap<Vec<String>, String>) -> HashMap<String, AbstractValue> {
    let mut roots = BTreeMap::new();
    for (path, value) in strings {
        insert_static_root_string(&mut roots, &path, value);
    }
    roots.into_iter().collect()
}

fn insert_kubernetes_version_fields(strings: &mut BTreeMap<Vec<String>, String>, version: &str) {
    let version = version.trim_start_matches('v');
    let helm_version = format!("v{version}");
    for field in ["Version", "GitVersion"] {
        strings.insert(
            vec![
                "Capabilities".to_string(),
                "KubeVersion".to_string(),
                field.to_string(),
            ],
            helm_version.clone(),
        );
    }

    let mut components = version.split('.');
    for field in ["Major", "Minor"] {
        let Some(component) = components.next() else {
            break;
        };
        strings.insert(
            vec![
                "Capabilities".to_string(),
                "KubeVersion".to_string(),
                field.to_string(),
            ],
            component.to_string(),
        );
    }
}

fn insert_static_root_string(
    fields: &mut BTreeMap<String, AbstractValue>,
    path: &[String],
    value: String,
) {
    let Some((head, tail)) = path.split_first() else {
        return;
    };
    if tail.is_empty() {
        fields.insert(
            head.clone(),
            AbstractValue::StringSet(BTreeSet::from([value])),
        );
        return;
    }
    let entry = fields
        .entry(head.clone())
        .or_insert_with(|| AbstractValue::Dict(BTreeMap::new()));
    if let AbstractValue::Dict(nested) = entry {
        insert_static_root_string(nested, tail, value);
    }
}

impl IrAnalysisDb {
    #[tracing::instrument(skip_all)]
    pub(crate) fn new(defines: &DefineIndex) -> Self {
        Self::with_policy(defines, SymbolicPolicy::default())
    }

    pub(crate) fn with_policy(defines: &DefineIndex, policy: SymbolicPolicy) -> Self {
        Self::with_parsed_policy(&ParsedDefines::new(defines), policy)
    }

    pub(crate) fn with_parsed_policy(
        parsed_defines: &ParsedDefines,
        policy: SymbolicPolicy,
    ) -> Self {
        let SymbolicPolicy {
            chart_default_strings,
            kubernetes_version,
            mut static_root_strings,
        } = policy;
        if let Some(version) = kubernetes_version.as_deref() {
            insert_kubernetes_version_fields(&mut static_root_strings, version);
        }
        Self {
            parsed_defines: parsed_defines.clone(),
            chart_default_strings,
            predicate_memo: Rc::new(helm_schema_core::PredicateMemo::new()),
            static_root_fields: static_root_fields(static_root_strings),
        }
    }

    pub(crate) fn predicate_memo(&self) -> &Rc<helm_schema_core::PredicateMemo> {
        &self.predicate_memo
    }

    pub(crate) fn static_root_fields(&self) -> &HashMap<String, AbstractValue> {
        &self.static_root_fields
    }

    pub(crate) fn has_helper(&self, name: &str) -> bool {
        self.parsed_defines.inner.define_bodies.contains_key(name)
    }

    pub(crate) fn file_source(&self, path: &str) -> Option<&str> {
        self.parsed_defines
            .inner
            .file_sources
            .get(path)
            .map(String::as_str)
    }

    /// Chart-authored defaults that one exact or wildcard values provenance
    /// can select at a `tpl` boundary.
    ///
    /// A wildcard names one mapping member at that depth. Enumerating its
    /// finite matching defaults recovers the exact programs Helm executes
    /// through ranged configuration maps while retaining each concrete
    /// source path for selection guards.
    pub(crate) fn chart_default_programs_matching<'a>(
        &'a self,
        path_pattern: &str,
    ) -> Vec<(&'a str, &'a str)> {
        let pattern = helm_schema_core::split_value_path(path_pattern);
        let has_wildcard = pattern.iter().any(|segment| segment == "*");
        self.chart_default_strings
            .iter()
            .filter_map(|(path, value)| {
                let segments = helm_schema_core::split_value_path(path);
                let matches = segments.len() == pattern.len()
                    && segments
                        .iter()
                        .zip(&pattern)
                        .all(|(segment, expected)| expected == "*" || segment == expected);
                if !matches
                    || (has_wildcard
                        && !matches!(helm_schema_ast::contains_template_action(value), Ok(true)))
                {
                    return None;
                }
                Some((path.as_str(), value.as_str()))
            })
            .collect()
    }

    /// Indexed chart file paths (templates plus `.Files.Get` sources),
    /// sorted for deterministic enumeration.
    pub(crate) fn file_source_paths(&self) -> Vec<&str> {
        let mut paths: Vec<&str> = self
            .parsed_defines
            .inner
            .file_sources
            .keys()
            .map(String::as_str)
            .collect();
        paths.sort_unstable();
        paths
    }

    pub(crate) fn parsed_helper_body(&self, name: &str) -> Option<ParsedHelperBody<'_>> {
        self.parsed_defines.parsed_body(name)
    }
}

struct CachedDefineBody {
    source: String,
    source_path: String,
    body_offset: usize,
    tree: OnceCell<Option<tree_sitter::Tree>>,
    expressions: OnceCell<Vec<TemplateExpr>>,
}

impl CachedDefineBody {
    fn new(source: &str, source_path: &str, body_offset: usize) -> Self {
        Self {
            source: source.to_string(),
            source_path: source_path.to_string(),
            body_offset,
            tree: OnceCell::new(),
            expressions: OnceCell::new(),
        }
    }

    fn tree(&self) -> Option<&tree_sitter::Tree> {
        self.tree
            .get_or_init(|| parse_go_template(&self.source))
            .as_ref()
    }

    fn expressions(&self) -> &[TemplateExpr] {
        self.expressions.get_or_init(|| {
            self.tree()
                .map(|tree| helm_schema_ast::node_expressions(tree.root_node(), &self.source))
                .unwrap_or_default()
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DefineBlock {
    name: String,
    body: String,
    body_offset: usize,
}

fn extract_define_blocks(src: &str) -> Vec<DefineBlock> {
    let Some(tree) = parse_go_template(src) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    collect_define_blocks(tree.root_node(), src, &mut out);
    out.sort_by_key(|block| block.body_offset);
    out
}

fn collect_define_blocks(node: tree_sitter::Node<'_>, src: &str, out: &mut Vec<DefineBlock>) {
    if node.kind() == "define_action"
        && let Some(block) = define_block_from_node(node, src)
    {
        out.push(block);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_define_blocks(child, src, out);
    }
}

fn define_block_from_node(node: tree_sitter::Node<'_>, src: &str) -> Option<DefineBlock> {
    let name = define_name(node, src)?;
    let body_children = children_with_field(node, "body");
    let end_action_start = find_end_action_start(node);

    let body_end = end_action_start.unwrap_or_else(|| {
        body_children
            .last()
            .map(tree_sitter::Node::end_byte)
            .unwrap_or_else(|| node.end_byte())
    });
    let body_start = body_children
        .first()
        .map(tree_sitter::Node::start_byte)
        .unwrap_or(body_end);
    let body_range = body_start..body_end;
    let body = src.get(body_range.clone())?.to_string();

    Some(DefineBlock {
        name,
        body,
        body_offset: body_range.start,
    })
}

fn define_name(node: tree_sitter::Node<'_>, src: &str) -> Option<String> {
    let raw = node
        .child_by_field_name("name")?
        .utf8_text(src.as_bytes())
        .ok()?
        .trim();
    let quoted = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            raw.strip_prefix('`')
                .and_then(|rest| rest.strip_suffix('`'))
        })
        .or_else(|| {
            raw.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(raw)
        .trim();
    if quoted.is_empty() {
        return None;
    }
    Some(quoted.to_string())
}

fn find_end_action_start(node: tree_sitter::Node<'_>) -> Option<usize> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == "end_action")
        .map(|child| child.start_byte())
}

fn children_with_field<'node>(
    node: tree_sitter::Node<'node>,
    field: &str,
) -> Vec<tree_sitter::Node<'node>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor).collect()
}

#[cfg(test)]
#[path = "tests/analysis_db.rs"]
mod tests;

#[cfg(test)]
pub(crate) use tests::{BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT, widen_large_bound_value_ref};
