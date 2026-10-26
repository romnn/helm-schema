use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use helm_schema_ast::{
    DefineIndex, DefineSourceRole, ParsedActions, TemplateExpr, parse_go_template,
};
use helm_schema_syntax::{TemplateAction, TemplatedDocument};

use crate::symbolic::SymbolicPolicy;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ParsedProgramId {
    ordinal: u64,
}

pub(crate) struct ParsedProgram {
    id: ParsedProgramId,
    source: String,
    source_path: Option<String>,
    body_offset: usize,
    syntax: OnceCell<Option<ParsedProgramSyntax>>,
}

impl std::fmt::Debug for ParsedProgram {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ParsedProgram")
            .field("id", &self.id)
            .field("source_path", &self.source_path)
            .field("body_offset", &self.body_offset)
            .finish_non_exhaustive()
    }
}

pub(crate) struct ParsedProgramSyntax {
    pub(crate) tree: tree_sitter::Tree,
    pub(crate) actions: Vec<TemplateAction>,
    pub(crate) expressions: ParsedActions,
    pub(crate) block_calls: BTreeMap<usize, TemplateExpr>,
}

impl ParsedProgram {
    pub(crate) fn id(&self) -> ParsedProgramId {
        self.id
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }

    pub(crate) fn body_offset(&self) -> usize {
        self.body_offset
    }

    pub(crate) fn syntax(&self) -> Option<&ParsedProgramSyntax> {
        self.syntax
            .get_or_init(|| {
                parse_go_template(&self.source).and_then(|tree| program_syntax(tree, &self.source))
            })
            .as_ref()
    }
}

fn program_syntax(tree: tree_sitter::Tree, source: &str) -> Option<ParsedProgramSyntax> {
    if tree.root_node().has_error() {
        return None;
    }
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    let actions = document.actions().to_vec();
    let expressions = ParsedActions::lower(tree.root_node(), source, &actions);
    let mut block_calls = BTreeMap::new();
    collect_block_calls(tree.root_node(), source, &mut block_calls);
    Some(ParsedProgramSyntax {
        tree,
        actions,
        expressions,
        block_calls,
    })
}

fn collect_block_calls(
    node: tree_sitter::Node<'_>,
    source: &str,
    calls: &mut BTreeMap<usize, TemplateExpr>,
) {
    if node.kind() == "block_action"
        && let Some(name) = define_name(node, source)
    {
        let argument = node
            .child_by_field_name("argument")
            .map(|argument| helm_schema_ast::node_expressions(argument, source));
        if let Some(arguments) = argument
            && let [argument] = arguments.as_slice()
        {
            calls.insert(
                node.start_byte(),
                TemplateExpr::Call {
                    function: "template".to_string(),
                    args: vec![
                        TemplateExpr::Literal(helm_schema_ast::Literal::String(name)),
                        argument.clone(),
                    ],
                },
            );
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_block_calls(child, source, calls);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DefinitionInventory {
    Complete,
    Unknown,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedDefinition {
    pub(crate) name: String,
    pub(crate) program: Rc<ParsedProgram>,
    pub(crate) syntactically_empty: bool,
}

#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum DefinitionError {
    #[error("multiple definition of template {name:?}")]
    Duplicate { name: String },
}

#[derive(Debug)]
pub(crate) struct ParsedTemplateSource {
    pub(crate) name: String,
    pub(crate) root: Rc<ParsedProgram>,
    pub(crate) root_syntactically_empty: bool,
    pub(crate) declarations: Vec<ParsedDefinition>,
    pub(crate) inventory: DefinitionInventory,
    pub(crate) error: Option<DefinitionError>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SourceKey {
    source: String,
    source_path: Option<String>,
    root_name: String,
}

#[derive(Default)]
struct ProgramCatalog {
    next_program: u64,
    sources: BTreeMap<SourceKey, Rc<ParsedTemplateSource>>,
}

impl ProgramCatalog {
    fn program(
        &mut self,
        source: &str,
        source_path: Option<&str>,
        body_offset: usize,
    ) -> Rc<ParsedProgram> {
        let id = ParsedProgramId {
            ordinal: self.next_program,
        };
        self.next_program += 1;
        Rc::new(ParsedProgram {
            id,
            source: source.to_string(),
            source_path: source_path.map(str::to_string),
            body_offset,
            syntax: OnceCell::new(),
        })
    }

    fn source(
        &mut self,
        source: &str,
        source_path: Option<&str>,
        root_name: &str,
    ) -> Rc<ParsedTemplateSource> {
        let key = SourceKey {
            source: source.to_string(),
            source_path: source_path.map(str::to_string),
            root_name: root_name.to_string(),
        };
        if let Some(source) = self.sources.get(&key) {
            return Rc::clone(source);
        }
        let tree = parse_go_template(source);
        let mut blocks = Vec::new();
        let inventory_complete = tree.as_ref().is_some_and(|tree| {
            !tree.root_node().has_error() && {
                let document = TemplatedDocument::parse_with_root(source, tree.root_node());
                collect_define_blocks(tree.root_node(), source, document.actions(), &mut blocks)
            }
        });
        blocks.sort_by_key(|block| block.body_offset);
        let root_syntactically_empty = tree.as_ref().is_some_and(|tree| {
            let mut cursor = tree.root_node().walk();
            tree.root_node().named_children(&mut cursor).all(|node| {
                node.kind() == "define_action" || syntactically_empty_node(node, source)
            })
        });
        let root = self.program(source, source_path, 0);
        root.syntax
            .get_or_init(|| tree.and_then(|tree| program_syntax(tree, source)));
        let declarations: Vec<_> = blocks
            .into_iter()
            .map(|block| {
                let DefineBlock {
                    name,
                    body,
                    body_offset,
                    syntactically_empty,
                } = block;
                ParsedDefinition {
                    name,
                    program: self.program(&body, source_path, body_offset),
                    syntactically_empty,
                }
            })
            .collect();
        let inventory = if inventory_complete {
            DefinitionInventory::Complete
        } else {
            DefinitionInventory::Unknown
        };
        let parsed = Rc::new(associate_definitions(
            root_name,
            root,
            root_syntactically_empty,
            declarations,
            inventory,
        ));
        self.sources.insert(key, Rc::clone(&parsed));
        parsed
    }
}

fn associate_definitions(
    name: &str,
    root: Rc<ParsedProgram>,
    root_syntactically_empty: bool,
    declarations: Vec<ParsedDefinition>,
    inventory: DefinitionInventory,
) -> ParsedTemplateSource {
    let mut nonempty = BTreeSet::new();
    if !root_syntactically_empty {
        nonempty.insert(name.to_string());
    }
    let mut error = None;
    if inventory == DefinitionInventory::Complete {
        for definition in &declarations {
            if !definition.syntactically_empty && !nonempty.insert(definition.name.clone()) {
                error = Some(DefinitionError::Duplicate {
                    name: definition.name.clone(),
                });
                break;
            }
        }
    }
    let mut local_definitions: BTreeMap<String, ParsedDefinition> = BTreeMap::new();
    for definition in declarations {
        let replace = local_definitions
            .get(&definition.name)
            .is_none_or(|existing| existing.syntactically_empty || !definition.syntactically_empty);
        if replace {
            local_definitions.insert(definition.name.clone(), definition);
        }
    }
    // Within one parse, the root is associated after its named definitions.
    let parsed_root = local_definitions
        .get(name)
        .filter(|definition| !definition.syntactically_empty && root_syntactically_empty)
        .cloned()
        .unwrap_or_else(|| ParsedDefinition {
            name: name.to_string(),
            program: root,
            syntactically_empty: root_syntactically_empty,
        });
    if local_definitions.contains_key(name) {
        local_definitions.insert(name.to_string(), parsed_root.clone());
    }
    ParsedTemplateSource {
        name: name.to_string(),
        root: parsed_root.program,
        root_syntactically_empty: parsed_root.syntactically_empty,
        declarations: local_definitions.into_values().collect(),
        inventory,
        error,
    }
}

/// Shared source catalog; runtime namespaces alone choose active definitions.
#[derive(Clone)]
pub struct ParsedDefines {
    inner: Rc<ParsedDefinesInner>,
}

struct ParsedDefinesInner {
    template_sources: Vec<Rc<ParsedTemplateSource>>,
    file_sources: HashMap<String, String>,
    catalog: RefCell<ProgramCatalog>,
}

impl ParsedDefines {
    /// Parses template declarations while keeping static file payloads outside the namespace.
    #[must_use]
    pub fn new(defines: &DefineIndex) -> Self {
        let mut catalog = ProgramCatalog::default();
        let mut template_sources = Vec::new();
        let mut file_sources = HashMap::new();
        let mut sources: Vec<_> = defines.file_sources().collect();
        sources.sort_by(|(left, _, _), (right, _, _)| helm_template_order(left, right));
        for (path, role, source) in sources {
            file_sources.insert(path.to_string(), source.to_string());
            match role {
                DefineSourceRole::FilesGet => continue,
                DefineSourceRole::Template => {}
            }
            let parsed = catalog.source(source, Some(path), path);
            template_sources.push(parsed);
        }
        Self {
            inner: Rc::new(ParsedDefinesInner {
                template_sources,
                file_sources,
                catalog: RefCell::new(catalog),
            }),
        }
    }

    /// Iterates declared names from the parsed catalog without a separate name index.
    pub fn define_names(&self) -> impl Iterator<Item = &str> {
        self.inner
            .template_sources
            .iter()
            .flat_map(|source| source.declarations.iter())
            .map(|definition| definition.name.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
    }

    /// Returns the selected declaration body for the legacy chart ownership view.
    #[must_use]
    pub fn define_body(&self, name: &str) -> Option<&str> {
        let definitions = || {
            self.inner
                .template_sources
                .iter()
                .flat_map(|source| source.declarations.iter())
                .filter(|definition| definition.name == name)
        };
        definitions()
            .rfind(|definition| !definition.syntactically_empty)
            .or_else(|| definitions().next())
            .map(|definition| definition.program.source())
    }

    /// Iterates every parsed declaration's source path in stable order.
    pub fn define_source_paths(&self, name: &str) -> impl Iterator<Item = &str> {
        self.inner
            .template_sources
            .iter()
            .flat_map(|source| source.declarations.iter())
            .filter(|definition| definition.name == name)
            .filter_map(|definition| definition.program.source_path())
            .collect::<BTreeSet<_>>()
            .into_iter()
    }

    pub(crate) fn template_sources(&self) -> &[Rc<ParsedTemplateSource>] {
        &self.inner.template_sources
    }

    pub(crate) fn parsed_source(
        &self,
        source: &str,
        source_path: Option<&str>,
        root_name: &str,
    ) -> Rc<ParsedTemplateSource> {
        self.inner
            .catalog
            .borrow_mut()
            .source(source, source_path, root_name)
    }
}

pub(crate) fn helm_template_order(left: &str, right: &str) -> std::cmp::Ordering {
    right
        .bytes()
        .filter(|byte| *byte == b'/')
        .count()
        .cmp(&left.bytes().filter(|byte| *byte == b'/').count())
        .then_with(|| right.cmp(left))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReceiverCall {
    pub(crate) receiver: TemplateExpr,
    pub(crate) name: String,
}

/// Recovers the structural receiver from the AST's callable selector spelling.
fn parse_receiver_call(function: &str) -> Option<ReceiverCall> {
    let expressions = helm_schema_ast::parse_action_expressions(&format!("{{{{ {function} }}}}"));
    let [expression] = expressions.as_slice() else {
        return None;
    };
    ReceiverCall::from_expression(expression)
}

impl ReceiverCall {
    pub(crate) fn from_expression(expression: &TemplateExpr) -> Option<Self> {
        match expression.deparen() {
            TemplateExpr::Field(path) => {
                let (name, prefix) = path.split_last()?;
                Some(ReceiverCall {
                    receiver: TemplateExpr::Field(prefix.to_vec()),
                    name: name.clone(),
                })
            }
            TemplateExpr::Selector { operand, path } => {
                let (name, prefix) = path.split_last()?;
                let receiver = if prefix.is_empty() {
                    operand.as_ref().clone()
                } else {
                    TemplateExpr::Selector {
                        operand: operand.clone(),
                        path: prefix.to_vec(),
                    }
                };
                Some(ReceiverCall {
                    receiver,
                    name: name.clone(),
                })
            }
            TemplateExpr::Literal(_)
            | TemplateExpr::Variable(_)
            | TemplateExpr::Call {
                function: _,
                args: _,
            }
            | TemplateExpr::Pipeline(_)
            | TemplateExpr::Parenthesized(_)
            | TemplateExpr::VariableDefinition { name: _, value: _ }
            | TemplateExpr::Assignment { name: _, value: _ }
            | TemplateExpr::Unknown(_) => None,
        }
    }
}

pub(crate) struct IrAnalysisDb {
    receiver_calls: RefCell<HashMap<String, Option<ReceiverCall>>>,
    parsed_defines: ParsedDefines,
    chart_default_strings: BTreeMap<String, String>,
    predicate_memo: Rc<helm_schema_core::PredicateMemo>,
    static_root_strings: BTreeMap<Vec<String>, String>,
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
            receiver_calls: RefCell::new(HashMap::new()),
            parsed_defines: parsed_defines.clone(),
            chart_default_strings,
            predicate_memo: Rc::new(helm_schema_core::PredicateMemo::new()),
            static_root_strings,
        }
    }

    pub(crate) fn receiver_call(&self, function: &str) -> Option<ReceiverCall> {
        self.receiver_calls
            .borrow_mut()
            .entry(function.to_owned())
            .or_insert_with(|| parse_receiver_call(function))
            .clone()
    }

    pub(crate) fn predicate_memo(&self) -> &Rc<helm_schema_core::PredicateMemo> {
        &self.predicate_memo
    }

    pub(crate) fn static_root_strings(&self) -> &BTreeMap<Vec<String>, String> {
        &self.static_root_strings
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

    pub(crate) fn template_sources(&self) -> &[Rc<ParsedTemplateSource>] {
        self.parsed_defines.template_sources()
    }

    pub(crate) fn parsed_source(
        &self,
        source: &str,
        source_path: Option<&str>,
        root_name: &str,
    ) -> Rc<ParsedTemplateSource> {
        self.parsed_defines
            .parsed_source(source, source_path, root_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DefineBlock {
    name: String,
    body: String,
    body_offset: usize,
    syntactically_empty: bool,
}

#[cfg(test)]
fn extract_define_blocks(src: &str) -> Vec<DefineBlock> {
    let Some(tree) = parse_go_template(src) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let document = TemplatedDocument::parse_with_root(src, tree.root_node());
    collect_define_blocks(tree.root_node(), src, document.actions(), &mut out);
    out.sort_by_key(|block| block.body_offset);
    out
}

fn collect_define_blocks(
    node: tree_sitter::Node<'_>,
    src: &str,
    actions: &[TemplateAction],
    out: &mut Vec<DefineBlock>,
) -> bool {
    let mut complete = true;
    if matches!(node.kind(), "define_action" | "block_action") {
        if let Some(block) = define_block_from_node(node, src, actions) {
            out.push(block);
        } else {
            complete = false;
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        complete &= collect_define_blocks(child, src, actions, out);
    }
    complete
}

fn syntactically_empty_node(node: tree_sitter::Node<'_>, source: &str) -> bool {
    match node.kind() {
        "comment" | "{{" | "{{-" | "}}" | "-}}" => true,
        "text" | "yaml_no_injection_text" => node
            .utf8_text(source.as_bytes())
            .is_ok_and(|text| text.trim().is_empty()),
        _ => false,
    }
}

fn define_block_from_node(
    node: tree_sitter::Node<'_>,
    src: &str,
    actions: &[helm_schema_syntax::TemplateAction],
) -> Option<DefineBlock> {
    use helm_schema_syntax::{ActionKind, ControlKind, Span, TextTrim, trim_text_span};

    let name = define_name(node, src)?;
    let opener = actions
        .iter()
        .find(|action| action.span.start == node.start_byte())?;
    let ActionKind::RegionOpen {
        region,
        kind,
        region_end: _,
    } = opener.kind
    else {
        return None;
    };
    if !matches!(kind, ControlKind::Define | ControlKind::Block) {
        return None;
    }
    let closing = actions.iter().find(
        |action| matches!(action.kind, ActionKind::RegionEnd { region: owner } if owner == region),
    )?;
    let span = trim_text_span(
        src,
        Span::new(opener.span.end, closing.span.start),
        TextTrim {
            start: opener.trim_right,
            end: closing.trim_left,
        },
    )?;
    Some(DefineBlock {
        name,
        body: src.get(span.start..span.end)?.to_owned(),
        body_offset: span.start,
        syntactically_empty: children_with_field(node, "body")
            .iter()
            .all(|node| syntactically_empty_node(*node, src)),
    })
}

fn define_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(source.as_bytes())
        .ok()?;
    let expression = format!("{{{{ {name} }}}}");
    let expressions = helm_schema_ast::parse_action_expressions(&expression);
    let [TemplateExpr::Literal(literal)] = expressions.as_slice() else {
        return None;
    };
    literal.as_string().map(str::to_string)
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
