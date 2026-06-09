use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::{DefineIndex, HelmAst, Literal, TemplateExpr, parse_action_expressions};

use crate::helper_eval::{CapabilityGuard, HelperBranch, HelperBranchBody, HelperOutput};
use crate::walker::{is_fragment_expr, parse_condition, values_path_from_expr};
use crate::{Guard, IrGenerator, ResourceRef, ValueKind, ValueUse, YamlPath};

thread_local! {
    static TEMPLATE_EXPR_CACHE: RefCell<HashMap<String, Vec<TemplateExpr>>> =
        RefCell::new(HashMap::new());
}

fn clear_template_expr_cache() {
    TEMPLATE_EXPR_CACHE.with(|cache| cache.borrow_mut().clear());
}

pub struct SymbolicIrGenerator;

impl IrGenerator for SymbolicIrGenerator {
    #[tracing::instrument(skip_all, fields(bytes = src.len()))]
    fn generate(&self, src: &str, _ast: &HelmAst, defines: &DefineIndex) -> Vec<ValueUse> {
        SymbolicIrContext::new(defines).generate(src, _ast, defines)
    }
}

/// Reusable state for generating symbolic IR across many templates that
/// share one [`DefineIndex`].
///
/// The context owns exact parse/helper-analysis caches. Reusing it across
/// templates avoids recomputing helper bodies without changing analysis
/// semantics; a cache miss and cache hit return the same structural facts.
#[derive(Clone)]
pub struct SymbolicIrContext {
    inner: Rc<SymbolicIrContextInner>,
}

struct SymbolicIrContextInner {
    define_body_sources: HashMap<String, String>,
    define_tree_cache: RefCell<HashMap<String, tree_sitter::Tree>>,
    bound_helper_calls_cache: RefCell<BTreeMap<BoundHelperCallsCacheKey, BoundHelperAnalysis>>,
}

impl SymbolicIrContext {
    #[tracing::instrument(skip_all)]
    pub fn new(defines: &DefineIndex) -> Self {
        clear_template_expr_cache();
        Self {
            inner: Rc::new(SymbolicIrContextInner {
                define_body_sources: SymbolicWalker::collect_define_body_sources(defines),
                define_tree_cache: RefCell::new(HashMap::new()),
                bound_helper_calls_cache: RefCell::new(BTreeMap::new()),
            }),
        }
    }

    #[tracing::instrument(skip_all, fields(bytes = src.len()))]
    pub fn generate(&self, src: &str, _ast: &HelmAst, defines: &DefineIndex) -> Vec<ValueUse> {
        let language =
            tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&language).is_err() {
            return Vec::new();
        }
        let Some(tree) = parser.parse(src, None) else {
            return Vec::new();
        };

        let mut w = SymbolicWalker::new_with_context(src, defines, self.clone());
        w.run(&tree)
    }
}

struct SymbolicWalker<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    ir_context: SymbolicIrContext,
    fragment_helper_body_cache: RefCell<BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>>,
    uses: Vec<ValueUse>,
    guards: Vec<Guard>,
    seed_guards: Vec<Guard>,
    seed_dot: Option<FragmentBinding>,
    no_output_depth: usize,
    dot_stack: Vec<Option<FragmentBinding>>,
    shape: Shape,
    output_inside_block_scalar: bool,
    resource_detector: ResourceDetector,
    text_spans: Vec<(usize, usize)>,
    text_span_idx: usize,
    text_pos: usize,

    inline_stack: Vec<String>,

    range_domains: HashMap<String, Vec<String>>,
    get_bindings: HashMap<String, GetBinding>,
    template_bindings: HashMap<String, FragmentBinding>,
    template_default_paths: HashMap<String, BTreeSet<String>>,
    template_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,

    inline_helpers_in_fragments: bool,
    root_bindings: HashMap<String, HelperBinding>,

    /// Paths the chart has structurally declared as null-tolerant via a
    /// `set OPERAND "KEY" (OPERAND.KEY | default V)` mutation inside a
    /// helper. Populated as the walker traverses templates that include
    /// such helpers; consumed by `emit_use_with_extra_guards` to attach
    /// `Guard::Default { path }` to any subsequent ValueUse whose
    /// `source_expr` matches.
    ///
    /// This models Helm's render-time semantics: a `set` action in a
    /// chart helper run before downstream reads (typical pattern: a
    /// `<chart>.defaultValues` helper `include`d at the top of every
    /// consumer template) means the merged values dict has the default
    /// applied before any read. Reads of that path therefore tolerate a
    /// null from values.yaml — `helm-lint --strict` sees the post-`set`
    /// value, not the raw user input.
    ///
    /// Walker scope is per-template, so a path is only widened in
    /// templates that actually traverse through an include of the
    /// defaulting helper. Templates that read `.Values.X` without
    /// running the helper produce ungrouped uses that the nullability
    /// pass treats as null-intolerant, which is the conservative read.
    chart_value_defaults: BTreeSet<String>,
}

#[derive(Clone, Debug)]
struct GetBinding {
    base: String,
    key_var: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct HelperOutputMeta {
    guards: BTreeSet<String>,
    defaulted: bool,
}

#[derive(Clone, Debug)]
struct HelperFragmentOutputUse {
    source_expr: String,
    relative_path: YamlPath,
    kind: ValueKind,
    meta: HelperOutputMeta,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StaticFileTemplate {
    path: String,
    dot: Option<FragmentBinding>,
}

#[derive(Clone, Debug)]
struct LiteralHelperCall {
    name: String,
    arg: Option<TemplateExpr>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum HelperBinding {
    ValuesPath(String),
    RootContext,
    Unknown,
    OutputSet(BTreeMap<String, HelperOutputMeta>),
    PathSet(BTreeSet<String>),
    Dict(BTreeMap<String, HelperBinding>),
    List(Vec<HelperBinding>),
    Overlay {
        entries: BTreeMap<String, HelperBinding>,
        fallback: Box<HelperBinding>,
    },
    Choice(BTreeSet<HelperBinding>),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FragmentBinding {
    ValuesPath(String),
    ValuesRoot,
    RootContext,
    Unknown,
    Dict(BTreeMap<String, FragmentBinding>),
    List(Vec<FragmentBinding>),
    Overlay {
        entries: BTreeMap<String, FragmentBinding>,
        fallback: Box<FragmentBinding>,
    },
    StringSet(BTreeSet<String>),
    PathSet(BTreeSet<String>),
    OutputSet(BTreeSet<String>),
    Choice(BTreeSet<FragmentBinding>),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FragmentHelperBodyCacheKey {
    name: String,
    helper_dot: Option<FragmentBinding>,
    locals: BTreeMap<String, FragmentBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BoundHelperCallsCacheKey {
    text: String,
    current_dot: Option<HelperBinding>,
}

#[derive(Clone, Debug, Default)]
struct BoundHelperAnalysis {
    output: BTreeMap<String, HelperOutputMeta>,
    fragment_output: BTreeSet<String>,
    fragment_output_uses: Vec<HelperFragmentOutputUse>,
    guard_paths: BTreeSet<String>,
    type_hints: BTreeMap<String, BTreeSet<String>>,
    suppress_roots: BTreeSet<String>,
    /// Values-rooted paths that the helper body has structurally declared
    /// as null-tolerant via a `set OPERAND "KEY" (OPERAND.KEY | default V)`
    /// mutation. Distinct from `defaulted` (which fires on every
    /// `(X | default V)` expression, including condition fallbacks):
    /// only the explicit set-mutation pattern counts here, because that
    /// is the chart writer asserting "this path gets defaulted before
    /// any other read." See [`SymbolicWalker::set_default_chart_paths_for_text`].
    chart_defaults: BTreeSet<String>,
}

impl BoundHelperAnalysis {
    fn extend(&mut self, other: Self) {
        for (path, meta) in other.output {
            let entry = self.output.entry(path).or_default();
            entry.guards.extend(meta.guards);
            entry.defaulted |= meta.defaulted;
        }
        self.fragment_output.extend(other.fragment_output);
        self.fragment_output_uses.extend(other.fragment_output_uses);
        self.guard_paths.extend(other.guard_paths);
        for (path, schema_types) in other.type_hints {
            self.type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
        self.suppress_roots.extend(other.suppress_roots);
        self.chart_defaults.extend(other.chart_defaults);
    }

    fn add_output(&mut self, path: String, guards: &BTreeSet<String>, defaulted: bool) {
        let entry = self.output.entry(path).or_default();
        entry.guards.extend(guards.iter().cloned());
        entry.defaulted |= defaulted;
    }
}

fn bound_helper_dependency_paths(analysis: &BoundHelperAnalysis) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = analysis
        .output
        .keys()
        .chain(analysis.guard_paths.iter())
        .chain(analysis.fragment_output.iter())
        .chain(analysis.type_hints.keys())
        .cloned()
        .collect();
    out.extend(
        analysis
            .fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone()),
    );
    out
}

fn convert_fragment_outputs_to_dependency_outputs(analysis: &mut BoundHelperAnalysis) {
    let fragment_output = std::mem::take(&mut analysis.fragment_output);
    for source_expr in fragment_output {
        analysis.add_output(source_expr, &BTreeSet::new(), false);
    }

    let fragment_output_uses = std::mem::take(&mut analysis.fragment_output_uses);
    for output in fragment_output_uses {
        let entry = analysis.output.entry(output.source_expr).or_default();
        entry.guards.extend(output.meta.guards);
        entry.defaulted |= output.meta.defaulted;
    }
}

fn merge_local_default_paths(
    mut base: HashMap<String, BTreeSet<String>>,
    other: HashMap<String, BTreeSet<String>>,
) -> HashMap<String, BTreeSet<String>> {
    for (key, paths) in other {
        base.entry(key).or_default().extend(paths);
    }
    base
}

fn extend_type_hints(
    target: &mut BTreeMap<String, BTreeSet<String>>,
    hints: BTreeMap<String, BTreeSet<String>>,
) {
    for (path, schema_types) in hints {
        target.entry(path).or_default().extend(schema_types);
    }
}

#[derive(Clone, Debug)]
struct Shape {
    stack: Vec<(usize, Container, Option<String>)>,
    at_line_start: bool,
    clear_pending_on_newline_at_indent: Option<usize>,
    prefix: Vec<String>,
    stack_floor: usize,
    block_scalar_parent_indent: Option<usize>,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            at_line_start: true,
            clear_pending_on_newline_at_indent: None,
            prefix: Vec::new(),
            stack_floor: 0,
            block_scalar_parent_indent: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Container {
    Mapping,
    Sequence,
}

struct ParsedYamlKey {
    key: String,
    scalar_value_present: bool,
    starts_block_scalar: bool,
}

fn parse_yaml_key(after: &str) -> Option<ParsedYamlKey> {
    fn finalize_yaml_key(key: String, rest: &str) -> Option<ParsedYamlKey> {
        if key.is_empty() {
            return None;
        }
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(':').unwrap_or(rest);
        let rest = rest.trim_start();
        let starts_block_scalar = rest.starts_with('|') || rest.starts_with('>');
        let is_template = rest.starts_with("{{");
        let is_template_fragment = is_template
            && (rest.contains("toYaml")
                || rest.contains("nindent")
                || rest.contains("indent")
                || rest.contains("tpl"));
        let is_block = rest.is_empty() || starts_block_scalar || is_template_fragment;
        Some(ParsedYamlKey {
            key,
            scalar_value_present: !is_block,
            starts_block_scalar,
        })
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
        if self.is_inside_block_scalar_line(indent) {
            return;
        }
        self.block_scalar_parent_indent = None;

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

    fn is_inside_block_scalar_line(&self, indent: usize) -> bool {
        self.block_scalar_parent_indent
            .is_some_and(|parent_indent| indent > parent_indent)
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

            if self.is_inside_block_scalar_line(indent) {
                self.at_line_start = is_newline_terminated;
                continue;
            }
            self.block_scalar_parent_indent = None;

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
                    let starts_block_scalar = rest.starts_with('|') || rest.starts_with('>');
                    if starts_block_scalar {
                        self.block_scalar_parent_indent = Some(child_indent);
                    }
                    if !rest.is_empty() && !starts_block_scalar {
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

            if let Some(parsed_key) = parse_yaml_key(after) {
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
                        *pending = Some(parsed_key.key);
                    }
                    Some((top_indent, _, _)) if *top_indent < indent => {
                        self.stack
                            .push((indent, Container::Mapping, Some(parsed_key.key)));
                    }
                    None => {
                        self.stack
                            .push((indent, Container::Mapping, Some(parsed_key.key)));
                    }
                    _ => {}
                }

                if parsed_key.starts_block_scalar {
                    self.block_scalar_parent_indent = Some(indent);
                }

                if parsed_key.scalar_value_present {
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

    #[cfg(test)]
    fn new(source: &'a str, defines: &'a DefineIndex) -> Self {
        Self::new_with_context(source, defines, SymbolicIrContext::new(defines))
    }

    fn new_with_context(
        source: &'a str,
        defines: &'a DefineIndex,
        ir_context: SymbolicIrContext,
    ) -> Self {
        Self {
            source,
            defines,
            ir_context,
            fragment_helper_body_cache: RefCell::new(BTreeMap::new()),
            uses: Vec::new(),
            guards: Vec::new(),
            seed_guards: Vec::new(),
            seed_dot: None,
            no_output_depth: 0,
            dot_stack: Vec::new(),
            shape: Shape::default(),
            output_inside_block_scalar: false,
            resource_detector: ResourceDetector::default(),
            text_spans: Vec::new(),
            text_span_idx: 0,
            text_pos: 0,

            inline_stack: Vec::new(),

            range_domains: HashMap::new(),
            get_bindings: HashMap::new(),
            template_bindings: HashMap::new(),
            template_default_paths: HashMap::new(),
            template_output_meta: HashMap::new(),

            inline_helpers_in_fragments: false,
            root_bindings: HashMap::new(),

            chart_value_defaults: BTreeSet::new(),
        }
    }

    fn with_initial_guards(mut self, guards: Vec<Guard>) -> Self {
        self.seed_guards = guards;
        self.guards = self.seed_guards.clone();
        self
    }

    fn with_initial_dot_binding(mut self, dot: Option<FragmentBinding>) -> Self {
        self.seed_dot = dot;
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

    /// Seed this walker's chart-level defaults from a parent walker so a
    /// nested static-file template walk inherits the same render-time
    /// mutation context. The parent's `include "X.defaultValues" .`
    /// already ran above the nested fragment in source order, so the
    /// fragment's reads see the same defaulted state.
    fn with_chart_value_defaults(mut self, defaults: BTreeSet<String>) -> Self {
        self.chart_value_defaults = defaults;
        self
    }

    #[tracing::instrument(skip_all, fields(bytes = src.len()))]
    fn parse_go_template(src: &str) -> Option<tree_sitter::Tree> {
        let language =
            tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&language).is_err() {
            return None;
        }
        parser.parse(src, None)
    }

    #[tracing::instrument(skip_all)]
    fn define_body_tree_from_cache(
        name: &str,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
    ) -> Option<tree_sitter::Tree> {
        if let Some(tree) = define_tree_cache.borrow().get(name) {
            return Some(tree.clone());
        }

        let src = define_sources.get(name)?;
        let tree = Self::parse_go_template(src)?;
        define_tree_cache
            .borrow_mut()
            .insert(name.to_string(), tree.clone());
        Some(tree)
    }

    #[tracing::instrument(skip_all)]
    fn collect_define_body_sources(defines: &DefineIndex) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for (_path, src) in defines.file_sources() {
            for block in crate::extract_define_blocks(src) {
                out.insert(block.name, block.body);
            }
        }
        out
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

        // Track `$var.field[.subfield]...` reads for templates that bind
        // `$var := get $.Values.someMap $key` inside a known key-domain range.
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

    fn inline_static_file_templates_from_helper_calls(&mut self, text: &str) {
        for helper_call in Self::literal_helper_calls(text) {
            let current_dot = self.current_dot_fragment();
            let mut seen = HashSet::new();
            let helper_dot = helper_call.arg.as_ref().and_then(|arg| {
                Self::fragment_binding_from_expr(
                    arg,
                    &self.template_bindings,
                    current_dot.as_ref(),
                    self.defines,
                    &self.ir_context.inner.define_body_sources,
                    &self.ir_context.inner.define_tree_cache,
                    &self.fragment_helper_body_cache,
                    &mut seen,
                )
            });
            let requests =
                self.static_file_templates_from_helper(&helper_call.name, helper_dot.as_ref());
            for request in requests {
                self.inline_static_file_template(request);
            }
        }
    }

    fn static_file_templates_from_helper(
        &self,
        name: &str,
        helper_dot: Option<&FragmentBinding>,
    ) -> BTreeSet<StaticFileTemplate> {
        let Some(src) = self.define_body_source(name) else {
            return BTreeSet::new();
        };
        let locals = HashMap::new();
        let mut requests = BTreeSet::new();
        for expr in parse_action_expressions(src) {
            let mut seen = HashSet::new();
            Self::collect_static_file_templates_from_expr(
                &expr,
                &locals,
                helper_dot,
                self.defines,
                &self.ir_context.inner.define_body_sources,
                &self.ir_context.inner.define_tree_cache,
                &self.fragment_helper_body_cache,
                &mut seen,
                &mut requests,
            );
        }
        requests
    }

    fn collect_static_file_templates_from_expr(
        expr: &TemplateExpr,
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
        requests: &mut BTreeSet<StaticFileTemplate>,
    ) {
        if let TemplateExpr::Call { function, args } = expr
            && function == "tpl"
            && let Some(template_arg) = args.first()
        {
            let dot = args.get(1).and_then(|arg| {
                Self::fragment_binding_from_expr(
                    arg,
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                )
            });
            let mut paths = BTreeSet::new();
            Self::collect_static_files_get_paths_from_expr(
                template_arg,
                locals,
                current_dot,
                defines,
                define_sources,
                define_tree_cache,
                fragment_helper_body_cache,
                seen,
                &mut paths,
            );
            for path in paths {
                requests.insert(StaticFileTemplate {
                    path,
                    dot: dot.clone(),
                });
            }
        }

        match expr {
            TemplateExpr::Call { args, .. } => {
                for arg in args {
                    Self::collect_static_file_templates_from_expr(
                        arg,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        requests,
                    );
                }
            }
            TemplateExpr::Selector { operand, .. }
            | TemplateExpr::Parenthesized(operand)
            | TemplateExpr::VariableDefinition { value: operand, .. }
            | TemplateExpr::Assignment { value: operand, .. } => {
                Self::collect_static_file_templates_from_expr(
                    operand,
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    requests,
                );
            }
            TemplateExpr::Pipeline(stages) => {
                for stage in stages {
                    Self::collect_static_file_templates_from_expr(
                        stage,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        requests,
                    );
                }
            }
            TemplateExpr::Literal(_)
            | TemplateExpr::Field(_)
            | TemplateExpr::Variable(_)
            | TemplateExpr::Unknown(_) => {}
        }
    }

    fn collect_static_files_get_paths_from_expr(
        expr: &TemplateExpr,
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
        out: &mut BTreeSet<String>,
    ) {
        if let TemplateExpr::Call { function, args } = expr
            && Self::is_static_files_get_call(function)
            && let Some(path_arg) = args.first()
            && let Some(binding) = Self::fragment_binding_from_expr(
                path_arg,
                locals,
                current_dot,
                defines,
                define_sources,
                define_tree_cache,
                fragment_helper_body_cache,
                seen,
            )
        {
            out.extend(Self::fragment_strings(&binding));
        }

        match expr {
            TemplateExpr::Call { args, .. } => {
                for arg in args {
                    Self::collect_static_files_get_paths_from_expr(
                        arg,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        out,
                    );
                }
            }
            TemplateExpr::Selector { operand, .. }
            | TemplateExpr::Parenthesized(operand)
            | TemplateExpr::VariableDefinition { value: operand, .. }
            | TemplateExpr::Assignment { value: operand, .. } => {
                Self::collect_static_files_get_paths_from_expr(
                    operand,
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    out,
                );
            }
            TemplateExpr::Pipeline(stages) => {
                for stage in stages {
                    Self::collect_static_files_get_paths_from_expr(
                        stage,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        out,
                    );
                }
            }
            TemplateExpr::Literal(_)
            | TemplateExpr::Field(_)
            | TemplateExpr::Variable(_)
            | TemplateExpr::Unknown(_) => {}
        }
    }

    fn is_static_files_get_call(function: &str) -> bool {
        function == "Files.Get" || function == ".Files.Get" || function.ends_with(".Files.Get")
    }

    fn inline_static_file_template(&mut self, request: StaticFileTemplate) {
        let token = format!("file:{}", request.path);
        if self.inline_stack.iter().any(|entry| entry == &token) {
            return;
        }
        let Some(src) = self.defines.get_file(&request.path) else {
            return;
        };
        let Some(tree) = Self::parse_go_template(src) else {
            return;
        };

        let mut stack = self.inline_stack.clone();
        stack.push(token);
        let mut nested =
            SymbolicWalker::new_with_context(src, self.defines, self.ir_context.clone())
                .with_initial_guards(self.guards.clone())
                .with_initial_dot_binding(request.dot)
                .with_inline_stack(stack)
                .with_inline_helpers_in_fragments(true)
                .with_chart_value_defaults(self.chart_value_defaults.clone());
        let uses = nested.run(&tree);
        self.uses.extend(uses);
    }

    #[tracing::instrument(skip_all)]
    fn run(&mut self, tree: &tree_sitter::Tree) -> Vec<ValueUse> {
        self.reset_text_spans(tree);
        self.walk(tree.root_node());
        Self::drop_default_guard_subsumed_duplicates(&mut self.uses);
        Self::drop_values_list_envelope_duplicates(&mut self.uses);
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

    fn drop_default_guard_subsumed_duplicates(uses: &mut Vec<ValueUse>) {
        let defaulted_render_sites: BTreeSet<_> = uses
            .iter()
            .filter(|use_| {
                use_.guards.iter().any(
                    |guard| matches!(guard, Guard::Default { path } if path == &use_.source_expr),
                )
            })
            .map(|use_| {
                (
                    use_.source_expr.clone(),
                    use_.path.clone(),
                    use_.kind,
                    use_.resource.clone(),
                )
            })
            .collect();

        uses.retain(|use_| {
            if use_
                .guards
                .iter()
                .any(|guard| matches!(guard, Guard::Default { path } if path == &use_.source_expr))
            {
                return true;
            }
            !defaulted_render_sites.contains(&(
                use_.source_expr.clone(),
                use_.path.clone(),
                use_.kind,
                use_.resource.clone(),
            ))
        });
    }

    fn drop_values_list_envelope_duplicates(uses: &mut Vec<ValueUse>) {
        let render_sites: BTreeSet<_> = uses
            .iter()
            .map(|use_| {
                (
                    use_.source_expr.clone(),
                    use_.path.clone(),
                    use_.kind,
                    use_.resource.clone(),
                )
            })
            .collect();

        uses.retain(|use_| {
            let Some(index) = use_
                .path
                .0
                .iter()
                .position(|segment| segment == "values[*]")
            else {
                return true;
            };
            let mut collapsed_path = use_.path.clone();
            collapsed_path.0.remove(index);
            !render_sites.contains(&(
                use_.source_expr.clone(),
                collapsed_path,
                use_.kind,
                use_.resource.clone(),
            ))
        });
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn literal_helper_calls(text: &str) -> Vec<LiteralHelperCall> {
        let mut out = Vec::new();
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
                out.push(LiteralHelperCall {
                    name: name.clone(),
                    arg: args.get(1).cloned(),
                });
            });
        }
        out.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| format!("{:?}", left.arg).cmp(&format!("{:?}", right.arg)))
        });
        out.dedup_by(|left, right| left.name == right.name && left.arg == right.arg);
        out
    }

    #[tracing::instrument(skip_all)]
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
        self.output_inside_block_scalar = false;
        self.guards = self.seed_guards.clone();
        self.dot_stack.clear();
        if let Some(dot) = self.seed_dot.clone() {
            self.dot_stack.push(Some(dot));
        }
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
        if !matches!(node.kind(), "template_action" | "variable") {
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
        self.output_inside_block_scalar = self.shape.is_inside_block_scalar_line(physical_indent);

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
        self.emit_use_with_extra_guards(source_expr, path, kind, &[]);
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
        // If a helper already invoked above this walk in source order
        // structurally set a default for this exact path (the chart
        // writer's `set OPERAND "K" (OPERAND.K | default V)` mutation —
        // see `set_default_chart_paths_for_text`), surface that as a
        // `Guard::Default` so the nullability pass sees the same null
        // tolerance Helm's render-time `set` produces. The chart-default
        // applies only to reads with a non-empty `path` (i.e. ones
        // contributing to a rendered field): without a render use the
        // guard would be meaningless.
        if !path.0.is_empty() && self.chart_value_defaults.contains(&source_expr) {
            let default_guard = Guard::Default {
                path: source_expr.clone(),
            };
            if !guards.contains(&default_guard) {
                guards.push(default_guard);
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
        self.emit_helper_use_with_extra_guards(source_expr, &[]);
    }

    fn emit_helper_use_with_extra_guards(&mut self, source_expr: String, extra_guards: &[Guard]) {
        if source_expr.trim().is_empty() {
            return;
        }
        let mut guards = self.guards.clone();
        for guard in extra_guards {
            if !guards.contains(guard) {
                guards.push(guard.clone());
            }
        }
        self.uses.push(ValueUse {
            source_expr,
            path: YamlPath(Vec::new()),
            kind: ValueKind::Scalar,
            guards,
            resource: None,
        });
    }

    fn helper_output_extra_guards(source_expr: &str, meta: &HelperOutputMeta) -> Vec<Guard> {
        let mut guards: Vec<Guard> = meta
            .guards
            .iter()
            .filter(|path| !path.trim().is_empty())
            .cloned()
            .map(|path| Guard::Truthy { path })
            .collect();
        if meta.defaulted {
            guards.push(Guard::Default {
                path: source_expr.to_string(),
            });
        }
        guards
    }

    fn helper_dependency_extra_guards(meta: &HelperOutputMeta) -> Vec<Guard> {
        meta.guards
            .iter()
            .filter(|path| !path.trim().is_empty())
            .cloned()
            .map(|path| Guard::Truthy { path })
            .collect()
    }

    fn values_path_has_descendant(path: &str, sources: &BTreeSet<String>) -> bool {
        sources
            .iter()
            .any(|source| Self::values_path_is_strict_descendant(source, path))
    }

    fn values_path_is_strict_descendant(path: &str, ancestor: &str) -> bool {
        if ancestor.trim().is_empty() {
            return !path.trim().is_empty();
        }

        path.strip_prefix(ancestor)
            .is_some_and(|rest| rest.starts_with('.'))
    }

    fn current_dot_binding(&self) -> Option<HelperBinding> {
        self.dot_stack
            .last()
            .and_then(|binding| binding.as_ref())
            .and_then(|binding| match binding {
                FragmentBinding::ValuesPath(path) => Some(HelperBinding::ValuesPath(path.clone())),
                FragmentBinding::ValuesRoot => Some(HelperBinding::ValuesPath(String::new())),
                FragmentBinding::RootContext => Some(HelperBinding::RootContext),
                FragmentBinding::Unknown
                | FragmentBinding::Dict(_)
                | FragmentBinding::List(_)
                | FragmentBinding::Overlay { .. }
                | FragmentBinding::StringSet(_)
                | FragmentBinding::PathSet(_)
                | FragmentBinding::OutputSet(_)
                | FragmentBinding::Choice(_) => None,
            })
    }

    fn current_dot_fragment(&self) -> Option<FragmentBinding> {
        self.dot_stack.last().and_then(|binding| binding.clone())
    }

    fn fragment_binding_in_context(
        &self,
        expr: &TemplateExpr,
        current_dot: Option<&FragmentBinding>,
    ) -> Option<FragmentBinding> {
        let mut seen = HashSet::new();
        Self::fragment_binding_from_expr(
            expr,
            &self.template_bindings,
            current_dot,
            self.defines,
            &self.ir_context.inner.define_body_sources,
            &self.ir_context.inner.define_tree_cache,
            &self.fragment_helper_body_cache,
            &mut seen,
        )
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn resolved_values_paths_in_context(&self, text: &str) -> Vec<String> {
        let exprs = Self::parse_expr_text(text);
        let mut paths = Self::direct_values_paths_from_exprs(&exprs);

        if !self.root_bindings.is_empty() {
            for expr in &exprs {
                Self::walk_expr_excluding_helper_call_args(expr, &mut |node| {
                    if let Some(path) = Self::resolve_bound_path_expr(node, &self.root_bindings) {
                        paths.insert(path);
                    }
                });
            }
        }

        if !self.template_bindings.is_empty() {
            for expr in &exprs {
                Self::walk_expr_excluding_helper_call_args(expr, &mut |node| {
                    paths.extend(self.local_alias_paths_for_expr(node));
                });
            }
        }

        if paths.is_empty() {
            paths.extend(self.resolved_values_paths_in_expr_tree_context(text));
        }

        paths.into_iter().collect()
    }

    fn direct_values_paths_from_exprs(exprs: &[TemplateExpr]) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        for expr in exprs {
            Self::walk_expr_excluding_helper_call_args(expr, &mut |node| {
                if let Some(path) = values_path_from_expr(node) {
                    paths.insert(path);
                }
            });
        }
        paths
    }

    fn resolve_expr_to_values_path_in_context(
        expr: &TemplateExpr,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> Option<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(path);
        }

        match Self::binding_from_expr(expr, bindings, current_dot) {
            Some(HelperBinding::ValuesPath(path)) => Some(path),
            _ => None,
        }
    }

    fn resolve_expr_to_values_paths_in_context(
        expr: &TemplateExpr,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return [path].into_iter().collect();
        }

        Self::binding_from_expr(expr, bindings, current_dot)
            .map(|binding| Self::helper_binding_paths(&binding))
            .unwrap_or_default()
    }

    fn resolved_default_fallback_paths_in_context(&self, text: &str) -> BTreeSet<String> {
        let current_dot = self.current_dot_binding();
        let mut paths = Self::resolved_default_fallback_paths_for_text(
            text,
            Some(&self.root_bindings),
            current_dot.as_ref(),
        );
        for expr in Self::parse_expr_text(text) {
            paths.extend(self.resolved_default_fallback_paths_for_expr_in_current_context(&expr));
        }
        if !self.template_default_paths.is_empty() {
            for expr in Self::parse_expr_text(text) {
                expr.walk(|node| {
                    paths.extend(self.local_alias_default_paths_for_expr(node));
                });
            }
        }
        paths
    }

    fn resolved_default_fallback_paths_for_expr_in_current_context(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        expr.walk(|node| match node {
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                out.extend(self.resolve_expr_to_values_paths_in_current_context(&args[1]));
            }
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let TemplateExpr::Call { function, .. } = &window[1] else {
                        continue;
                    };
                    if function != "default" {
                        continue;
                    }
                    out.extend(self.resolve_expr_to_values_paths_in_current_context(&window[0]));
                }
            }
            _ => {}
        });
        out
    }

    fn resolve_expr_to_values_paths_in_current_context(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeSet<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return [path].into_iter().collect();
        }

        let mut locals = self.template_bindings.clone();
        for (key, value) in &self.root_bindings {
            locals.insert(
                key.clone(),
                Self::fragment_binding_from_helper_binding(value),
            );
        }

        let current_dot_fragment = self.current_dot_fragment();
        let current_dot_binding = self.current_dot_binding();
        let outer_binding = Self::fragment_binding_from_outer_expr(
            expr,
            Some(&locals),
            Some(&self.root_bindings),
            current_dot_binding.as_ref(),
        );
        let binding = match outer_binding {
            Some(binding) if !Self::fragment_paths(&binding).is_empty() => Some(binding),
            _ => self.fragment_binding_in_context(expr, current_dot_fragment.as_ref()),
        };

        binding
            .map(|binding| {
                Self::fragment_paths(&binding)
                    .into_iter()
                    .filter(|path| !path.trim().is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn local_alias_paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_bindings
                .get(var)
                .map(Self::fragment_paths)
                .unwrap_or_default(),
            TemplateExpr::Selector { operand, path } => match operand.as_ref() {
                TemplateExpr::Variable(var) if !var.is_empty() => self
                    .template_bindings
                    .get(var)
                    .and_then(|binding| Self::fragment_apply_binding(binding, path))
                    .map(|binding| Self::fragment_paths(&binding))
                    .unwrap_or_default(),
                _ => BTreeSet::new(),
            },
            _ => BTreeSet::new(),
        }
    }

    fn local_alias_default_paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_default_paths
                .get(var)
                .cloned()
                .unwrap_or_default(),
            _ => BTreeSet::new(),
        }
    }

    fn local_alias_output_meta_for_text(&self, text: &str) -> BTreeMap<String, HelperOutputMeta> {
        let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        for expr in Self::parse_expr_text(text) {
            Self::walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                for (path, meta) in self.local_alias_output_meta_for_expr(node) {
                    let entry = out.entry(path).or_default();
                    entry.guards.extend(meta.guards);
                    entry.defaulted |= meta.defaulted;
                }
            });
        }
        out
    }

    fn helper_output_meta_for_text(&self, text: &str) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = self.local_alias_output_meta_for_text(text);
        let analysis = self.analyze_bound_helper_calls(text);
        for (path, meta) in analysis.output {
            let entry = out.entry(path).or_default();
            entry.guards.extend(meta.guards);
            entry.defaulted |= meta.defaulted;
        }
        for output in analysis.fragment_output_uses {
            let entry = out.entry(output.source_expr).or_default();
            entry.guards.extend(output.meta.guards);
            entry.defaulted |= output.meta.defaulted;
        }
        out
    }

    fn local_alias_output_meta_for_expr(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeMap<String, HelperOutputMeta> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_output_meta
                .get(var)
                .cloned()
                .unwrap_or_default(),
            TemplateExpr::Selector { operand, path } => {
                let TemplateExpr::Variable(var) = operand.as_ref() else {
                    return BTreeMap::new();
                };
                if var.is_empty() {
                    return BTreeMap::new();
                }
                let Some(binding) = self.template_bindings.get(var) else {
                    return BTreeMap::new();
                };
                let Some(bound) = Self::fragment_apply_binding(binding, path) else {
                    return BTreeMap::new();
                };
                let selected_paths = Self::fragment_paths(&bound);
                self.template_output_meta
                    .get(var)
                    .into_iter()
                    .flat_map(|meta_by_path| meta_by_path.iter())
                    .filter(|(path, _meta)| selected_paths.contains(*path))
                    .map(|(path, meta)| (path.clone(), meta.clone()))
                    .collect()
            }
            _ => BTreeMap::new(),
        }
    }

    fn condition_guards_in_context(&self, text: &str) -> Vec<Guard> {
        let mut cond_guards = parse_condition(text);
        let alias_guards = self.condition_guards_from_aliases_in_context(text);
        cond_guards
            .retain(|guard| !Self::guard_is_subsumed_by_alias_or_guard(guard, &alias_guards));
        for guard in alias_guards {
            if !cond_guards.contains(&guard) {
                cond_guards.push(guard);
            }
        }
        if !cond_guards.is_empty() {
            return cond_guards;
        }
        if self.condition_has_unrepresentable_values_comparison(text) {
            return Vec::new();
        }
        self.resolved_values_paths_in_expr_tree_context(text)
            .into_iter()
            .map(|path| Guard::Truthy { path })
            .collect()
    }

    fn guard_is_subsumed_by_alias_or_guard(guard: &Guard, alias_guards: &[Guard]) -> bool {
        if !matches!(guard, Guard::Truthy { .. } | Guard::Or { .. }) {
            return false;
        }

        alias_guards.iter().any(|alias_guard| {
            let Guard::Or { paths } = alias_guard else {
                return false;
            };
            guard.value_paths().iter().all(|path| {
                paths
                    .iter()
                    .any(|alias_guard_path| alias_guard_path == path)
            })
        })
    }

    fn expr_needs_context_value_resolution(&self, expr: &TemplateExpr) -> bool {
        !self.local_alias_paths_for_expr(expr).is_empty()
            || (values_path_from_expr(expr).is_none()
                && !self
                    .resolve_expr_to_values_paths_in_current_context(expr)
                    .is_empty())
    }

    fn condition_guards_from_aliases_in_context(&self, text: &str) -> Vec<Guard> {
        fn string_literal(arg: &TemplateExpr) -> Option<String> {
            match arg.deparen() {
                TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
                    Some(value.clone())
                }
                _ => None,
            }
        }

        fn paths_for_expr(walker: &SymbolicWalker<'_>, expr: &TemplateExpr) -> BTreeSet<String> {
            let mut paths = walker.resolve_expr_to_values_paths_in_current_context(expr);
            paths.extend(walker.local_alias_paths_for_expr(expr));
            paths
                .into_iter()
                .filter(|path| !path.trim().is_empty())
                .collect()
        }

        let mut out = Vec::new();
        for expr in Self::parse_expr_text(text) {
            let TemplateExpr::Call { function, args } = expr.deparen() else {
                continue;
            };
            match function.as_str() {
                "not" => {
                    let [arg] = args.as_slice() else {
                        continue;
                    };
                    if !self.expr_needs_context_value_resolution(arg) {
                        continue;
                    }
                    let paths = paths_for_expr(self, arg);
                    out.extend(paths.into_iter().map(|path| Guard::Not { path }));
                }
                "or" => {
                    if !args
                        .iter()
                        .any(|arg| self.expr_needs_context_value_resolution(arg))
                    {
                        continue;
                    }
                    let paths: BTreeSet<String> = args
                        .iter()
                        .flat_map(|arg| paths_for_expr(self, arg))
                        .collect();
                    if !paths.is_empty() {
                        out.push(Guard::Or {
                            paths: paths.into_iter().collect(),
                        });
                    }
                }
                "eq" => {
                    let [left, right] = args.as_slice() else {
                        continue;
                    };
                    if !self.expr_needs_context_value_resolution(left)
                        && !self.expr_needs_context_value_resolution(right)
                    {
                        continue;
                    }
                    let (value, paths) = match (string_literal(left), string_literal(right)) {
                        (Some(value), None) => (value, paths_for_expr(self, right)),
                        (None, Some(value)) => (value, paths_for_expr(self, left)),
                        _ => continue,
                    };
                    out.extend(paths.into_iter().map(|path| Guard::Eq {
                        path,
                        value: value.clone(),
                    }));
                }
                "typeIs" => {
                    let Some(schema_type) = Self::type_is_schema_type(args.first()) else {
                        continue;
                    };
                    if !args
                        .iter()
                        .skip(1)
                        .any(|arg| self.expr_needs_context_value_resolution(arg))
                    {
                        continue;
                    }
                    let paths: BTreeSet<String> = args
                        .iter()
                        .skip(1)
                        .flat_map(|arg| paths_for_expr(self, arg))
                        .collect();
                    out.extend(paths.into_iter().map(|path| Guard::TypeIs {
                        path,
                        schema_type: schema_type.clone(),
                    }));
                }
                _ => {}
            }
        }
        out
    }

    fn condition_has_unrepresentable_values_comparison(&self, text: &str) -> bool {
        fn string_literal(arg: &TemplateExpr) -> Option<&str> {
            match arg.deparen() {
                TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
                    Some(value)
                }
                _ => None,
            }
        }

        Self::parse_expr_text(text).into_iter().any(|expr| {
            let TemplateExpr::Call { function, args } = expr.deparen() else {
                return false;
            };
            match function.as_str() {
                "eq" => {
                    let has_values_path = args
                        .iter()
                        .any(|arg| self.expr_needs_context_value_resolution(arg));
                    if !has_values_path {
                        return false;
                    }
                    let [left, right] = args.as_slice() else {
                        return true;
                    };
                    !matches!(
                        (string_literal(left), string_literal(right)),
                        (Some(_), None) | (None, Some(_))
                    )
                }
                "ne" | "typeIs" => args
                    .iter()
                    .any(|arg| self.expr_needs_context_value_resolution(arg)),
                _ => false,
            }
        })
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn resolved_values_paths_in_expr_tree_context(&self, text: &str) -> BTreeSet<String> {
        let mut locals = self.template_bindings.clone();
        for (key, value) in &self.root_bindings {
            locals.insert(
                key.clone(),
                Self::fragment_binding_from_helper_binding(value),
            );
        }

        let current_dot_fragment = self.current_dot_fragment();
        let current_dot_binding = self.current_dot_binding();
        let mut paths = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            Self::walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                if Self::expr_contains_helper_call(node) {
                    return;
                }
                let outer_binding = Self::fragment_binding_from_outer_expr(
                    node,
                    Some(&locals),
                    Some(&self.root_bindings),
                    current_dot_binding.as_ref(),
                );
                let binding = match outer_binding {
                    Some(binding) if !Self::fragment_paths(&binding).is_empty() => Some(binding),
                    _ => self.fragment_binding_in_context(node, current_dot_fragment.as_ref()),
                };
                if let Some(binding) = binding {
                    paths.extend(
                        Self::fragment_paths(&binding)
                            .into_iter()
                            .filter(|path| !path.trim().is_empty()),
                    );
                }
            });
        }
        paths
    }

    fn expr_contains_helper_call(expr: &TemplateExpr) -> bool {
        let mut contains = false;
        expr.walk(|node| {
            if matches!(
                node,
                TemplateExpr::Call { function, .. }
                    if matches!(function.as_str(), "include" | "template")
            ) {
                contains = true;
            }
        });
        contains
    }

    fn expr_starts_with_helper_call(expr: &TemplateExpr) -> bool {
        match expr {
            TemplateExpr::Parenthesized(inner) => Self::expr_starts_with_helper_call(inner),
            TemplateExpr::Call { function, .. } => {
                matches!(function.as_str(), "include" | "template")
            }
            TemplateExpr::Pipeline(stages) => stages
                .first()
                .is_some_and(Self::expr_starts_with_helper_call),
            _ => false,
        }
    }

    fn text_starts_with_helper_call(text: &str) -> bool {
        let exprs = Self::parse_expr_text(text);
        matches!(exprs.as_slice(), [expr] if Self::expr_starts_with_helper_call(expr))
    }

    fn text_pipeline_merges_into_var(text: &str, var: &str) -> bool {
        let exprs = Self::parse_expr_text(text);
        let [TemplateExpr::Pipeline(stages)] = exprs.as_slice() else {
            return false;
        };
        stages.iter().skip(1).any(|stage| {
            let TemplateExpr::Call { function, args } = stage else {
                return false;
            };
            matches!(function.as_str(), "merge" | "mergeOverwrite")
                && args
                    .iter()
                    .any(|arg| matches!(arg, TemplateExpr::Variable(name) if name == var))
        })
    }

    fn walk_expr_excluding_helper_call_args<F>(expr: &TemplateExpr, visit: &mut F)
    where
        F: FnMut(&TemplateExpr),
    {
        visit(expr);
        match expr {
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "include" | "template") =>
            {
                // Values passed as helper arguments configure that helper;
                // they are not rendered directly by the caller expression.
            }
            TemplateExpr::Call { args, .. } => {
                for arg in args {
                    Self::walk_expr_excluding_helper_call_args(arg, visit);
                }
            }
            TemplateExpr::Selector { operand, .. } => {
                Self::walk_expr_excluding_helper_call_args(operand, visit);
            }
            TemplateExpr::Pipeline(stages) => {
                for stage in stages {
                    Self::walk_expr_excluding_helper_call_args(stage, visit);
                }
            }
            TemplateExpr::Parenthesized(inner)
            | TemplateExpr::VariableDefinition { value: inner, .. }
            | TemplateExpr::Assignment { value: inner, .. } => {
                Self::walk_expr_excluding_helper_call_args(inner, visit);
            }
            TemplateExpr::Literal(_)
            | TemplateExpr::Field(_)
            | TemplateExpr::Variable(_)
            | TemplateExpr::Unknown(_) => {}
        }
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

    fn define_body_source(&self, name: &str) -> Option<&str> {
        self.ir_context
            .inner
            .define_body_sources
            .get(name)
            .map(String::as_str)
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
        let Some(tree) = Self::define_body_tree_from_cache(
            name,
            &self.ir_context.inner.define_body_sources,
            &self.ir_context.inner.define_tree_cache,
        ) else {
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
        let mut nested =
            SymbolicWalker::new_with_context(src, self.defines, self.ir_context.clone())
                .with_initial_guards(self.guards.clone())
                .with_inline_stack(stack)
                .with_inline_helpers_in_fragments(true)
                .with_helper_bindings(bindings)
                .with_chart_value_defaults(self.chart_value_defaults.clone());
        let uses = nested.run(&tree);
        self.uses.extend(uses);
        true
    }

    #[tracing::instrument(skip_all)]
    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        let Ok(text) = node.utf8_text(self.source.as_bytes()) else {
            return;
        };

        self.inline_static_file_templates_from_helper_calls(text);

        let enclosing_action_text = self.enclosing_action_text(node);
        let kind = if enclosing_action_text
            .as_deref()
            .is_some_and(is_fragment_expr)
            || is_fragment_expr(text)
        {
            ValueKind::Fragment
        } else {
            ValueKind::Scalar
        };

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
            if path.0.is_empty()
                && let Some(inline_path) = self.inline_mapping_value_path(node)
            {
                path = inline_path;
            }
        }
        if self.output_inside_block_scalar {
            path = YamlPath(Vec::new());
        }

        let helper_inlined = self.inline_exact_helper_call(text);

        let default_fallback_values = self.resolved_default_fallback_paths_in_context(text);
        let mut values: BTreeSet<String> = self
            .resolved_values_paths_in_context(text)
            .into_iter()
            .collect();
        values.extend(default_fallback_values.iter().cloned());
        let local_output_meta = self.local_alias_output_meta_for_text(text);

        let bound_values = self.extract_bound_values(text);

        let mut helper_output_values: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        let mut helper_fragment_output_values: Vec<String> = Vec::new();
        let mut helper_fragment_output_uses: Vec<HelperFragmentOutputUse> = Vec::new();
        let mut helper_guard_values: BTreeSet<String> = BTreeSet::new();
        let mut helper_type_hints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut suppress_direct_values: BTreeSet<String> = BTreeSet::new();
        if !helper_inlined {
            let bound = self.analyze_bound_helper_calls(text);
            helper_output_values.extend(bound.output);
            helper_fragment_output_uses.extend(bound.fragment_output_uses);
            if kind == ValueKind::Fragment {
                helper_fragment_output_values.extend(bound.fragment_output);
                helper_fragment_output_values
                    .extend(self.analyze_bound_fragment_helper_calls_in_context(text));
            }
            helper_guard_values.extend(bound.guard_paths);
            extend_type_hints(&mut helper_type_hints, bound.type_hints);
            suppress_direct_values.extend(bound.suppress_roots);
            // Stash chart-level `set X "K" (X.K | default V)` mutations
            // discovered in any helper called from this text. Subsequent
            // `emit_use` calls in this walker (same template scope) will
            // attach `Guard::Default { path }` for matching reads,
            // modeling that the helper's `set` has already run by the
            // time those reads are evaluated.
            self.chart_value_defaults.extend(bound.chart_defaults);
            helper_fragment_output_values.sort();
            helper_fragment_output_values.dedup();
        }

        if values.is_empty()
            && bound_values.is_empty()
            && helper_output_values.is_empty()
            && helper_fragment_output_values.is_empty()
            && helper_fragment_output_uses.is_empty()
            && helper_guard_values.is_empty()
            && helper_type_hints.is_empty()
        {
            return;
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
            let default_guard = Guard::Default { path: v.clone() };
            let mut extra_guards: Vec<Guard> = Vec::new();
            if let Some(meta) = local_output_meta.get(&v) {
                extra_guards.extend(
                    meta.guards
                        .iter()
                        .cloned()
                        .map(|path| Guard::Truthy { path }),
                );
                if meta.defaulted {
                    extra_guards.push(default_guard.clone());
                }
            }
            if default_fallback_values.contains(&v) {
                extra_guards.push(default_guard);
            }
            if extra_guards.is_empty() {
                self.emit_use(v, emit_path, kind);
            } else {
                self.emit_use_with_extra_guards(v, emit_path, kind, &extra_guards);
            }
        }

        for v in bound_values {
            self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
        }

        let helper_call_caller_scalar_path = !helper_inlined
            && !in_mapping_key
            && !path.0.is_empty()
            && !helper_output_values.is_empty()
            && helper_fragment_output_values.is_empty()
            && helper_fragment_output_uses.is_empty()
            && kind == ValueKind::Scalar
            && self.output_node_is_entire_scalar_value(node);
        let helper_call_caller_fragment_path = !helper_inlined
            && !in_mapping_key
            && !path.0.is_empty()
            && (!helper_fragment_output_values.is_empty()
                || !helper_fragment_output_uses.is_empty())
            && kind == ValueKind::Fragment;
        let helper_call_caller_structured_path = !helper_inlined
            && !in_mapping_key
            && !path.0.is_empty()
            && !helper_fragment_output_uses.is_empty()
            && (kind == ValueKind::Fragment
                || (kind == ValueKind::Scalar && self.output_node_is_entire_scalar_value(node)));
        let structured_fragment_sources: BTreeSet<String> = helper_fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        let mut helper_rendered_sources = structured_fragment_sources.clone();
        helper_rendered_sources.extend(helper_output_values.keys().cloned());
        helper_rendered_sources.extend(helper_fragment_output_values.iter().cloned());

        for (v, meta) in &helper_output_values {
            if structured_fragment_sources.contains(v) {
                continue;
            }
            let has_rendered_descendant =
                Self::values_path_has_descendant(v, &helper_rendered_sources);
            if helper_call_caller_scalar_path && !has_rendered_descendant {
                let extra_guards = Self::helper_output_extra_guards(v, meta);
                self.emit_use_with_extra_guards(v.clone(), path.clone(), kind, &extra_guards);
            } else {
                let extra_guards = Self::helper_dependency_extra_guards(meta);
                self.emit_helper_use_with_extra_guards(v.clone(), &extra_guards);
            }
        }

        for output in helper_fragment_output_uses {
            let extra_guards = Self::helper_output_extra_guards(&output.source_expr, &output.meta);
            let has_rendered_descendant =
                Self::values_path_has_descendant(&output.source_expr, &helper_rendered_sources);
            if helper_call_caller_structured_path && !has_rendered_descendant {
                let emit_path = Self::helper_fragment_output_path(&path, &output.relative_path);
                self.emit_use_with_extra_guards(
                    output.source_expr,
                    emit_path,
                    output.kind,
                    &extra_guards,
                );
            } else {
                let dependency_guards = Self::helper_dependency_extra_guards(&output.meta);
                self.emit_helper_use_with_extra_guards(output.source_expr, &dependency_guards);
            }
        }

        for v in helper_fragment_output_values {
            if structured_fragment_sources.contains(&v) {
                continue;
            }
            let has_rendered_descendant =
                Self::values_path_has_descendant(&v, &helper_rendered_sources);
            if helper_call_caller_fragment_path && !has_rendered_descendant {
                self.emit_use(v, path.clone(), kind);
            } else {
                self.emit_helper_use(v);
            }
        }

        for v in helper_guard_values {
            self.emit_helper_use(v);
        }

        for (path, schema_types) in helper_type_hints {
            for schema_type in schema_types {
                self.emit_use_with_extra_guards(
                    path.clone(),
                    YamlPath(Vec::new()),
                    ValueKind::Scalar,
                    &[Guard::TypeIs {
                        path: path.clone(),
                        schema_type,
                    }],
                );
            }
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

    fn enclosing_action_text(&self, node: tree_sitter::Node<'_>) -> Option<String> {
        let mut current = node;
        loop {
            match current.kind() {
                "template_action" => {
                    return current
                        .utf8_text(self.source.as_bytes())
                        .ok()
                        .map(std::string::ToString::to_string);
                }
                "if_action" | "with_action" | "range_action" => return None,
                _ => {
                    current = current.parent()?;
                }
            }
        }
    }

    fn inline_mapping_value_path(&self, node: tree_sitter::Node<'_>) -> Option<YamlPath> {
        let start = node.start_byte();
        let end = node.end_byte();
        let line_start = self.source[..start].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = self.source[end..]
            .find('\n')
            .map_or(self.source.len(), |idx| end + idx);
        let line = &self.source[line_start..line_end];
        let rel_start = start - line_start;
        let colon_offset = first_mapping_colon_offset(line)?;
        if rel_start <= colon_offset {
            return None;
        }
        let trimmed = line.trim_start();
        let key = parse_yaml_key(trimmed)?.key;
        let mut path = self.shape.current_path();
        if path.0.last().is_none_or(|segment| segment != &key) {
            path.0.push(key);
        }
        Some(path)
    }

    fn output_node_is_entire_scalar_value(&self, node: tree_sitter::Node<'_>) -> bool {
        fn normalize_value_text(value_text: &str) -> &str {
            let trimmed = value_text.trim();
            let unquoted = if trimmed.len() >= 2
                && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
                    || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
            {
                &trimmed[1..trimmed.len() - 1]
            } else {
                trimmed
            };

            let Some(rest) = unquoted.strip_prefix("{{") else {
                return unquoted.trim();
            };
            let rest = rest.strip_prefix('-').unwrap_or(rest);
            let Some(rest) = rest.strip_suffix("}}") else {
                return unquoted.trim();
            };
            let rest = rest.strip_suffix('-').unwrap_or(rest);
            rest.trim()
        }

        let start = node.start_byte();
        let end = node.end_byte();
        let line_start = self.source[..start].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = self.source[end..]
            .find('\n')
            .map_or(self.source.len(), |idx| end + idx);
        let line = &self.source[line_start..line_end];
        let rel_start = start - line_start;
        let rel_end = end - line_start;
        let node_text = &line[rel_start..rel_end];

        if let Some(colon_offset) = first_mapping_colon_offset(line) {
            if rel_start <= colon_offset {
                return false;
            }
            let value_text = line[colon_offset + 1..].trim();
            return normalize_value_text(value_text) == node_text.trim();
        }

        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('-') {
            return normalize_value_text(rest.trim_start()) == node_text.trim();
        }

        false
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn parse_expr_text(text: &str) -> Vec<TemplateExpr> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let normalized = if trimmed.starts_with("{{") {
            trimmed.to_string()
        } else {
            format!("{{{{ {trimmed} }}}}")
        };

        if let Some(cached) =
            TEMPLATE_EXPR_CACHE.with(|cache| cache.borrow().get(&normalized).cloned())
        {
            return cached;
        }

        let parsed = parse_action_expressions(&normalized);
        TEMPLATE_EXPR_CACHE.with(|cache| {
            cache.borrow_mut().insert(normalized, parsed.clone());
        });
        parsed
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
                if let TemplateExpr::Variable(var) = operand.as_ref()
                    && var.is_empty()
                    && let Some((head, tail)) = path.split_first()
                    && let Some(binding) = bindings.get(head)
                {
                    return Self::apply_binding(binding, tail);
                }
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
        let binding = Self::binding_from_bound_segments(segments, bindings)?;
        let paths = Self::helper_binding_paths(&binding);
        let mut paths = paths.into_iter();
        let first = paths.next()?;
        if paths.next().is_none() {
            Some(first)
        } else {
            None
        }
    }

    fn binding_from_bound_segments(
        segments: &[String],
        bindings: &HashMap<String, HelperBinding>,
    ) -> Option<HelperBinding> {
        let (first, rest) = segments.split_first()?;
        let binding = bindings.get(first)?;
        Self::apply_binding_to_binding(binding, rest)
    }

    fn helper_binding_paths(binding: &HelperBinding) -> BTreeSet<String> {
        match binding {
            HelperBinding::ValuesPath(path) => [path.clone()].into_iter().collect(),
            HelperBinding::OutputSet(outputs) => outputs.keys().cloned().collect(),
            HelperBinding::PathSet(paths) => paths.clone(),
            HelperBinding::Dict(map) => map.values().flat_map(Self::helper_binding_paths).collect(),
            HelperBinding::List(items) => {
                items.iter().flat_map(Self::helper_binding_paths).collect()
            }
            HelperBinding::Overlay { entries, fallback } => entries
                .values()
                .flat_map(Self::helper_binding_paths)
                .chain(Self::helper_binding_paths(fallback).into_iter())
                .collect(),
            HelperBinding::Choice(choices) => choices
                .iter()
                .flat_map(Self::helper_binding_paths)
                .collect(),
            HelperBinding::RootContext | HelperBinding::Unknown => BTreeSet::new(),
        }
    }

    fn helper_item_binding(binding: &HelperBinding) -> Option<HelperBinding> {
        match binding {
            HelperBinding::ValuesPath(path) => {
                if path.is_empty() {
                    Some(HelperBinding::ValuesPath("*".to_string()))
                } else {
                    Some(HelperBinding::ValuesPath(format!("{path}.*")))
                }
            }
            HelperBinding::PathSet(paths) => Some(HelperBinding::PathSet(
                paths
                    .iter()
                    .map(|path| {
                        if path.is_empty() {
                            "*".to_string()
                        } else {
                            format!("{path}.*")
                        }
                    })
                    .collect(),
            )),
            HelperBinding::OutputSet(outputs) => Some(HelperBinding::OutputSet(
                outputs
                    .iter()
                    .map(|(path, meta)| (path.clone(), meta.clone()))
                    .collect(),
            )),
            HelperBinding::Dict(entries) => {
                Self::helper_choice(entries.values().cloned().collect())
            }
            HelperBinding::List(items) => Self::helper_choice(items.clone()),
            HelperBinding::Overlay { entries, fallback } => {
                let mut choices: Vec<_> = entries.values().cloned().collect();
                if let Some(fallback_item) = Self::helper_item_binding(fallback) {
                    choices.push(fallback_item);
                }
                Self::helper_choice(choices)
            }
            HelperBinding::Choice(choices) => Self::helper_choice(
                choices
                    .iter()
                    .filter_map(Self::helper_item_binding)
                    .collect(),
            ),
            HelperBinding::RootContext | HelperBinding::Unknown => None,
        }
    }

    fn helper_binding_definitely_nonempty_iterable(binding: &HelperBinding) -> bool {
        match binding {
            HelperBinding::List(items) => !items.is_empty(),
            HelperBinding::Choice(choices) => {
                !choices.is_empty()
                    && choices
                        .iter()
                        .all(Self::helper_binding_definitely_nonempty_iterable)
            }
            _ => false,
        }
    }

    fn helper_choice(bindings: Vec<HelperBinding>) -> Option<HelperBinding> {
        let mut choices = BTreeSet::new();
        for binding in bindings {
            match binding {
                HelperBinding::Choice(inner) => choices.extend(inner),
                HelperBinding::Unknown => {}
                other => {
                    choices.insert(other);
                }
            }
        }
        match choices.len() {
            0 => None,
            1 => choices.into_iter().next(),
            _ => Some(HelperBinding::Choice(choices)),
        }
    }

    fn merge_helper_bindings(bindings: Vec<HelperBinding>) -> Option<HelperBinding> {
        let mut map = BTreeMap::new();
        let mut non_dict_bindings = Vec::new();

        let mut pending = bindings;
        while let Some(binding) = pending.pop() {
            match binding {
                HelperBinding::Choice(choices) => {
                    pending.extend(choices);
                }
                HelperBinding::Dict(entries) => {
                    for (key, value) in entries {
                        let merged = match map.remove(&key) {
                            Some(existing) => Self::helper_choice(vec![existing, value]),
                            None => Some(value),
                        };
                        if let Some(merged) = merged {
                            map.insert(key, merged);
                        }
                    }
                }
                other => {
                    non_dict_bindings.push(other);
                }
            }
        }

        let fallback = Self::helper_choice(non_dict_bindings);
        match (map.is_empty(), fallback) {
            (true, None) => None,
            (false, None) => Some(HelperBinding::Dict(map)),
            (true, Some(fallback)) => Some(fallback),
            (false, Some(fallback)) => Some(HelperBinding::Overlay {
                entries: map,
                fallback: Box::new(fallback),
            }),
        }
    }

    fn apply_binding_to_binding(binding: &HelperBinding, rest: &[String]) -> Option<HelperBinding> {
        if rest.is_empty() {
            return Some(binding.clone());
        }

        match binding {
            HelperBinding::ValuesPath(prefix) => {
                if prefix.is_empty() {
                    Some(HelperBinding::ValuesPath(rest.join(".")))
                } else {
                    Some(HelperBinding::ValuesPath(format!(
                        "{prefix}.{}",
                        rest.join(".")
                    )))
                }
            }
            HelperBinding::RootContext => match rest {
                [head] if head == "Values" => Some(HelperBinding::ValuesPath(String::new())),
                [head, tail @ ..] if head == "Values" => {
                    Some(HelperBinding::ValuesPath(tail.join(".")))
                }
                _ => None,
            },
            HelperBinding::Unknown => None,
            HelperBinding::OutputSet(outputs) => Some(HelperBinding::OutputSet(
                outputs
                    .iter()
                    .map(|(path, meta)| (path.clone(), meta.clone()))
                    .collect(),
            )),
            HelperBinding::PathSet(paths) => {
                let appended = paths
                    .iter()
                    .map(|path| {
                        if rest.is_empty() {
                            path.clone()
                        } else if path.is_empty() {
                            rest.join(".")
                        } else {
                            format!("{path}.{}", rest.join("."))
                        }
                    })
                    .collect();
                Some(HelperBinding::PathSet(appended))
            }
            HelperBinding::Dict(map) => {
                let (head, tail) = rest.split_first()?;
                let binding = map.get(head)?;
                Self::apply_binding_to_binding(binding, tail)
            }
            HelperBinding::List(items) => {
                let (head, tail) = rest.split_first()?;
                let index = head.parse::<usize>().ok()?;
                let binding = items.get(index)?;
                Self::apply_binding_to_binding(binding, tail)
            }
            HelperBinding::Overlay { entries, fallback } => {
                let (head, tail) = rest.split_first()?;
                if let Some(binding) = entries.get(head) {
                    Self::apply_binding_to_binding(binding, tail)
                } else {
                    Self::apply_binding_to_binding(fallback, rest)
                }
            }
            HelperBinding::Choice(choices) => Self::helper_choice(
                choices
                    .iter()
                    .filter_map(|choice| Self::apply_binding_to_binding(choice, rest))
                    .collect(),
            ),
        }
    }

    fn apply_binding(binding: &HelperBinding, rest: &[String]) -> Option<String> {
        let binding = Self::apply_binding_to_binding(binding, rest)?;
        let paths = Self::helper_binding_paths(&binding);
        let mut paths = paths.into_iter();
        let first = paths.next()?;
        if paths.next().is_none() {
            Some(first)
        } else {
            None
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
            TemplateExpr::Variable(_) => None,
            TemplateExpr::Selector { operand, path } => {
                let binding = Self::binding_from_expr(operand, outer, current_dot)?;
                Self::apply_binding_to_binding(&binding, path)
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "list" | "tuple") =>
            {
                let mut items = Vec::new();
                for arg in args {
                    items.push(
                        Self::binding_from_expr(arg, outer, current_dot)
                            .unwrap_or(HelperBinding::Unknown),
                    );
                }
                Some(HelperBinding::List(items))
            }
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut map = BTreeMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                        &args[index]
                    else {
                        index += 1;
                        continue;
                    };
                    let binding = Self::binding_from_expr(&args[index + 1], outer, current_dot)
                        .unwrap_or(HelperBinding::Unknown);
                    map.insert(key.clone(), binding);
                    index += 2;
                }
                Some(HelperBinding::Dict(map))
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "merge" | "mergeOverwrite") =>
            {
                let mut bindings = Vec::new();
                for arg in args {
                    if let Some(binding) = Self::binding_from_expr(arg, outer, current_dot) {
                        bindings.push(binding);
                    }
                }
                Self::merge_helper_bindings(bindings)
            }
            TemplateExpr::Call { function, args } if function == "coalesce" => {
                let mut choices = Vec::new();
                for arg in args {
                    if let Some(binding) = Self::binding_from_expr(arg, outer, current_dot) {
                        choices.push(binding);
                    }
                }
                Self::helper_choice(choices)
            }
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let mut choices = Vec::new();
                if let Some(primary) = Self::binding_from_expr(&args[1], outer, current_dot) {
                    choices.push(primary);
                }
                if let Some(fallback) = Self::binding_from_expr(&args[0], outer, current_dot) {
                    choices.push(fallback);
                }
                Self::helper_choice(choices)
            }
            TemplateExpr::Call { function, args } if function == "ternary" => {
                let mut choices = Vec::new();
                for arg in args.iter().take(2) {
                    if let Some(binding) = Self::binding_from_expr(arg, outer, current_dot) {
                        choices.push(binding);
                    }
                }
                Self::helper_choice(choices)
            }
            TemplateExpr::Pipeline(stages) => {
                let mut current = Self::binding_from_expr(&stages[0], outer, current_dot);
                for stage in &stages[1..] {
                    let TemplateExpr::Call { function, args } = stage else {
                        continue;
                    };
                    current = match function.as_str() {
                        "default" => {
                            let mut choices = Vec::new();
                            if let Some(current) = current {
                                choices.push(current);
                            }
                            for arg in args {
                                if let Some(binding) =
                                    Self::binding_from_expr(arg, outer, current_dot)
                                {
                                    choices.push(binding);
                                }
                            }
                            Self::helper_choice(choices)
                        }
                        "merge" | "mergeOverwrite" => {
                            let mut bindings = Vec::new();
                            if let Some(current) = current {
                                bindings.push(current);
                            }
                            for arg in args {
                                if let Some(binding) =
                                    Self::binding_from_expr(arg, outer, current_dot)
                                {
                                    bindings.push(binding);
                                }
                            }
                            Self::merge_helper_bindings(bindings)
                        }
                        "toYaml" | "fromYaml" | "quote" | "toString" | "deepCopy" | "tpl"
                        | "nindent" | "indent" => current,
                        _ => None,
                    };
                }
                current
            }
            TemplateExpr::Call { function, args } if function == "index" => {
                let mut binding = Self::binding_from_expr(args.first()?, outer, current_dot)?;
                for arg in &args[1..] {
                    let segment = match arg {
                        TemplateExpr::Literal(
                            Literal::String(value) | Literal::RawString(value),
                        ) => value.clone(),
                        TemplateExpr::Literal(Literal::Int(value)) => value.to_string(),
                        _ => return None,
                    };
                    binding = match &binding {
                        HelperBinding::ValuesPath(_)
                        | HelperBinding::RootContext
                        | HelperBinding::Unknown
                        | HelperBinding::OutputSet(_)
                        | HelperBinding::PathSet(_)
                        | HelperBinding::Dict(_)
                        | HelperBinding::List(_)
                        | HelperBinding::Overlay { .. }
                        | HelperBinding::Choice(_) => {
                            Self::apply_binding_to_binding(&binding, &[segment])?
                        }
                    };
                }
                Some(binding)
            }
            TemplateExpr::Field(path) => {
                // Helper bindings take priority: a `Field` whose head
                // names a helper-bound key resolves through that
                // binding (e.g. a `dict "ctx" .Values.cfg` helper call
                // binds `ctx`, so the callee's `.ctx.X` becomes
                // `cfg.X`).
                if let Some(bound) =
                    outer.and_then(|bindings| Self::binding_from_bound_segments(path, bindings))
                {
                    return Some(bound);
                }
                // Otherwise apply the field selector to the current dot.
                // This covers direct values dots (`with .Values.foo`) and
                // overlay dots produced by helper arguments like
                // `merge (dict "file" "...") .`, where explicit dict keys
                // shadow the fallback map but missing keys still resolve
                // through that fallback.
                if let Some(current_dot) = current_dot
                    && let Some(bound) = Self::apply_binding_to_binding(current_dot, path)
                {
                    return Some(bound);
                }
                None
            }
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
                    let binding = Self::binding_from_expr(&args[index + 1], outer, current_dot)
                        .unwrap_or(HelperBinding::Unknown);
                    bindings.insert(key.clone(), binding);
                    index += 2;
                }
                bindings
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "merge" | "mergeOverwrite") =>
            {
                let mut merged = HashMap::new();
                for arg in args {
                    match Self::binding_from_expr(arg, outer, current_dot) {
                        Some(HelperBinding::Dict(map)) => {
                            for (key, value) in map {
                                merged.insert(key, value);
                            }
                        }
                        Some(HelperBinding::RootContext) => {
                            if let Some(outer) = outer {
                                for (key, value) in outer {
                                    merged.insert(key.clone(), value.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                merged
            }
            _ => HashMap::new(),
        }
    }

    fn helper_binding_from_expr_with_fragment_locals(
        expr: &TemplateExpr,
        fragment_locals: &HashMap<String, FragmentBinding>,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> Option<HelperBinding> {
        match expr {
            TemplateExpr::Parenthesized(inner) => {
                Self::helper_binding_from_expr_with_fragment_locals(
                    inner,
                    fragment_locals,
                    outer,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                )
            }
            TemplateExpr::Variable(var) if !var.is_empty() => fragment_locals
                .get(var)
                .and_then(Self::helper_binding_from_fragment_binding),
            TemplateExpr::Selector { operand, path } => {
                if let TemplateExpr::Variable(var) = operand.as_ref()
                    && !var.is_empty()
                    && let Some(binding) = fragment_locals
                        .get(var)
                        .and_then(Self::helper_binding_from_fragment_binding)
                {
                    return Self::apply_binding_to_binding(&binding, path);
                }
                Self::binding_from_expr(expr, outer, current_dot)
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "include" | "template") =>
            {
                let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                    return None;
                };
                let analysis = Self::analyze_bound_helper_call_with_fragment_locals(
                    name,
                    args.get(1),
                    outer,
                    current_dot,
                    fragment_locals,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                Self::helper_binding_from_helper_analysis(analysis)
            }
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut map = BTreeMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                        &args[index]
                    else {
                        index += 1;
                        continue;
                    };
                    let binding = Self::helper_binding_from_expr_with_fragment_locals(
                        &args[index + 1],
                        fragment_locals,
                        outer,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    )
                    .unwrap_or(HelperBinding::Unknown);
                    map.insert(key.clone(), binding);
                    index += 2;
                }
                Some(HelperBinding::Dict(map))
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "list" | "tuple") =>
            {
                Some(HelperBinding::List(
                    args.iter()
                        .map(|arg| {
                            Self::helper_binding_from_expr_with_fragment_locals(
                                arg,
                                fragment_locals,
                                outer,
                                current_dot,
                                defines,
                                define_sources,
                                define_tree_cache,
                                fragment_helper_body_cache,
                                seen,
                            )
                            .unwrap_or(HelperBinding::Unknown)
                        })
                        .collect(),
                ))
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "merge" | "mergeOverwrite") =>
            {
                let bindings = args
                    .iter()
                    .filter_map(|arg| {
                        Self::helper_binding_from_expr_with_fragment_locals(
                            arg,
                            fragment_locals,
                            outer,
                            current_dot,
                            defines,
                            define_sources,
                            define_tree_cache,
                            fragment_helper_body_cache,
                            seen,
                        )
                    })
                    .collect();
                Self::merge_helper_bindings(bindings)
            }
            TemplateExpr::Pipeline(stages) => {
                let mut current = Self::helper_binding_from_expr_with_fragment_locals(
                    &stages[0],
                    fragment_locals,
                    outer,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                for stage in &stages[1..] {
                    let TemplateExpr::Call { function, args } = stage else {
                        continue;
                    };
                    current = match function.as_str() {
                        "default" => {
                            let mut bindings = Vec::new();
                            if let Some(current) = current {
                                bindings.push(current);
                            }
                            for arg in args {
                                if let Some(binding) =
                                    Self::helper_binding_from_expr_with_fragment_locals(
                                        arg,
                                        fragment_locals,
                                        outer,
                                        current_dot,
                                        defines,
                                        define_sources,
                                        define_tree_cache,
                                        fragment_helper_body_cache,
                                        seen,
                                    )
                                {
                                    bindings.push(binding);
                                }
                            }
                            Self::helper_choice(bindings)
                        }
                        "merge" | "mergeOverwrite" => {
                            let mut bindings = Vec::new();
                            if let Some(current) = current {
                                bindings.push(current);
                            }
                            for arg in args {
                                if let Some(binding) =
                                    Self::helper_binding_from_expr_with_fragment_locals(
                                        arg,
                                        fragment_locals,
                                        outer,
                                        current_dot,
                                        defines,
                                        define_sources,
                                        define_tree_cache,
                                        fragment_helper_body_cache,
                                        seen,
                                    )
                                {
                                    bindings.push(binding);
                                }
                            }
                            Self::merge_helper_bindings(bindings)
                        }
                        "toYaml" | "fromYaml" | "toJson" | "fromJson" | "quote" | "toString"
                        | "deepCopy" | "tpl" | "nindent" | "indent" => current,
                        _ => None,
                    };
                }
                current
            }
            _ => Self::binding_from_expr(expr, outer, current_dot),
        }
    }

    fn bindings_for_helper_arg_with_fragment_locals(
        arg: Option<&TemplateExpr>,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> HashMap<String, HelperBinding> {
        let Some(arg) = arg else {
            return HashMap::new();
        };

        match arg {
            TemplateExpr::Parenthesized(inner) => {
                Self::bindings_for_helper_arg_with_fragment_locals(
                    Some(inner),
                    outer,
                    current_dot,
                    fragment_locals,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                )
            }
            TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
            TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut bindings = HashMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                        &args[index]
                    else {
                        index += 1;
                        continue;
                    };
                    let binding = Self::helper_binding_from_expr_with_fragment_locals(
                        &args[index + 1],
                        fragment_locals,
                        outer,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    )
                    .unwrap_or(HelperBinding::Unknown);
                    bindings.insert(key.clone(), binding);
                    index += 2;
                }
                bindings
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "merge" | "mergeOverwrite") =>
            {
                let mut merged = HashMap::new();
                for arg in args {
                    match Self::helper_binding_from_expr_with_fragment_locals(
                        arg,
                        fragment_locals,
                        outer,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    ) {
                        Some(HelperBinding::Dict(map)) => {
                            for (key, value) in map {
                                merged.insert(key, value);
                            }
                        }
                        Some(HelperBinding::RootContext) => {
                            if let Some(outer) = outer {
                                for (key, value) in outer {
                                    merged.insert(key.clone(), value.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                merged
            }
            _ => HashMap::new(),
        }
    }

    fn fragment_binding_from_helper_binding(binding: &HelperBinding) -> FragmentBinding {
        match binding {
            HelperBinding::ValuesPath(path) => FragmentBinding::ValuesPath(path.clone()),
            HelperBinding::RootContext => FragmentBinding::RootContext,
            HelperBinding::Unknown => FragmentBinding::Unknown,
            HelperBinding::OutputSet(outputs) => {
                FragmentBinding::OutputSet(outputs.keys().cloned().collect())
            }
            HelperBinding::PathSet(paths) => FragmentBinding::PathSet(paths.clone()),
            HelperBinding::Dict(map) => FragmentBinding::Dict(
                map.iter()
                    .map(|(key, value)| {
                        (
                            key.clone(),
                            Self::fragment_binding_from_helper_binding(value),
                        )
                    })
                    .collect(),
            ),
            HelperBinding::List(items) => FragmentBinding::List(
                items
                    .iter()
                    .map(Self::fragment_binding_from_helper_binding)
                    .collect(),
            ),
            HelperBinding::Overlay { entries, fallback } => FragmentBinding::Overlay {
                entries: entries
                    .iter()
                    .map(|(key, value)| {
                        (
                            key.clone(),
                            Self::fragment_binding_from_helper_binding(value),
                        )
                    })
                    .collect(),
                fallback: Box::new(Self::fragment_binding_from_helper_binding(fallback)),
            },
            HelperBinding::Choice(choices) => FragmentBinding::Choice(
                choices
                    .iter()
                    .map(Self::fragment_binding_from_helper_binding)
                    .collect(),
            ),
        }
    }

    fn helper_binding_from_fragment_binding(binding: &FragmentBinding) -> Option<HelperBinding> {
        match binding {
            FragmentBinding::ValuesPath(path) => Some(HelperBinding::ValuesPath(path.clone())),
            FragmentBinding::ValuesRoot => Some(HelperBinding::ValuesPath(String::new())),
            FragmentBinding::RootContext => Some(HelperBinding::RootContext),
            FragmentBinding::Unknown | FragmentBinding::StringSet(_) => {
                Some(HelperBinding::Unknown)
            }
            FragmentBinding::OutputSet(paths) => Some(HelperBinding::OutputSet(
                paths
                    .iter()
                    .map(|path| (path.clone(), HelperOutputMeta::default()))
                    .collect(),
            )),
            FragmentBinding::PathSet(paths) => Some(HelperBinding::PathSet(paths.clone())),
            FragmentBinding::Dict(map) => Some(HelperBinding::Dict(
                map.iter()
                    .map(|(key, value)| {
                        Some((
                            key.clone(),
                            Self::helper_binding_from_fragment_binding(value)?,
                        ))
                    })
                    .collect::<Option<BTreeMap<_, _>>>()?,
            )),
            FragmentBinding::List(items) => Some(HelperBinding::List(
                items
                    .iter()
                    .map(Self::helper_binding_from_fragment_binding)
                    .collect::<Option<Vec<_>>>()?,
            )),
            FragmentBinding::Overlay { entries, fallback } => Some(HelperBinding::Overlay {
                entries: entries
                    .iter()
                    .map(|(key, value)| {
                        Some((
                            key.clone(),
                            Self::helper_binding_from_fragment_binding(value)?,
                        ))
                    })
                    .collect::<Option<BTreeMap<_, _>>>()?,
                fallback: Box::new(Self::helper_binding_from_fragment_binding(fallback)?),
            }),
            FragmentBinding::Choice(choices) => Self::helper_choice(
                choices
                    .iter()
                    .filter_map(Self::helper_binding_from_fragment_binding)
                    .collect(),
            ),
        }
    }

    fn fragment_choice(bindings: Vec<FragmentBinding>) -> Option<FragmentBinding> {
        let mut flat = BTreeSet::new();
        for binding in bindings {
            match binding {
                FragmentBinding::Choice(inner) => flat.extend(inner),
                other => {
                    flat.insert(other);
                }
            }
        }
        match flat.len() {
            0 => None,
            1 => flat.into_iter().next(),
            _ => Some(FragmentBinding::Choice(flat)),
        }
    }

    fn fragment_paths(binding: &FragmentBinding) -> BTreeSet<String> {
        match binding {
            FragmentBinding::ValuesPath(path) => [path.clone()].into_iter().collect(),
            FragmentBinding::OutputSet(paths) => paths.clone(),
            FragmentBinding::PathSet(paths) => paths.clone(),
            FragmentBinding::Overlay { entries, fallback } => entries
                .values()
                .flat_map(Self::fragment_paths)
                .chain(Self::fragment_paths(fallback).into_iter())
                .collect(),
            FragmentBinding::Choice(choices) => {
                choices.iter().flat_map(Self::fragment_paths).collect()
            }
            _ => BTreeSet::new(),
        }
    }

    fn fragment_strings(binding: &FragmentBinding) -> BTreeSet<String> {
        match binding {
            FragmentBinding::StringSet(values) => values.clone(),
            FragmentBinding::Choice(choices) => {
                choices.iter().flat_map(Self::fragment_strings).collect()
            }
            _ => BTreeSet::new(),
        }
    }

    fn fragment_union(
        left: Option<FragmentBinding>,
        right: Option<FragmentBinding>,
    ) -> Option<FragmentBinding> {
        match (left, right) {
            (Some(left), Some(right)) => Self::fragment_choice(vec![left, right]),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        }
    }

    fn merge_fragment_bindings(bindings: Vec<FragmentBinding>) -> Option<FragmentBinding> {
        let mut map = BTreeMap::new();
        let mut non_dict_value_paths = BTreeSet::new();
        let mut non_dict_output_paths = BTreeSet::new();
        let mut non_dict_strings = BTreeSet::new();

        let mut pending = bindings;
        while let Some(binding) = pending.pop() {
            match binding {
                FragmentBinding::Choice(choices) => {
                    pending.extend(choices);
                }
                FragmentBinding::Dict(entries) => {
                    for (key, value) in entries {
                        let merged = Self::fragment_union(map.remove(&key), Some(value));
                        if let Some(merged) = merged {
                            map.insert(key, merged);
                        }
                    }
                }
                FragmentBinding::Overlay { entries, fallback } => {
                    pending.push(*fallback);
                    for (key, value) in entries {
                        let merged = Self::fragment_union(map.remove(&key), Some(value));
                        if let Some(merged) = merged {
                            map.insert(key, merged);
                        }
                    }
                }
                FragmentBinding::ValuesPath(path) => {
                    non_dict_value_paths.insert(path);
                }
                FragmentBinding::ValuesRoot => {
                    non_dict_value_paths.insert(String::new());
                }
                FragmentBinding::PathSet(paths) => {
                    non_dict_value_paths.extend(paths);
                }
                FragmentBinding::OutputSet(paths) => {
                    non_dict_output_paths.extend(paths);
                }
                FragmentBinding::StringSet(strings) => {
                    non_dict_strings.extend(strings);
                }
                FragmentBinding::RootContext
                | FragmentBinding::Unknown
                | FragmentBinding::List(_) => {}
            }
        }

        let mut fallback_choices = Vec::new();
        if !non_dict_value_paths.is_empty() {
            fallback_choices.push(FragmentBinding::PathSet(non_dict_value_paths));
        }
        if !non_dict_output_paths.is_empty() {
            fallback_choices.push(FragmentBinding::OutputSet(non_dict_output_paths));
        }
        if !non_dict_strings.is_empty() {
            fallback_choices.push(FragmentBinding::StringSet(non_dict_strings));
        }
        let fallback = Self::fragment_choice(fallback_choices);

        if map.is_empty() {
            fallback
        } else if let Some(fallback) = fallback {
            Some(FragmentBinding::Overlay {
                entries: map,
                fallback: Box::new(fallback),
            })
        } else {
            Some(FragmentBinding::Dict(map))
        }
    }

    fn remove_fragment_paths(
        binding: FragmentBinding,
        remove: &BTreeSet<String>,
    ) -> Option<FragmentBinding> {
        if remove.is_empty() {
            return Some(binding);
        }

        match binding {
            FragmentBinding::ValuesPath(path) if remove.contains(&path) => None,
            FragmentBinding::ValuesPath(_)
            | FragmentBinding::ValuesRoot
            | FragmentBinding::RootContext
            | FragmentBinding::Unknown
            | FragmentBinding::StringSet(_) => Some(binding),
            FragmentBinding::OutputSet(mut paths) => {
                paths.retain(|path| !remove.contains(path));
                if paths.is_empty() {
                    None
                } else {
                    Some(FragmentBinding::OutputSet(paths))
                }
            }
            FragmentBinding::PathSet(mut paths) => {
                paths.retain(|path| !remove.contains(path));
                if paths.is_empty() {
                    None
                } else {
                    Some(FragmentBinding::PathSet(paths))
                }
            }
            FragmentBinding::Dict(entries) => {
                let entries = entries
                    .into_iter()
                    .filter_map(|(key, value)| {
                        Self::remove_fragment_paths(value, remove).map(|value| (key, value))
                    })
                    .collect::<BTreeMap<_, _>>();
                if entries.is_empty() {
                    None
                } else {
                    Some(FragmentBinding::Dict(entries))
                }
            }
            FragmentBinding::List(items) => {
                let items = items
                    .into_iter()
                    .filter_map(|item| Self::remove_fragment_paths(item, remove))
                    .collect::<Vec<_>>();
                if items.is_empty() {
                    None
                } else {
                    Some(FragmentBinding::List(items))
                }
            }
            FragmentBinding::Overlay { entries, fallback } => {
                let entries = entries
                    .into_iter()
                    .filter_map(|(key, value)| {
                        Self::remove_fragment_paths(value, remove).map(|value| (key, value))
                    })
                    .collect::<BTreeMap<_, _>>();
                match (
                    entries.is_empty(),
                    Self::remove_fragment_paths(*fallback, remove),
                ) {
                    (true, fallback) => fallback,
                    (false, Some(fallback)) => Some(FragmentBinding::Overlay {
                        entries,
                        fallback: Box::new(fallback),
                    }),
                    (false, None) => Some(FragmentBinding::Dict(entries)),
                }
            }
            FragmentBinding::Choice(choices) => Self::fragment_choice(
                choices
                    .into_iter()
                    .filter_map(|choice| Self::remove_fragment_paths(choice, remove))
                    .collect(),
            ),
        }
    }

    fn fragment_binding_for_output_path(
        source_expr: String,
        relative_path: &YamlPath,
    ) -> FragmentBinding {
        let mut binding = FragmentBinding::OutputSet([source_expr].into_iter().collect());
        for segment in relative_path.0.iter().rev() {
            binding = FragmentBinding::Dict(BTreeMap::from([(segment.clone(), binding)]));
        }
        binding
    }

    fn helper_binding_for_output_path(
        source_expr: String,
        relative_path: &YamlPath,
        meta: HelperOutputMeta,
    ) -> HelperBinding {
        let mut binding = HelperBinding::OutputSet(BTreeMap::from([(source_expr, meta)]));
        for segment in relative_path.0.iter().rev() {
            binding = HelperBinding::Dict(BTreeMap::from([(segment.clone(), binding)]));
        }
        binding
    }

    fn fragment_binding_from_helper_analysis(
        mut analysis: BoundHelperAnalysis,
    ) -> Option<FragmentBinding> {
        let structured_sources: BTreeSet<String> = analysis
            .fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        let mut rendered_sources = structured_sources.clone();
        rendered_sources.extend(analysis.fragment_output.iter().cloned());
        rendered_sources.extend(analysis.output.keys().cloned());
        let mut bindings = Vec::new();
        for output in analysis.fragment_output_uses.drain(..) {
            bindings.push(Self::fragment_binding_for_output_path(
                output.source_expr,
                &output.relative_path,
            ));
        }
        for source in analysis.fragment_output {
            if !structured_sources.contains(&source)
                && !Self::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
            }
        }
        for source in analysis.output.into_keys() {
            if !structured_sources.contains(&source)
                && !Self::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
            }
        }
        Self::merge_fragment_bindings(bindings)
    }

    fn helper_binding_from_helper_analysis(
        mut analysis: BoundHelperAnalysis,
    ) -> Option<HelperBinding> {
        let structured_sources: BTreeSet<String> = analysis
            .fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        let mut rendered_sources = structured_sources.clone();
        rendered_sources.extend(analysis.fragment_output.iter().cloned());
        rendered_sources.extend(analysis.output.keys().cloned());

        let mut bindings = Vec::new();
        for output in analysis.fragment_output_uses.drain(..) {
            bindings.push(Self::helper_binding_for_output_path(
                output.source_expr,
                &output.relative_path,
                output.meta,
            ));
        }
        for source in analysis.fragment_output {
            if !structured_sources.contains(&source)
                && !Self::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(HelperBinding::PathSet([source].into_iter().collect()));
            }
        }
        for (source, meta) in analysis.output {
            if !structured_sources.contains(&source)
                && !Self::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(HelperBinding::OutputSet(
                    [(source, meta)].into_iter().collect(),
                ));
            }
        }
        Self::merge_helper_bindings(bindings)
    }

    fn fragment_apply_binding(
        binding: &FragmentBinding,
        rest: &[String],
    ) -> Option<FragmentBinding> {
        match binding {
            FragmentBinding::ValuesPath(prefix) => {
                if rest.is_empty() {
                    Some(FragmentBinding::ValuesPath(prefix.clone()))
                } else if prefix.is_empty() {
                    Some(FragmentBinding::ValuesPath(rest.join(".")))
                } else {
                    Some(FragmentBinding::ValuesPath(format!(
                        "{prefix}.{}",
                        rest.join(".")
                    )))
                }
            }
            FragmentBinding::ValuesRoot => {
                if rest.is_empty() {
                    Some(FragmentBinding::ValuesRoot)
                } else {
                    Some(FragmentBinding::ValuesPath(rest.join(".")))
                }
            }
            FragmentBinding::RootContext => match rest {
                [head] if head == "Values" => Some(FragmentBinding::ValuesRoot),
                [head, tail @ ..] if head == "Values" => {
                    Some(FragmentBinding::ValuesPath(tail.join(".")))
                }
                _ => None,
            },
            FragmentBinding::Unknown => None,
            FragmentBinding::Dict(map) => {
                let (first, tail) = rest.split_first()?;
                let binding = map.get(first)?;
                if tail.is_empty() {
                    Some(binding.clone())
                } else {
                    Self::fragment_apply_binding(binding, tail)
                }
            }
            FragmentBinding::PathSet(paths) => {
                let appended: BTreeSet<String> = paths
                    .iter()
                    .map(|path| {
                        if rest.is_empty() {
                            path.clone()
                        } else if path.is_empty() {
                            rest.join(".")
                        } else {
                            format!("{path}.{}", rest.join("."))
                        }
                    })
                    .collect();
                Some(FragmentBinding::PathSet(appended))
            }
            FragmentBinding::OutputSet(paths) => Some(FragmentBinding::OutputSet(paths.clone())),
            FragmentBinding::Overlay { entries, fallback } => {
                let (first, tail) = rest.split_first()?;
                if let Some(binding) = entries.get(first) {
                    if tail.is_empty() {
                        Some(binding.clone())
                    } else {
                        Self::fragment_apply_binding(binding, tail)
                    }
                } else {
                    Self::fragment_apply_binding(fallback, rest)
                }
            }
            FragmentBinding::Choice(choices) => Self::fragment_choice(
                choices
                    .iter()
                    .filter_map(|choice| Self::fragment_apply_binding(choice, rest))
                    .collect(),
            ),
            FragmentBinding::List(_) | FragmentBinding::StringSet(_) => None,
        }
    }

    fn fragment_item_binding(binding: &FragmentBinding) -> Option<FragmentBinding> {
        match binding {
            FragmentBinding::ValuesPath(path) => {
                Some(FragmentBinding::ValuesPath(format!("{path}.*")))
            }
            FragmentBinding::ValuesRoot => Some(FragmentBinding::ValuesPath("*".to_string())),
            FragmentBinding::PathSet(paths) => Some(FragmentBinding::PathSet(
                paths
                    .iter()
                    .map(|path| {
                        if path.is_empty() {
                            "*".to_string()
                        } else {
                            format!("{path}.*")
                        }
                    })
                    .collect(),
            )),
            FragmentBinding::OutputSet(paths) => Some(FragmentBinding::OutputSet(paths.clone())),
            FragmentBinding::List(items) => Self::fragment_choice(items.clone()),
            FragmentBinding::Choice(choices) => Self::fragment_choice(
                choices
                    .iter()
                    .filter_map(Self::fragment_item_binding)
                    .collect(),
            ),
            FragmentBinding::RootContext
            | FragmentBinding::Unknown
            | FragmentBinding::Dict(_)
            | FragmentBinding::Overlay { .. }
            | FragmentBinding::StringSet(_) => None,
        }
    }

    fn fragment_binding_definitely_nonempty_iterable(binding: &FragmentBinding) -> bool {
        match binding {
            FragmentBinding::List(items) => !items.is_empty(),
            FragmentBinding::Choice(choices) => {
                !choices.is_empty()
                    && choices
                        .iter()
                        .all(Self::fragment_binding_definitely_nonempty_iterable)
            }
            _ => false,
        }
    }

    fn fragment_binding_from_outer_expr(
        expr: &TemplateExpr,
        outer_locals: Option<&HashMap<String, FragmentBinding>>,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> Option<FragmentBinding> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(FragmentBinding::ValuesPath(path));
        }

        match expr {
            TemplateExpr::Literal(Literal::String(value)) => Some(FragmentBinding::StringSet(
                [value.clone()].into_iter().collect(),
            )),
            TemplateExpr::Parenthesized(inner) => {
                Self::fragment_binding_from_outer_expr(inner, outer_locals, outer, current_dot)
            }
            TemplateExpr::Field(path) if path.is_empty() => {
                if let Some(bindings) = outer {
                    return Some(FragmentBinding::Dict(
                        bindings
                            .iter()
                            .map(|(key, binding)| {
                                (
                                    key.clone(),
                                    Self::fragment_binding_from_helper_binding(binding),
                                )
                            })
                            .collect(),
                    ));
                }
                current_dot
                    .map(Self::fragment_binding_from_helper_binding)
                    .or(Some(FragmentBinding::RootContext))
            }
            TemplateExpr::Field(path)
                if path.first().is_some_and(|segment| segment == "Values") =>
            {
                Some(FragmentBinding::ValuesPath(path[1..].join(".")))
            }
            TemplateExpr::Variable(var) if var.is_empty() => {
                if let Some(bindings) = outer {
                    return Some(FragmentBinding::Dict(
                        bindings
                            .iter()
                            .map(|(key, binding)| {
                                (
                                    key.clone(),
                                    Self::fragment_binding_from_helper_binding(binding),
                                )
                            })
                            .collect(),
                    ));
                }
                Some(FragmentBinding::RootContext)
            }
            TemplateExpr::Variable(var) if !var.is_empty() => {
                outer_locals.and_then(|locals| locals.get(var).cloned())
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "list" | "tuple") =>
            {
                let mut items = Vec::new();
                for arg in args {
                    items.push(
                        Self::fragment_binding_from_outer_expr(
                            arg,
                            outer_locals,
                            outer,
                            current_dot,
                        )
                        .unwrap_or(FragmentBinding::Unknown),
                    );
                }
                Some(FragmentBinding::List(items))
            }
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut map = BTreeMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key)) = &args[index] else {
                        index += 1;
                        continue;
                    };
                    if let Some(binding) = Self::fragment_binding_from_outer_expr(
                        &args[index + 1],
                        outer_locals,
                        outer,
                        current_dot,
                    ) {
                        map.insert(key.clone(), binding);
                    }
                    index += 2;
                }
                Some(FragmentBinding::Dict(map))
            }
            TemplateExpr::Call { function, args } if function == "coalesce" => {
                let mut choices = Vec::new();
                for arg in args {
                    if let Some(binding) = Self::fragment_binding_from_outer_expr(
                        arg,
                        outer_locals,
                        outer,
                        current_dot,
                    ) {
                        choices.push(binding);
                    }
                }
                Self::fragment_choice(choices)
            }
            TemplateExpr::Call { function, args } if function == "ternary" => {
                let mut choices = Vec::new();
                for arg in args.iter().take(2) {
                    if let Some(binding) = Self::fragment_binding_from_outer_expr(
                        arg,
                        outer_locals,
                        outer,
                        current_dot,
                    ) {
                        choices.push(binding);
                    }
                }
                Self::fragment_choice(choices)
            }
            _ => Self::binding_from_expr(expr, outer, current_dot)
                .map(|binding| Self::fragment_binding_from_helper_binding(&binding)),
        }
    }

    fn fragment_binding_from_expr(
        expr: &TemplateExpr,
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(FragmentBinding::ValuesPath(path));
        }

        match expr {
            TemplateExpr::Literal(Literal::String(value)) => Some(FragmentBinding::StringSet(
                [value.clone()].into_iter().collect(),
            )),
            TemplateExpr::Parenthesized(inner) => Self::fragment_binding_from_expr(
                inner,
                locals,
                current_dot,
                defines,
                define_sources,
                define_tree_cache,
                fragment_helper_body_cache,
                seen,
            ),
            TemplateExpr::Field(path)
                if path.first().is_some_and(|segment| segment == "Values") =>
            {
                Some(FragmentBinding::ValuesPath(path[1..].join(".")))
            }
            TemplateExpr::Field(path) if path.is_empty() => {
                current_dot.cloned().or(Some(FragmentBinding::RootContext))
            }
            TemplateExpr::Field(path) => {
                let dot = current_dot?;
                Self::fragment_apply_binding(dot, path)
            }
            TemplateExpr::Variable(var) if var.is_empty() => Some(FragmentBinding::RootContext),
            TemplateExpr::Variable(var) => locals.get(var).cloned(),
            TemplateExpr::Selector { operand, path } => {
                if let TemplateExpr::Variable(var) = operand.as_ref()
                    && var.is_empty()
                    && let Some((head, tail)) = path.split_first()
                    && let Some(binding) = locals.get(head)
                {
                    return Self::fragment_apply_binding(binding, tail);
                }
                let binding = Self::fragment_binding_from_expr(
                    operand,
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                )?;
                Self::fragment_apply_binding(&binding, path)
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "list" | "tuple") =>
            {
                let mut items = Vec::new();
                for arg in args {
                    items.push(
                        Self::fragment_binding_from_expr(
                            arg,
                            locals,
                            current_dot,
                            defines,
                            define_sources,
                            define_tree_cache,
                            fragment_helper_body_cache,
                            seen,
                        )
                        .unwrap_or(FragmentBinding::Unknown),
                    );
                }
                Some(FragmentBinding::List(items))
            }
            TemplateExpr::Call { function, args } if function == "append" => {
                let mut items = match Self::fragment_binding_from_expr(
                    args.first()?,
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                ) {
                    Some(FragmentBinding::List(items)) => items,
                    Some(binding) => vec![binding],
                    None => Vec::new(),
                };
                for arg in &args[1..] {
                    if let Some(binding) = Self::fragment_binding_from_expr(
                        arg,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    ) {
                        items.push(binding);
                    }
                }
                Some(FragmentBinding::List(items))
            }
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut map = BTreeMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key)) = &args[index] else {
                        index += 1;
                        continue;
                    };
                    if let Some(binding) = Self::fragment_binding_from_expr(
                        &args[index + 1],
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    ) {
                        map.insert(key.clone(), binding);
                    }
                    index += 2;
                }
                Some(FragmentBinding::Dict(map))
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "merge" | "mergeOverwrite") =>
            {
                let mut bindings = Vec::new();
                for arg in args {
                    let Some(binding) = Self::fragment_binding_from_expr(
                        arg,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    ) else {
                        continue;
                    };
                    bindings.push(binding);
                }
                Self::merge_fragment_bindings(bindings)
            }
            TemplateExpr::Call { function, args } if function == "coalesce" => {
                let mut choices = Vec::new();
                for arg in args {
                    if let Some(binding) = Self::fragment_binding_from_expr(
                        arg,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    ) {
                        choices.push(binding);
                    }
                }
                Self::fragment_choice(choices)
            }
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let mut choices = Vec::new();
                if let Some(binding) = Self::fragment_binding_from_expr(
                    &args[1],
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                ) {
                    choices.push(binding);
                }
                if let Some(binding) = Self::fragment_binding_from_expr(
                    &args[0],
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                ) {
                    choices.push(binding);
                }
                Self::fragment_choice(choices)
            }
            TemplateExpr::Call { function, args }
                if matches!(
                    function.as_str(),
                    "toYaml"
                        | "fromYaml"
                        | "quote"
                        | "toString"
                        | "int"
                        | "tpl"
                        | "b64enc"
                        | "b64dec"
                ) =>
            {
                Self::fragment_binding_from_expr(
                    args.first()?,
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                )
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "indent" | "nindent" | "trimAll") =>
            {
                Self::fragment_binding_from_expr(
                    args.last()?,
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                )
            }
            TemplateExpr::Call { function, args } if function == "printf" => {
                let TemplateExpr::Literal(Literal::String(format)) = args.first()? else {
                    return None;
                };
                let mut rendered: BTreeSet<String> = [format.clone()].into_iter().collect();
                for arg in &args[1..] {
                    let strings = Self::fragment_strings(&Self::fragment_binding_from_expr(
                        arg,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    )?);
                    if strings.is_empty() {
                        return None;
                    }
                    let mut next = BTreeSet::new();
                    for current in &rendered {
                        for value in &strings {
                            next.insert(current.replacen("%s", value, 1));
                        }
                    }
                    rendered = next;
                }
                Some(FragmentBinding::StringSet(rendered))
            }
            TemplateExpr::Call { function, args } if function == "index" => {
                let base = Self::fragment_binding_from_expr(
                    args.first()?,
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                )?;
                match base {
                    FragmentBinding::List(items) if args.len() == 2 => {
                        let index = match &args[1] {
                            TemplateExpr::Literal(Literal::Int(value)) => {
                                usize::try_from(*value).ok()?
                            }
                            _ => {
                                let strings =
                                    Self::fragment_strings(&Self::fragment_binding_from_expr(
                                        &args[1],
                                        locals,
                                        current_dot,
                                        defines,
                                        define_sources,
                                        define_tree_cache,
                                        fragment_helper_body_cache,
                                        seen,
                                    )?);
                                strings.iter().next()?.parse::<usize>().ok()?
                            }
                        };
                        items.get(index).cloned()
                    }
                    binding => {
                        let mut segment_options: Vec<Vec<String>> = Vec::new();
                        for arg in &args[1..] {
                            let arg_binding = Self::fragment_binding_from_expr(
                                arg,
                                locals,
                                current_dot,
                                defines,
                                define_sources,
                                define_tree_cache,
                                fragment_helper_body_cache,
                                seen,
                            );
                            let strings = Self::fragment_strings(&arg_binding?);
                            if strings.is_empty() {
                                return None;
                            }
                            segment_options.push(strings.into_iter().collect());
                        }

                        let mut bindings = vec![binding.clone()];
                        for options in segment_options {
                            let mut next = Vec::new();
                            for binding in &bindings {
                                for option in &options {
                                    if let Some(bound) = Self::fragment_apply_binding(
                                        binding,
                                        std::slice::from_ref(option),
                                    ) {
                                        next.push(bound);
                                    }
                                }
                            }
                            bindings = next;
                        }
                        Self::fragment_choice(bindings)
                    }
                }
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "include" | "template") =>
            {
                let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                    return None;
                };
                let current_dot_helper =
                    current_dot.and_then(Self::helper_binding_from_fragment_binding);
                let analysis = Self::analyze_bound_helper_call_with_fragment_locals(
                    name,
                    args.get(1),
                    None,
                    current_dot_helper.as_ref(),
                    locals,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                Self::fragment_binding_from_helper_analysis(analysis)
            }
            TemplateExpr::Call { function, args } if function == "tpl" => {
                Self::fragment_binding_from_expr(
                    args.first()?,
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                )
            }
            TemplateExpr::Call { function, args } if function == "ternary" => {
                let mut choices = Vec::new();
                for arg in args.iter().take(2) {
                    if let Some(binding) = Self::fragment_binding_from_expr(
                        arg,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    ) {
                        choices.push(binding);
                    }
                }
                Self::fragment_choice(choices)
            }
            TemplateExpr::Pipeline(stages) => {
                let mut current = Self::fragment_binding_from_expr(
                    &stages[0],
                    locals,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                for stage in &stages[1..] {
                    let TemplateExpr::Call { function, args } = stage else {
                        continue;
                    };
                    current = match function.as_str() {
                        "quote" | "toString" | "toYaml" | "fromYaml" | "indent" | "nindent"
                        | "trimAll" | "trimPrefix" | "trimSuffix" | "trunc" | "replace" | "int"
                        | "uniq" | "b64enc" | "b64dec" => current,
                        "merge" | "mergeOverwrite" => {
                            let mut bindings = Vec::new();
                            if let Some(current) = current {
                                bindings.push(current);
                            }
                            for arg in args {
                                if let Some(binding) = Self::fragment_binding_from_expr(
                                    arg,
                                    locals,
                                    current_dot,
                                    defines,
                                    define_sources,
                                    define_tree_cache,
                                    fragment_helper_body_cache,
                                    seen,
                                ) {
                                    bindings.push(binding);
                                }
                            }
                            Self::merge_fragment_bindings(bindings)
                        }
                        "default" => {
                            let mut choices = Vec::new();
                            if let Some(current) = current {
                                choices.push(current);
                            }
                            for arg in args {
                                if let Some(binding) = Self::fragment_binding_from_expr(
                                    arg,
                                    locals,
                                    current_dot,
                                    defines,
                                    define_sources,
                                    define_tree_cache,
                                    fragment_helper_body_cache,
                                    seen,
                                ) {
                                    choices.push(binding);
                                }
                            }
                            Self::fragment_choice(choices)
                        }
                        "ternary" => {
                            let mut choices = Vec::new();
                            if let Some(current) = current {
                                choices.push(current);
                            }
                            for arg in args {
                                if let Some(binding) = Self::fragment_binding_from_expr(
                                    arg,
                                    locals,
                                    current_dot,
                                    defines,
                                    define_sources,
                                    define_tree_cache,
                                    fragment_helper_body_cache,
                                    seen,
                                ) {
                                    choices.push(binding);
                                }
                            }
                            Self::fragment_choice(choices)
                        }
                        _ => return None,
                    };
                }
                current
            }
            _ => None,
        }
    }

    fn parse_helper_assignment(text: &str) -> Option<(String, bool, String)> {
        let owned;
        let trimmed = if text.trim_start().starts_with("{{") {
            owned = ResourceDetector::strip_action_wrapping(text)?;
            owned.trim()
        } else {
            text.trim()
        };
        if let Some(index) = trimmed.find(":=") {
            let var = trimmed[..index].trim().strip_prefix('$')?.to_string();
            return Some((var, true, trimmed[index + 2..].trim().to_string()));
        }
        if let Some(index) = trimmed.find(" = ") {
            let var = trimmed[..index].trim().strip_prefix('$')?.to_string();
            return Some((var, false, trimmed[index + 3..].trim().to_string()));
        }
        None
    }

    fn merge_fragment_locals(
        mut base: HashMap<String, FragmentBinding>,
        other: HashMap<String, FragmentBinding>,
    ) -> HashMap<String, FragmentBinding> {
        for (key, value) in other {
            let merged = Self::fragment_union(base.remove(&key), Some(value));
            if let Some(merged) = merged {
                base.insert(key, merged);
            }
        }
        base
    }

    fn shadow_fragment_binding_keys(
        binding: FragmentBinding,
        keys: BTreeSet<String>,
    ) -> FragmentBinding {
        if keys.is_empty() {
            return binding;
        }
        let new_entries: BTreeMap<String, FragmentBinding> = keys
            .into_iter()
            .map(|key| (key, FragmentBinding::Unknown))
            .collect();
        match binding {
            FragmentBinding::Overlay {
                mut entries,
                fallback,
            } => {
                entries.extend(new_entries);
                FragmentBinding::Overlay { entries, fallback }
            }
            other => FragmentBinding::Overlay {
                entries: new_entries,
                fallback: Box::new(other),
            },
        }
    }

    fn local_set_mutation_target_and_keys(
        text: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> Vec<(String, BTreeSet<String>)> {
        let mut out = Vec::new();
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if function != "set" || args.len() < 2 {
                    return;
                }
                let TemplateExpr::Variable(var) = &args[0] else {
                    return;
                };
                if var.is_empty() || !local_bindings.contains_key(var) {
                    return;
                }
                let Some(key_binding) = Self::fragment_binding_from_expr(
                    &args[1],
                    local_bindings,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                ) else {
                    return;
                };
                let keys = Self::fragment_strings(&key_binding);
                if !keys.is_empty() {
                    out.push((var.clone(), keys));
                }
            });
        }
        out
    }

    fn apply_local_set_mutations(
        text: &str,
        local_bindings: &mut HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> bool {
        let mutations = Self::local_set_mutation_target_and_keys(
            text,
            local_bindings,
            current_dot,
            defines,
            define_sources,
            define_tree_cache,
            fragment_helper_body_cache,
            seen,
        );
        let has_mutation = !mutations.is_empty();
        for (var, keys) in mutations {
            if let Some(binding) = local_bindings.remove(&var) {
                local_bindings.insert(var, Self::shadow_fragment_binding_keys(binding, keys));
            }
        }
        has_mutation
    }

    fn range_variable_item_binding(
        header: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> Option<(String, FragmentBinding)> {
        let header = header
            .trim()
            .strip_prefix("range ")
            .unwrap_or_else(|| header.trim());
        let exprs = Self::parse_expr_text(header);
        let [TemplateExpr::VariableDefinition { name, value }] = exprs.as_slice() else {
            return None;
        };
        let binding = Self::fragment_binding_from_range_value_expr(
            value,
            local_bindings,
            current_dot,
            defines,
            define_sources,
            define_tree_cache,
            fragment_helper_body_cache,
            seen,
        )?;
        let item = Self::fragment_item_binding(&binding)?;
        Some((name.trim_start_matches('$').to_string(), item))
    }

    fn range_variable_name(header: &str) -> Option<String> {
        let header = header
            .trim()
            .strip_prefix("range ")
            .unwrap_or_else(|| header.trim());
        let exprs = Self::parse_expr_text(header);
        let [TemplateExpr::VariableDefinition { name, .. }] = exprs.as_slice() else {
            return None;
        };
        Some(name.trim_start_matches('$').to_string())
    }

    fn range_iterable_binding(
        header: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        let header = header
            .trim()
            .strip_prefix("range ")
            .unwrap_or_else(|| header.trim());
        let exprs = Self::parse_expr_text(header);
        let value = match exprs.as_slice() {
            [TemplateExpr::VariableDefinition { value, .. }]
            | [TemplateExpr::Assignment { value, .. }] => value.as_ref(),
            [expr] => expr,
            _ => return None,
        };
        Self::fragment_binding_from_range_value_expr(
            value,
            local_bindings,
            current_dot,
            defines,
            define_sources,
            define_tree_cache,
            fragment_helper_body_cache,
            seen,
        )
    }

    fn fragment_binding_from_range_value_expr(
        value: &TemplateExpr,
        local_bindings: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        Self::fragment_binding_from_expr(
            value,
            local_bindings,
            current_dot,
            defines,
            define_sources,
            define_tree_cache,
            fragment_helper_body_cache,
            seen,
        )
    }

    fn analyze_bound_fragment_helper_body(
        name: &str,
        helper_dot: Option<FragmentBinding>,
        mut locals: HashMap<String, FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        _fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> BTreeSet<String> {
        let token = format!("fragment:{name}");
        if !seen.insert(token.clone()) {
            return BTreeSet::new();
        }
        let mut outputs = BTreeSet::new();
        if let Some(src) = define_sources.get(name)
            && let Some(tree) =
                Self::define_body_tree_from_cache(name, define_sources, define_tree_cache)
        {
            Self::collect_bound_fragment_outputs_from_tree(
                tree.root_node(),
                src,
                &mut locals,
                helper_dot.as_ref(),
                defines,
                define_sources,
                define_tree_cache,
                _fragment_helper_body_cache,
                seen,
                &mut outputs,
            );
        }

        seen.remove(&token);
        outputs
    }

    fn range_header_text_from_source(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
        if let Some(range) = node.child_by_field_name("range") {
            return range
                .utf8_text(source.as_bytes())
                .ok()
                .map(|text| text.trim().to_string());
        }
        let mut walker = node.walk();
        for child in node.named_children(&mut walker) {
            if child.kind() == "range_variable_definition"
                && let Some(range) = child.child_by_field_name("range")
            {
                return range
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(|text| text.trim().to_string());
            }
        }
        None
    }

    fn range_body_emits_sequence_item_from_source(
        node: tree_sitter::Node<'_>,
        source: &str,
    ) -> bool {
        for body_node in Self::children_with_field(node, "body") {
            let Ok(text) = body_node.utf8_text(source.as_bytes()) else {
                continue;
            };
            for line in text.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("- ") || trimmed == "-" {
                    return true;
                }
            }
        }
        false
    }

    fn range_has_destructured_variable_definition(node: tree_sitter::Node<'_>) -> bool {
        let mut walker = node.walk();
        node.named_children(&mut walker)
            .find(|child| child.kind() == "range_variable_definition")
            .is_some_and(|definition| {
                let mut definition_walker = definition.walk();
                definition
                    .named_children(&mut definition_walker)
                    .filter(|child| child.kind() == "variable")
                    .count()
                    >= 2
            })
    }

    fn fragment_binding_from_text(
        text: &str,
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        let mut bindings = Vec::new();
        for expr in Self::parse_expr_text(text) {
            if let Some(binding) = Self::fragment_binding_from_expr(
                &expr,
                locals,
                current_dot,
                defines,
                define_sources,
                define_tree_cache,
                fragment_helper_body_cache,
                seen,
            ) {
                bindings.push(binding);
            }
        }
        Self::fragment_choice(bindings)
    }

    fn collect_bound_fragment_outputs_from_tree(
        node: tree_sitter::Node<'_>,
        source: &str,
        locals: &mut HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
        outputs: &mut BTreeSet<String>,
    ) {
        match node.kind() {
            "variable_definition" | "assignment" => {
                if let Ok(text) = node.utf8_text(source.as_bytes()) {
                    if Self::apply_local_set_mutations(
                        text,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    ) {
                        return;
                    }
                    if let Some((var, _declares, rhs)) = Self::parse_helper_assignment(text) {
                        let binding = Self::fragment_binding_from_text(
                            &rhs,
                            locals,
                            current_dot,
                            defines,
                            define_sources,
                            define_tree_cache,
                            fragment_helper_body_cache,
                            seen,
                        );
                        if let Some(binding) = binding {
                            locals.insert(var, binding);
                        }
                    }
                }
            }
            "template_action"
            | "dot"
            | "variable"
            | "field"
            | "chained_pipeline"
            | "parenthesized_pipeline"
            | "selector_expression"
            | "function_call"
            | "method_call" => {
                if let Ok(text) = node.utf8_text(source.as_bytes())
                    && let Some(binding) = Self::fragment_binding_from_text(
                        text,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    )
                {
                    outputs.extend(Self::fragment_paths(&binding));
                }
            }
            "if_action" => {
                let mut then_locals = locals.clone();
                for child in Self::children_with_field(node, "consequence") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut then_locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                }

                let mut else_locals = locals.clone();
                for child in Self::children_with_field(node, "alternative") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut else_locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                }

                *locals = Self::merge_fragment_locals(then_locals, else_locals);
            }
            "with_action" => {
                let binding = node
                    .child_by_field_name("condition")
                    .and_then(|condition| condition.utf8_text(source.as_bytes()).ok())
                    .and_then(|text| {
                        Self::fragment_binding_from_text(
                            text,
                            locals,
                            current_dot,
                            defines,
                            define_sources,
                            define_tree_cache,
                            fragment_helper_body_cache,
                            seen,
                        )
                    });

                let mut body_locals = locals.clone();
                for child in Self::children_with_field(node, "consequence") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut body_locals,
                        binding.as_ref(),
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                }

                let mut else_locals = locals.clone();
                for child in Self::children_with_field(node, "alternative") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut else_locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                }
            }
            "range_action" => {
                let has_destructured_variable_definition =
                    Self::range_has_destructured_variable_definition(node);
                let header = Self::range_header_text_from_source(node, source);
                let binding = header.as_deref().and_then(|text| {
                    Self::range_iterable_binding(
                        text,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    )
                });
                if has_destructured_variable_definition
                    && !Self::range_body_emits_sequence_item_from_source(node, source)
                    && let Some(binding) = &binding
                {
                    outputs.extend(Self::fragment_paths(binding));
                }

                let body_dot = binding.as_ref().and_then(Self::fragment_item_binding);
                let mut body_locals = locals.clone();
                for child in Self::children_with_field(node, "body") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut body_locals,
                        body_dot.as_ref(),
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                }
                if binding
                    .as_ref()
                    .is_some_and(Self::fragment_binding_definitely_nonempty_iterable)
                {
                    *locals = body_locals;
                } else {
                    *locals = Self::merge_fragment_locals(locals.clone(), body_locals);
                }
            }
            _ => {
                let mut walker = node.walk();
                for child in node.named_children(&mut walker) {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        locals,
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                }
            }
        }
    }

    fn ast_key_segment(node: &HelmAst) -> Option<String> {
        match node {
            HelmAst::Scalar { text } => Some(text.clone()),
            _ => None,
        }
    }

    fn pending_mapping_key_path(node: &HelmAst, relative_path: &YamlPath) -> Option<YamlPath> {
        let segment = match node {
            HelmAst::Mapping { items } => {
                let [HelmAst::Pair { key, value: None }] = items.as_slice() else {
                    return None;
                };
                Self::ast_key_segment(key)?
            }
            HelmAst::Pair { key, value: None } => Self::ast_key_segment(key)?,
            _ => return None,
        };
        let mut path = relative_path.clone();
        path.0.push(segment);
        Some(path)
    }

    fn sequence_item_output_path(relative_path: &YamlPath) -> YamlPath {
        let mut path = relative_path.clone();
        if let Some(last) = path.0.last_mut() {
            if !last.ends_with("[*]") {
                last.push_str("[*]");
            }
        } else {
            path.0.push("[*]".to_string());
        }
        path
    }

    fn trailing_pending_mapping_key_path(
        node: &HelmAst,
        relative_path: &YamlPath,
    ) -> Option<YamlPath> {
        match node {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Define { body: items, .. }
            | HelmAst::Block { body: items, .. } => items
                .last()
                .and_then(|item| Self::trailing_pending_mapping_key_path(item, relative_path)),
            HelmAst::Pair { key, value } => {
                let segment = Self::ast_key_segment(key)?;
                let mut value_path = relative_path.clone();
                value_path.0.push(segment);
                match value {
                    Some(value) => Self::trailing_pending_mapping_key_path(value, &value_path),
                    None => Some(value_path),
                }
            }
            HelmAst::Sequence { items } => {
                let item_path = Self::sequence_item_output_path(relative_path);
                items
                    .last()
                    .and_then(|item| Self::trailing_pending_mapping_key_path(item, &item_path))
            }
            HelmAst::If {
                then_branch,
                else_branch,
                ..
            } => then_branch
                .last()
                .and_then(|item| Self::trailing_pending_mapping_key_path(item, relative_path))
                .or_else(|| {
                    else_branch.last().and_then(|item| {
                        Self::trailing_pending_mapping_key_path(item, relative_path)
                    })
                }),
            HelmAst::With {
                body, else_branch, ..
            }
            | HelmAst::Range {
                body, else_branch, ..
            } => body
                .last()
                .and_then(|item| Self::trailing_pending_mapping_key_path(item, relative_path))
                .or_else(|| {
                    else_branch.last().and_then(|item| {
                        Self::trailing_pending_mapping_key_path(item, relative_path)
                    })
                }),
            HelmAst::Scalar { .. } | HelmAst::HelmExpr { .. } | HelmAst::HelmComment { .. } => None,
        }
    }

    fn helper_output_meta_with_guards(
        mut meta: HelperOutputMeta,
        active_output_guards: &BTreeSet<String>,
    ) -> HelperOutputMeta {
        meta.guards.extend(active_output_guards.iter().cloned());
        meta
    }

    fn helper_fragment_output_path(base: &YamlPath, relative: &YamlPath) -> YamlPath {
        let mut out = base.clone();
        out.0.extend(relative.0.iter().cloned());
        out
    }

    fn expression_output_use_is_keyed_map_projection(
        output: &HelperFragmentOutputUse,
        expression_base: &YamlPath,
    ) -> bool {
        let suffix = if output.relative_path.0.starts_with(&expression_base.0) {
            &output.relative_path.0[expression_base.0.len()..]
        } else {
            output.relative_path.0.as_slice()
        };
        !suffix.is_empty() && suffix.iter().all(|segment| !segment.ends_with("[*]"))
    }

    fn static_yaml_fragment_output_path(text: &str) -> Option<YamlPath> {
        fn printf_format(expr: &TemplateExpr) -> Option<&str> {
            match expr {
                TemplateExpr::Parenthesized(inner) => printf_format(inner),
                TemplateExpr::Call { function, args } if function == "printf" => {
                    let TemplateExpr::Literal(Literal::String(format) | Literal::RawString(format)) =
                        args.first()?
                    else {
                        return None;
                    };
                    Some(format)
                }
                TemplateExpr::Pipeline(stages) => stages.first().and_then(printf_format),
                _ => None,
            }
        }

        let exprs = Self::parse_expr_text(text);
        let [expr] = exprs.as_slice() else {
            return None;
        };
        let format = printf_format(expr)?;
        let key = parse_yaml_key(format.trim_start())?.key;
        Some(YamlPath(vec![key]))
    }

    fn push_helper_fragment_output(
        outputs: &mut Vec<HelperFragmentOutputUse>,
        source_expr: String,
        relative_path: &YamlPath,
        kind: ValueKind,
        meta: HelperOutputMeta,
    ) {
        outputs.push(HelperFragmentOutputUse {
            source_expr,
            relative_path: relative_path.clone(),
            kind,
            meta,
        });
    }

    fn fragment_binding_output_child_kind(binding: &FragmentBinding) -> ValueKind {
        match binding {
            FragmentBinding::Dict(_)
            | FragmentBinding::List(_)
            | FragmentBinding::Overlay { .. } => ValueKind::Fragment,
            FragmentBinding::Choice(choices)
                if choices.iter().any(|choice| {
                    matches!(
                        choice,
                        FragmentBinding::Dict(_)
                            | FragmentBinding::List(_)
                            | FragmentBinding::Overlay { .. }
                    )
                }) =>
            {
                ValueKind::Fragment
            }
            FragmentBinding::ValuesPath(_)
            | FragmentBinding::ValuesRoot
            | FragmentBinding::RootContext
            | FragmentBinding::Unknown
            | FragmentBinding::StringSet(_)
            | FragmentBinding::PathSet(_)
            | FragmentBinding::OutputSet(_)
            | FragmentBinding::Choice(_) => ValueKind::Scalar,
        }
    }

    fn helper_binding_output_child_kind(binding: &HelperBinding) -> ValueKind {
        match binding {
            HelperBinding::Dict(_) | HelperBinding::List(_) | HelperBinding::Overlay { .. } => {
                ValueKind::Fragment
            }
            HelperBinding::Choice(choices)
                if choices.iter().any(|choice| {
                    matches!(
                        choice,
                        HelperBinding::Dict(_)
                            | HelperBinding::List(_)
                            | HelperBinding::Overlay { .. }
                    )
                }) =>
            {
                ValueKind::Fragment
            }
            HelperBinding::ValuesPath(_)
            | HelperBinding::RootContext
            | HelperBinding::Unknown
            | HelperBinding::OutputSet(_)
            | HelperBinding::PathSet(_)
            | HelperBinding::Choice(_) => ValueKind::Scalar,
        }
    }

    fn collect_fragment_binding_output_uses(
        outputs: &mut Vec<HelperFragmentOutputUse>,
        binding: &FragmentBinding,
        relative_path: &YamlPath,
        kind: ValueKind,
        active_output_guards: &BTreeSet<String>,
        defaulted_paths: &BTreeSet<String>,
    ) {
        match binding {
            FragmentBinding::ValuesPath(path) => {
                Self::push_helper_fragment_output(
                    outputs,
                    path.clone(),
                    relative_path,
                    kind,
                    HelperOutputMeta {
                        guards: active_output_guards.clone(),
                        defaulted: defaulted_paths.contains(path),
                    },
                );
            }
            FragmentBinding::PathSet(paths) => {
                for path in paths {
                    Self::push_helper_fragment_output(
                        outputs,
                        path.clone(),
                        relative_path,
                        kind,
                        HelperOutputMeta {
                            guards: active_output_guards.clone(),
                            defaulted: defaulted_paths.contains(path),
                        },
                    );
                }
            }
            FragmentBinding::OutputSet(paths) => {
                for path in paths {
                    Self::push_helper_fragment_output(
                        outputs,
                        path.clone(),
                        relative_path,
                        kind,
                        HelperOutputMeta {
                            guards: active_output_guards.clone(),
                            defaulted: defaulted_paths.contains(path),
                        },
                    );
                }
            }
            FragmentBinding::Dict(entries) => {
                for (key, value) in entries {
                    let child_path = Self::helper_fragment_output_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    Self::collect_fragment_binding_output_uses(
                        outputs,
                        value,
                        &child_path,
                        Self::fragment_binding_output_child_kind(value),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            FragmentBinding::Overlay { entries, fallback } => {
                Self::collect_fragment_binding_output_uses(
                    outputs,
                    fallback,
                    relative_path,
                    kind,
                    active_output_guards,
                    defaulted_paths,
                );
                for (key, value) in entries {
                    let child_path = Self::helper_fragment_output_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    Self::collect_fragment_binding_output_uses(
                        outputs,
                        value,
                        &child_path,
                        Self::fragment_binding_output_child_kind(value),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            FragmentBinding::Choice(choices) => {
                for choice in choices {
                    Self::collect_fragment_binding_output_uses(
                        outputs,
                        choice,
                        relative_path,
                        kind,
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            FragmentBinding::List(items) => {
                let item_path = Self::sequence_item_output_path(relative_path);
                for item in items {
                    Self::collect_fragment_binding_output_uses(
                        outputs,
                        item,
                        &item_path,
                        Self::fragment_binding_output_child_kind(item),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            FragmentBinding::ValuesRoot
            | FragmentBinding::RootContext
            | FragmentBinding::Unknown
            | FragmentBinding::StringSet(_) => {}
        }
    }

    fn collect_helper_binding_output_uses(
        outputs: &mut Vec<HelperFragmentOutputUse>,
        binding: &HelperBinding,
        relative_path: &YamlPath,
        kind: ValueKind,
        active_output_guards: &BTreeSet<String>,
        defaulted_paths: &BTreeSet<String>,
    ) {
        match binding {
            HelperBinding::ValuesPath(path) => {
                Self::push_helper_fragment_output(
                    outputs,
                    path.clone(),
                    relative_path,
                    kind,
                    HelperOutputMeta {
                        guards: active_output_guards.clone(),
                        defaulted: defaulted_paths.contains(path),
                    },
                );
            }
            HelperBinding::PathSet(paths) => {
                for path in paths {
                    Self::push_helper_fragment_output(
                        outputs,
                        path.clone(),
                        relative_path,
                        kind,
                        HelperOutputMeta {
                            guards: active_output_guards.clone(),
                            defaulted: defaulted_paths.contains(path),
                        },
                    );
                }
            }
            HelperBinding::OutputSet(outputs_by_path) => {
                for (path, meta) in outputs_by_path {
                    let meta = Self::helper_output_meta_with_guards(
                        HelperOutputMeta {
                            guards: meta.guards.clone(),
                            defaulted: meta.defaulted || defaulted_paths.contains(path),
                        },
                        active_output_guards,
                    );
                    Self::push_helper_fragment_output(
                        outputs,
                        path.clone(),
                        relative_path,
                        kind,
                        meta,
                    );
                }
            }
            HelperBinding::Dict(entries) => {
                for (key, value) in entries {
                    let child_path = Self::helper_fragment_output_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    Self::collect_helper_binding_output_uses(
                        outputs,
                        value,
                        &child_path,
                        Self::helper_binding_output_child_kind(value),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            HelperBinding::Overlay { entries, fallback } => {
                Self::collect_helper_binding_output_uses(
                    outputs,
                    fallback,
                    relative_path,
                    kind,
                    active_output_guards,
                    defaulted_paths,
                );
                for (key, value) in entries {
                    let child_path = Self::helper_fragment_output_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    Self::collect_helper_binding_output_uses(
                        outputs,
                        value,
                        &child_path,
                        Self::helper_binding_output_child_kind(value),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            HelperBinding::Choice(choices) => {
                for choice in choices {
                    Self::collect_helper_binding_output_uses(
                        outputs,
                        choice,
                        relative_path,
                        kind,
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            HelperBinding::List(items) => {
                let item_path = Self::sequence_item_output_path(relative_path);
                for item in items {
                    Self::collect_helper_binding_output_uses(
                        outputs,
                        item,
                        &item_path,
                        Self::helper_binding_output_child_kind(item),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            HelperBinding::RootContext | HelperBinding::Unknown => {}
        }
    }

    fn collect_helper_binding_output_uses_from_expr(
        expr: &TemplateExpr,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
        relative_path: &YamlPath,
        kind: ValueKind,
        active_output_guards: &BTreeSet<String>,
        defaulted_paths: &BTreeSet<String>,
        outputs: &mut Vec<HelperFragmentOutputUse>,
    ) {
        if Self::expr_contains_helper_call(expr) {
            return;
        }

        if let Some(binding) = Self::binding_from_expr(expr, Some(bindings), current_dot) {
            Self::collect_helper_binding_output_uses(
                outputs,
                &binding,
                relative_path,
                kind,
                active_output_guards,
                defaulted_paths,
            );
            return;
        }

        match expr {
            TemplateExpr::Call { args, .. } => {
                for arg in args {
                    Self::collect_helper_binding_output_uses_from_expr(
                        arg,
                        bindings,
                        current_dot,
                        relative_path,
                        kind,
                        active_output_guards,
                        defaulted_paths,
                        outputs,
                    );
                }
            }
            TemplateExpr::Selector { operand, .. } => {
                Self::collect_helper_binding_output_uses_from_expr(
                    operand,
                    bindings,
                    current_dot,
                    relative_path,
                    kind,
                    active_output_guards,
                    defaulted_paths,
                    outputs,
                );
            }
            TemplateExpr::Pipeline(stages) => {
                for stage in stages {
                    Self::collect_helper_binding_output_uses_from_expr(
                        stage,
                        bindings,
                        current_dot,
                        relative_path,
                        kind,
                        active_output_guards,
                        defaulted_paths,
                        outputs,
                    );
                }
            }
            TemplateExpr::Parenthesized(inner)
            | TemplateExpr::VariableDefinition { value: inner, .. }
            | TemplateExpr::Assignment { value: inner, .. } => {
                Self::collect_helper_binding_output_uses_from_expr(
                    inner,
                    bindings,
                    current_dot,
                    relative_path,
                    kind,
                    active_output_guards,
                    defaulted_paths,
                    outputs,
                );
            }
            TemplateExpr::Literal(_)
            | TemplateExpr::Field(_)
            | TemplateExpr::Variable(_)
            | TemplateExpr::Unknown(_) => {}
        }
    }

    fn collect_bound_fragment_output_uses_from_items(
        items: &[HelmAst],
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
        current_dot_fragment: Option<&FragmentBinding>,
        relative_path: &YamlPath,
        active_output_guards: &BTreeSet<String>,
        local_bindings: &mut HashMap<String, FragmentBinding>,
        local_default_paths: &mut HashMap<String, BTreeSet<String>>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
        outputs: &mut Vec<HelperFragmentOutputUse>,
    ) {
        let mut pending_path: Option<YamlPath> = None;
        for item in items {
            if let Some(path) = Self::pending_mapping_key_path(item, relative_path) {
                pending_path = Some(path);
                continue;
            }
            let item_path = pending_path.as_ref().unwrap_or(relative_path);
            Self::collect_bound_fragment_output_uses_from_ast(
                item,
                bindings,
                current_dot,
                current_dot_fragment,
                item_path,
                active_output_guards,
                local_bindings,
                local_default_paths,
                defines,
                define_sources,
                define_tree_cache,
                fragment_helper_body_cache,
                seen,
                outputs,
            );
            pending_path = Self::trailing_pending_mapping_key_path(item, item_path);
        }
    }

    fn collect_bound_fragment_output_uses_from_ast(
        node: &HelmAst,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
        current_dot_fragment: Option<&FragmentBinding>,
        relative_path: &YamlPath,
        active_output_guards: &BTreeSet<String>,
        local_bindings: &mut HashMap<String, FragmentBinding>,
        local_default_paths: &mut HashMap<String, BTreeSet<String>>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
        outputs: &mut Vec<HelperFragmentOutputUse>,
    ) {
        match node {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Define { body: items, .. }
            | HelmAst::Block { body: items, .. } => {
                Self::collect_bound_fragment_output_uses_from_items(
                    items,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    local_bindings,
                    local_default_paths,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    outputs,
                );
            }
            HelmAst::Sequence { items } => {
                let item_path = Self::sequence_item_output_path(relative_path);
                for item in items {
                    Self::collect_bound_fragment_output_uses_from_ast(
                        item,
                        bindings,
                        current_dot,
                        current_dot_fragment,
                        &item_path,
                        active_output_guards,
                        local_bindings,
                        local_default_paths,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                }
            }
            HelmAst::Pair { key, value } => {
                if let Some(segment) = Self::ast_key_segment(key) {
                    let mut value_path = relative_path.clone();
                    value_path.0.push(segment);
                    if let Some(value) = value {
                        Self::collect_bound_fragment_output_uses_from_ast(
                            value,
                            bindings,
                            current_dot,
                            current_dot_fragment,
                            &value_path,
                            active_output_guards,
                            local_bindings,
                            local_default_paths,
                            defines,
                            define_sources,
                            define_tree_cache,
                            fragment_helper_body_cache,
                            seen,
                            outputs,
                        );
                    }
                    return;
                }

                Self::collect_bound_fragment_output_uses_from_ast(
                    key,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    local_bindings,
                    local_default_paths,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    outputs,
                );
                if let Some(value) = value {
                    Self::collect_bound_fragment_output_uses_from_ast(
                        value,
                        bindings,
                        current_dot,
                        current_dot_fragment,
                        relative_path,
                        active_output_guards,
                        local_bindings,
                        local_default_paths,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                }
            }
            HelmAst::HelmExpr { text } => {
                let mut seen_set = HashSet::new();
                if Self::apply_local_set_mutations(
                    text,
                    local_bindings,
                    current_dot_fragment,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    &mut seen_set,
                ) {
                    return;
                }

                if let Some((var, _declares, rhs)) = Self::parse_helper_assignment(text) {
                    let mut seen_rhs = HashSet::new();
                    let mut binding = Self::fragment_binding_from_text(
                        &rhs,
                        local_bindings,
                        current_dot_fragment,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        &mut seen_rhs,
                    );
                    let mut top_level_helper_dependency_paths = BTreeSet::new();
                    if Self::text_starts_with_helper_call(&rhs) {
                        let mut rhs_seen = seen.clone();
                        let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                            &rhs,
                            Some(bindings),
                            current_dot,
                            local_bindings,
                            defines,
                            define_sources,
                            define_tree_cache,
                            fragment_helper_body_cache,
                            &mut rhs_seen,
                        );
                        top_level_helper_dependency_paths = bound_helper_dependency_paths(&nested);
                        if let Some(nested_binding) =
                            Self::fragment_binding_from_helper_analysis(nested)
                        {
                            binding = match binding {
                                Some(binding) => {
                                    Self::merge_fragment_bindings(vec![binding, nested_binding])
                                }
                                None => Some(nested_binding),
                            };
                        }
                    }
                    if Self::text_pipeline_merges_into_var(&rhs, &var)
                        && let Some(current_dot_fragment) = current_dot_fragment
                        && matches!(
                            current_dot_fragment,
                            FragmentBinding::Dict(_) | FragmentBinding::Overlay { .. }
                        )
                    {
                        let current_item_paths = Self::fragment_paths(current_dot_fragment);
                        let mut internal_item_paths = current_item_paths;
                        internal_item_paths.extend(top_level_helper_dependency_paths);
                        if !internal_item_paths.is_empty() {
                            binding = binding.and_then(|binding| {
                                Self::remove_fragment_paths(binding, &internal_item_paths)
                            });
                        }
                        binding = match binding {
                            Some(binding) => Self::merge_fragment_bindings(vec![
                                binding,
                                current_dot_fragment.clone(),
                            ]),
                            None => Some(current_dot_fragment.clone()),
                        };
                    }
                    if let Some(binding) = binding {
                        local_bindings.insert(var.clone(), binding);
                    }
                    let mut defaulted_paths = Self::resolved_default_fallback_paths_for_text(
                        &rhs,
                        Some(bindings),
                        current_dot,
                    );
                    defaulted_paths.extend(Self::local_default_paths_from_text(
                        &rhs,
                        local_default_paths,
                    ));
                    if defaulted_paths.is_empty() {
                        local_default_paths.remove(&var);
                    } else {
                        local_default_paths.insert(var, defaulted_paths);
                    }
                    return;
                }

                let kind = if is_fragment_expr(text) {
                    ValueKind::Fragment
                } else {
                    ValueKind::Scalar
                };
                let output_path = Self::static_yaml_fragment_output_path(text)
                    .map(|output_path| {
                        Self::helper_fragment_output_path(relative_path, &output_path)
                    })
                    .unwrap_or_else(|| relative_path.clone());
                let direct_outputs =
                    Self::direct_bound_paths_from_text_in_context(text, bindings, current_dot);
                let fallback_paths = Self::resolved_default_fallback_paths_for_text(
                    text,
                    Some(bindings),
                    current_dot,
                );
                let local_outputs = Self::local_bound_paths_from_text(text, local_bindings);
                let handled_outputs: BTreeSet<String> = direct_outputs
                    .iter()
                    .chain(local_outputs.iter())
                    .cloned()
                    .collect();
                let mut direct_output_uses = Vec::new();
                for expr in Self::parse_expr_text(text) {
                    Self::collect_helper_binding_output_uses_from_expr(
                        &expr,
                        bindings,
                        current_dot,
                        &output_path,
                        kind,
                        active_output_guards,
                        &fallback_paths,
                        &mut direct_output_uses,
                    );
                }
                outputs.extend(direct_output_uses);

                let local_fallback_paths =
                    Self::local_default_paths_from_text(text, local_default_paths);
                let mut local_output_uses = Vec::new();
                for expr in Self::parse_expr_text(text) {
                    Self::walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                        let binding = match node {
                            TemplateExpr::Variable(var) if !var.is_empty() => {
                                local_bindings.get(var).cloned()
                            }
                            TemplateExpr::Selector { operand, path } => {
                                let TemplateExpr::Variable(var) = operand.as_ref() else {
                                    return;
                                };
                                if var.is_empty() {
                                    return;
                                }
                                local_bindings
                                    .get(var)
                                    .and_then(|binding| Self::fragment_apply_binding(binding, path))
                            }
                            _ => None,
                        };
                        if let Some(binding) = binding {
                            Self::collect_fragment_binding_output_uses(
                                &mut local_output_uses,
                                &binding,
                                &output_path,
                                kind,
                                active_output_guards,
                                &local_fallback_paths,
                            );
                        }
                    });
                }
                let mut nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    text,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                let nested_structured_sources: BTreeSet<String> = nested
                    .fragment_output_uses
                    .iter()
                    .map(|output| output.source_expr.clone())
                    .collect();
                let empty_output_path = YamlPath(Vec::new());
                let nested_descendant_structured_sources: BTreeSet<String> = nested
                    .fragment_output_uses
                    .iter()
                    .filter(|output| {
                        Self::expression_output_use_is_keyed_map_projection(
                            output,
                            &empty_output_path,
                        )
                    })
                    .map(|output| output.source_expr.clone())
                    .collect();
                let nested_scalar_sources: BTreeSet<String> =
                    nested.output.keys().cloned().collect();
                let nested_has_fragment_outputs =
                    !nested.fragment_output.is_empty() || !nested.fragment_output_uses.is_empty();

                let mut expression_output_uses = Vec::new();
                let mut expression_seen = seen.clone();
                for expr in Self::parse_expr_text(text) {
                    if !Self::expr_contains_helper_call(&expr) {
                        continue;
                    }
                    if let Some(binding) = Self::helper_binding_from_expr_with_fragment_locals(
                        &expr,
                        local_bindings,
                        Some(bindings),
                        current_dot,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        &mut expression_seen,
                    ) {
                        Self::collect_helper_binding_output_uses(
                            &mut expression_output_uses,
                            &binding,
                            &output_path,
                            kind,
                            active_output_guards,
                            &fallback_paths,
                        );
                    }
                }
                expression_output_uses.retain(|output| {
                    Self::expression_output_use_is_keyed_map_projection(output, &output_path)
                });
                let expression_descendant_sources: BTreeSet<String> = expression_output_uses
                    .iter()
                    .filter(|output| !output.relative_path.0.is_empty())
                    .map(|output| output.source_expr.clone())
                    .collect();

                outputs.extend(local_output_uses);
                for output in expression_output_uses {
                    if output.relative_path.0.is_empty()
                        && (handled_outputs.contains(&output.source_expr)
                            || nested_structured_sources.contains(&output.source_expr)
                            || nested_scalar_sources.contains(&output.source_expr))
                    {
                        continue;
                    }
                    outputs.push(output);
                }
                for (source_expr, meta) in nested.output {
                    if kind == ValueKind::Fragment && nested_has_fragment_outputs {
                        continue;
                    }
                    if nested_structured_sources.contains(&source_expr)
                        || expression_descendant_sources.contains(&source_expr)
                    {
                        continue;
                    }
                    let meta = Self::helper_output_meta_with_guards(meta, active_output_guards);
                    Self::push_helper_fragment_output(
                        outputs,
                        source_expr,
                        relative_path,
                        kind,
                        meta,
                    );
                }
                for nested_output in nested.fragment_output_uses.drain(..) {
                    if kind == ValueKind::Fragment
                        && nested_output.relative_path.0.is_empty()
                        && (nested_scalar_sources.contains(&nested_output.source_expr)
                            || nested_descendant_structured_sources
                                .contains(&nested_output.source_expr)
                            || expression_descendant_sources.contains(&nested_output.source_expr))
                    {
                        continue;
                    }
                    let meta = Self::helper_output_meta_with_guards(
                        nested_output.meta,
                        active_output_guards,
                    );
                    Self::push_helper_fragment_output(
                        outputs,
                        nested_output.source_expr,
                        &Self::helper_fragment_output_path(
                            relative_path,
                            &nested_output.relative_path,
                        ),
                        nested_output.kind,
                        meta,
                    );
                }
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    Self::direct_bound_paths_from_text_in_context(cond, bindings, current_dot);
                branch_guard_paths.extend(Self::local_bound_paths_from_text(cond, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    cond,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_dependency_paths(&nested));

                let mut then_guards = active_output_guards.clone();
                then_guards.extend(branch_guard_paths);
                let mut then_bindings = local_bindings.clone();
                let mut then_defaults = local_default_paths.clone();
                Self::collect_bound_fragment_output_uses_from_items(
                    then_branch,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    &then_guards,
                    &mut then_bindings,
                    &mut then_defaults,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    outputs,
                );

                let mut else_bindings = local_bindings.clone();
                let mut else_defaults = local_default_paths.clone();
                Self::collect_bound_fragment_output_uses_from_items(
                    else_branch,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    &mut else_bindings,
                    &mut else_defaults,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    outputs,
                );
                *local_bindings = Self::merge_fragment_locals(then_bindings, else_bindings);
                *local_default_paths = merge_local_default_paths(then_defaults, else_defaults);
            }
            HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    Self::direct_bound_paths_from_text_in_context(header, bindings, current_dot);
                branch_guard_paths
                    .extend(Self::local_bound_paths_from_text(header, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    header,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_dependency_paths(&nested));
                let body_dot = Self::computed_with_body_dot(header, bindings, current_dot);

                let mut body_guards = active_output_guards.clone();
                body_guards.extend(branch_guard_paths);
                let mut body_bindings = local_bindings.clone();
                let mut body_defaults = local_default_paths.clone();
                let body_dot_fragment = body_dot
                    .as_ref()
                    .map(Self::fragment_binding_from_helper_binding);
                Self::collect_bound_fragment_output_uses_from_items(
                    body,
                    bindings,
                    body_dot.as_ref(),
                    body_dot_fragment.as_ref(),
                    relative_path,
                    &body_guards,
                    &mut body_bindings,
                    &mut body_defaults,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    outputs,
                );

                let mut else_bindings = local_bindings.clone();
                let mut else_defaults = local_default_paths.clone();
                Self::collect_bound_fragment_output_uses_from_items(
                    else_branch,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    &mut else_bindings,
                    &mut else_defaults,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    outputs,
                );
                *local_bindings = Self::merge_fragment_locals(body_bindings, else_bindings);
                *local_default_paths = merge_local_default_paths(body_defaults, else_defaults);
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    Self::direct_bound_paths_from_text_in_context(header, bindings, current_dot);
                branch_guard_paths
                    .extend(Self::local_bound_paths_from_text(header, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    header,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_dependency_paths(&nested));
                let mut seen_range_binding = HashSet::new();
                let range_binding = Self::range_iterable_binding(
                    header,
                    local_bindings,
                    current_dot_fragment,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    &mut seen_range_binding,
                );
                let body_dot_fragment =
                    range_binding.as_ref().and_then(Self::fragment_item_binding);
                let body_dot = body_dot_fragment
                    .as_ref()
                    .and_then(Self::helper_binding_from_fragment_binding);

                let mut body_guards = active_output_guards.clone();
                body_guards.extend(branch_guard_paths);
                let mut body_bindings = local_bindings.clone();
                let mut body_defaults = local_default_paths.clone();
                if let Some(FragmentBinding::List(items)) = &range_binding {
                    let range_var = Self::range_variable_name(header);
                    for item_binding in items {
                        if let Some(range_var) = &range_var {
                            body_bindings.insert(range_var.clone(), item_binding.clone());
                        }
                        let item_dot = Self::helper_binding_from_fragment_binding(item_binding);
                        let mut item_seen = seen.clone();
                        Self::collect_bound_fragment_output_uses_from_items(
                            body,
                            bindings,
                            item_dot.as_ref(),
                            Some(item_binding),
                            relative_path,
                            &body_guards,
                            &mut body_bindings,
                            &mut body_defaults,
                            defines,
                            define_sources,
                            define_tree_cache,
                            fragment_helper_body_cache,
                            &mut item_seen,
                            outputs,
                        );
                    }
                } else {
                    Self::collect_bound_fragment_output_uses_from_items(
                        body,
                        bindings,
                        body_dot.as_ref(),
                        body_dot_fragment.as_ref(),
                        relative_path,
                        &body_guards,
                        &mut body_bindings,
                        &mut body_defaults,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                }

                if range_binding
                    .as_ref()
                    .is_some_and(Self::fragment_binding_definitely_nonempty_iterable)
                {
                    *local_bindings = body_bindings;
                    *local_default_paths = body_defaults;
                } else {
                    let mut else_bindings = local_bindings.clone();
                    let mut else_defaults = local_default_paths.clone();
                    Self::collect_bound_fragment_output_uses_from_items(
                        else_branch,
                        bindings,
                        current_dot,
                        current_dot_fragment,
                        relative_path,
                        active_output_guards,
                        &mut else_bindings,
                        &mut else_defaults,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        outputs,
                    );
                    *local_bindings = Self::merge_fragment_locals(body_bindings, else_bindings);
                    *local_default_paths = merge_local_default_paths(body_defaults, else_defaults);
                }
            }
            HelmAst::Scalar { .. } | HelmAst::HelmComment { .. } => {}
        }
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn analyze_bound_helper_calls(&self, text: &str) -> BoundHelperAnalysis {
        let current_dot = self.current_dot_binding();
        let key = BoundHelperCallsCacheKey {
            text: text.to_string(),
            current_dot: current_dot.clone(),
        };
        if let Some(cached) = self
            .ir_context
            .inner
            .bound_helper_calls_cache
            .borrow()
            .get(&key)
        {
            return cached.clone();
        }

        let mut seen = HashSet::new();
        let analysis = Self::analyze_bound_helper_calls_with_bindings(
            text,
            None,
            current_dot.as_ref(),
            self.defines,
            &self.ir_context.inner.define_body_sources,
            &self.ir_context.inner.define_tree_cache,
            &self.fragment_helper_body_cache,
            &mut seen,
        );
        self.ir_context
            .inner
            .bound_helper_calls_cache
            .borrow_mut()
            .insert(key, analysis.clone());
        analysis
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn analyze_bound_fragment_helper_calls_in_context(&self, text: &str) -> BTreeSet<String> {
        let mut locals = self.template_bindings.clone();
        for (key, value) in &self.root_bindings {
            locals.insert(
                key.clone(),
                Self::fragment_binding_from_helper_binding(value),
            );
        }

        let mut seen = HashSet::new();
        let mut outputs = BTreeSet::new();
        let current_dot = self.current_dot_binding();
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

                let helper_dot = args.get(1).and_then(|arg| {
                    Self::fragment_binding_from_outer_expr(
                        arg,
                        Some(&locals),
                        Some(&self.root_bindings),
                        current_dot.as_ref(),
                    )
                });
                let mut helper_locals = HashMap::new();
                if let Some(FragmentBinding::Dict(map)) = &helper_dot {
                    for (key, value) in map {
                        helper_locals.insert(key.clone(), value.clone());
                    }
                }

                let nested = Self::analyze_bound_fragment_helper_body(
                    name,
                    helper_dot,
                    helper_locals,
                    self.defines,
                    &self.ir_context.inner.define_body_sources,
                    &self.ir_context.inner.define_tree_cache,
                    &self.fragment_helper_body_cache,
                    &mut seen,
                );
                outputs.extend(nested);
            });
        }
        outputs
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn analyze_bound_helper_calls_with_bindings(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        let fragment_locals = HashMap::new();
        Self::analyze_bound_helper_calls_with_fragment_locals(
            text,
            bindings,
            current_dot,
            &fragment_locals,
            defines,
            define_sources,
            define_tree_cache,
            fragment_helper_body_cache,
            seen,
        )
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn analyze_bound_helper_calls_with_fragment_locals(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        let mut analysis = BoundHelperAnalysis::default();
        for expr in Self::parse_expr_text(text) {
            Self::walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if !matches!(function.as_str(), "include" | "template") {
                    return;
                }
                let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                    return;
                };
                let nested = Self::analyze_bound_helper_call_with_fragment_locals(
                    name,
                    args.get(1),
                    bindings,
                    current_dot,
                    fragment_locals,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                analysis.extend(nested);
            });
        }
        analysis
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn analyze_bound_helper_call(
        name: &str,
        arg: Option<&TemplateExpr>,
        outer_bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        let fragment_locals = HashMap::new();
        Self::analyze_bound_helper_call_with_fragment_locals(
            name,
            arg,
            outer_bindings,
            current_dot,
            &fragment_locals,
            defines,
            define_sources,
            define_tree_cache,
            fragment_helper_body_cache,
            seen,
        )
    }

    #[tracing::instrument(skip_all)]
    fn analyze_bound_helper_call_with_fragment_locals(
        name: &str,
        arg: Option<&TemplateExpr>,
        outer_bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        if !seen.insert(name.to_string()) {
            return BoundHelperAnalysis::default();
        }

        let mut binding_seen = seen.clone();
        let bindings = Self::bindings_for_helper_arg_with_fragment_locals(
            arg,
            outer_bindings,
            current_dot,
            fragment_locals,
            defines,
            define_sources,
            define_tree_cache,
            fragment_helper_body_cache,
            &mut binding_seen,
        );
        // Inside the helper body, `.` is what the caller passed as the
        // helper argument. `binding_from_expr` against the outer
        // `current_dot` resolves that for us (and falls back to
        // `RootContext` when the caller passes the bare `.` from a
        // template-root context). `None` is acceptable when the
        // caller's dot can't be statically pinned.
        let helper_body_dot = {
            let mut dot_seen = seen.clone();
            arg.and_then(|expr| {
                Self::helper_binding_from_expr_with_fragment_locals(
                    expr,
                    fragment_locals,
                    outer_bindings,
                    current_dot,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    &mut dot_seen,
                )
            })
            .or_else(|| current_dot.cloned())
        };
        let mut analysis = BoundHelperAnalysis::default();
        if let Some(body) = defines.get(name) {
            let active_output_guards = BTreeSet::new();
            let mut local_bindings = HashMap::new();
            let mut local_default_paths = HashMap::new();
            for node in body {
                Self::collect_bound_helper_values_from_ast(
                    node,
                    &bindings,
                    helper_body_dot.as_ref(),
                    &active_output_guards,
                    &mut local_bindings,
                    &mut local_default_paths,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    &mut analysis,
                );
            }
        }
        let mut helper_fragment_locals = HashMap::new();
        let helper_dot = arg.and_then(|expr| {
            Self::fragment_binding_from_outer_expr(
                expr,
                Some(fragment_locals),
                outer_bindings,
                current_dot,
            )
        });
        if let Some(src) = define_sources.get(name)
            && let Some(tree) =
                Self::define_body_tree_from_cache(name, define_sources, define_tree_cache)
        {
            Self::collect_bound_fragment_outputs_from_tree(
                tree.root_node(),
                src,
                &mut helper_fragment_locals,
                helper_dot.as_ref(),
                defines,
                define_sources,
                define_tree_cache,
                fragment_helper_body_cache,
                seen,
                &mut analysis.fragment_output,
            );
        }
        if let Some(body) = defines.get(name) {
            let mut fragment_output_uses = Vec::new();
            let mut local_bindings = helper_fragment_locals;
            let mut local_default_paths = HashMap::new();
            let active_output_guards = BTreeSet::new();
            Self::collect_bound_fragment_output_uses_from_items(
                body,
                &bindings,
                helper_body_dot.as_ref(),
                helper_dot.as_ref(),
                &YamlPath(Vec::new()),
                &active_output_guards,
                &mut local_bindings,
                &mut local_default_paths,
                defines,
                define_sources,
                define_tree_cache,
                fragment_helper_body_cache,
                seen,
                &mut fragment_output_uses,
            );
            for source in analysis.output.keys() {
                analysis.fragment_output.remove(source);
            }
            let structured_sources: BTreeSet<String> = fragment_output_uses
                .iter()
                .map(|output| output.source_expr.clone())
                .collect();
            for source in &structured_sources {
                analysis.output.remove(source);
                analysis.fragment_output.remove(source);
            }
            analysis.fragment_output_uses.extend(fragment_output_uses);
        }

        for binding in bindings.values() {
            let HelperBinding::ValuesPath(root) = binding else {
                continue;
            };
            let prefix = format!("{root}.");
            if analysis
                .output
                .keys()
                .chain(analysis.guard_paths.iter())
                .any(|path| path.starts_with(&prefix))
            {
                analysis.suppress_roots.insert(root.clone());
            }
        }

        seen.remove(name);
        analysis
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn direct_bound_paths_from_text(
        text: &str,
        bindings: &HashMap<String, HelperBinding>,
    ) -> BTreeSet<String> {
        Self::direct_bound_paths_from_text_in_context(text, bindings, None)
    }

    fn direct_bound_paths_from_text_in_context(
        text: &str,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            Self::walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                if Self::expr_contains_helper_call(node) {
                    return;
                }
                if let Some(binding) = Self::binding_from_expr(node, Some(bindings), current_dot) {
                    out.extend(Self::helper_binding_paths(&binding));
                }
            });
        }
        out
    }

    fn local_bound_paths_from_text(
        text: &str,
        locals: &HashMap<String, FragmentBinding>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            Self::walk_expr_excluding_helper_call_args(&expr, &mut |node| match node {
                TemplateExpr::Variable(var) if !var.is_empty() => {
                    if let Some(binding) = locals.get(var) {
                        out.extend(Self::fragment_paths(binding));
                    }
                }
                TemplateExpr::Selector { operand, path } => {
                    let TemplateExpr::Variable(var) = operand.as_ref() else {
                        return;
                    };
                    if var.is_empty() {
                        return;
                    }
                    if let Some(binding) = locals.get(var)
                        && let Some(bound) = Self::fragment_apply_binding(binding, path)
                    {
                        out.extend(Self::fragment_paths(&bound));
                    }
                }
                _ => {}
            });
        }
        out
    }

    fn local_default_paths_from_text(
        text: &str,
        local_default_paths: &HashMap<String, BTreeSet<String>>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                let TemplateExpr::Variable(var) = node else {
                    return;
                };
                if var.is_empty() {
                    return;
                }
                if let Some(paths) = local_default_paths.get(var) {
                    out.extend(paths.iter().cloned());
                }
            });
        }
        out
    }

    /// Walk a helper's AST node collecting bound helper values.
    ///
    /// `current_dot` tracks the value the current `.` resolves to as
    /// the walk descends into `with`/`range` blocks. Without it, a
    /// helper body like
    ///
    /// ```text
    /// {{- define "X.defaults" }}
    ///   {{- with .Values }}
    ///     {{- $_ := set .a "name" (.a.name | default "fallback") }}
    ///   {{- end }}
    /// {{- end }}
    /// ```
    ///
    /// would not register `a.name` as a chart-declared default — the
    /// default-fallback detector inside the inner `set` action only
    /// sees `.a.name` (no `.Values` prefix), so it can't resolve to a
    /// values path. Threading the with-shifted dot lets the detector
    /// re-root `.a.name` against `.Values` and record the default.
    #[tracing::instrument(skip_all)]
    fn collect_bound_helper_values_from_ast(
        node: &HelmAst,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
        active_output_guards: &BTreeSet<String>,
        local_bindings: &mut HashMap<String, FragmentBinding>,
        local_default_paths: &mut HashMap<String, BTreeSet<String>>,
        defines: &DefineIndex,
        define_sources: &HashMap<String, String>,
        define_tree_cache: &RefCell<HashMap<String, tree_sitter::Tree>>,
        fragment_helper_body_cache: &RefCell<
            BTreeMap<FragmentHelperBodyCacheKey, BTreeSet<String>>,
        >,
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
                        item,
                        bindings,
                        current_dot,
                        active_output_guards,
                        local_bindings,
                        local_default_paths,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        analysis,
                    );
                }
            }
            HelmAst::Pair { key, value } => {
                Self::collect_bound_helper_values_from_ast(
                    key,
                    bindings,
                    current_dot,
                    active_output_guards,
                    local_bindings,
                    local_default_paths,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                    analysis,
                );
                if let Some(value) = value {
                    Self::collect_bound_helper_values_from_ast(
                        value,
                        bindings,
                        current_dot,
                        active_output_guards,
                        local_bindings,
                        local_default_paths,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        analysis,
                    );
                }
            }
            HelmAst::HelmExpr { text } => {
                if let Some((var, _declares, rhs)) = Self::parse_helper_assignment(text) {
                    let set_default_paths =
                        Self::set_default_chart_paths_for_text(text, Some(bindings), current_dot);
                    analysis.chart_defaults.extend(set_default_paths);
                    extend_type_hints(
                        &mut analysis.type_hints,
                        Self::resolved_type_is_paths_for_text(&rhs, Some(bindings), current_dot),
                    );

                    let current_dot_fragment =
                        current_dot.map(Self::fragment_binding_from_helper_binding);
                    let mut seen_set = HashSet::new();
                    if Self::apply_local_set_mutations(
                        text,
                        local_bindings,
                        current_dot_fragment.as_ref(),
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        &mut seen_set,
                    ) {
                        return;
                    }

                    let fallback_paths = Self::resolved_default_fallback_paths_for_text(
                        &rhs,
                        Some(bindings),
                        current_dot,
                    );
                    let direct_outputs =
                        Self::direct_bound_paths_from_text_in_context(&rhs, bindings, current_dot);
                    let local_fallback_paths =
                        Self::local_default_paths_from_text(&rhs, local_default_paths);
                    let local_outputs = Self::local_bound_paths_from_text(&rhs, local_bindings);
                    for output in &direct_outputs {
                        analysis.add_output(
                            output.clone(),
                            active_output_guards,
                            fallback_paths.contains(output),
                        );
                    }
                    for output in &local_outputs {
                        analysis.add_output(
                            output.clone(),
                            active_output_guards,
                            local_fallback_paths.contains(output),
                        );
                    }
                    let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                        &rhs,
                        Some(bindings),
                        current_dot,
                        local_bindings,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                    );
                    analysis
                        .chart_defaults
                        .extend(nested.chart_defaults.clone());
                    extend_type_hints(&mut analysis.type_hints, nested.type_hints.clone());

                    let mut seen_rhs = HashSet::new();
                    if let Some(binding) = Self::fragment_binding_from_text(
                        &rhs,
                        local_bindings,
                        current_dot_fragment.as_ref(),
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        &mut seen_rhs,
                    ) {
                        local_bindings.insert(var.clone(), binding);
                    }
                    let mut defaulted_paths = fallback_paths;
                    defaulted_paths.extend(local_fallback_paths);
                    defaulted_paths.extend(
                        nested
                            .output
                            .iter()
                            .filter_map(|(path, meta)| meta.defaulted.then(|| path.clone())),
                    );
                    defaulted_paths.extend(nested.fragment_output_uses.iter().filter_map(
                        |output| output.meta.defaulted.then(|| output.source_expr.clone()),
                    ));
                    let mut nested = nested;
                    convert_fragment_outputs_to_dependency_outputs(&mut nested);
                    for meta in nested.output.values_mut() {
                        meta.guards.extend(active_output_guards.iter().cloned());
                    }
                    analysis.extend(nested);
                    if defaulted_paths.is_empty() {
                        local_default_paths.remove(&var);
                    } else {
                        local_default_paths.insert(var, defaulted_paths);
                    }
                    return;
                }

                let current_dot_fragment =
                    current_dot.map(Self::fragment_binding_from_helper_binding);
                let mut seen_set = HashSet::new();
                if Self::apply_local_set_mutations(
                    text,
                    local_bindings,
                    current_dot_fragment.as_ref(),
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    &mut seen_set,
                ) {
                    let set_default_paths =
                        Self::set_default_chart_paths_for_text(text, Some(bindings), current_dot);
                    analysis.chart_defaults.extend(set_default_paths);
                    return;
                }

                let direct_outputs =
                    Self::direct_bound_paths_from_text_in_context(text, bindings, current_dot);
                let fallback_paths = Self::resolved_default_fallback_paths_for_text(
                    text,
                    Some(bindings),
                    current_dot,
                );
                extend_type_hints(
                    &mut analysis.type_hints,
                    Self::resolved_type_is_paths_for_text(text, Some(bindings), current_dot),
                );
                let local_outputs = Self::local_bound_paths_from_text(text, local_bindings);
                let local_fallback_paths =
                    Self::local_default_paths_from_text(text, local_default_paths);
                for output in direct_outputs {
                    analysis.add_output(
                        output.clone(),
                        active_output_guards,
                        fallback_paths.contains(&output),
                    );
                }
                for output in local_outputs {
                    analysis.add_output(
                        output.clone(),
                        active_output_guards,
                        local_fallback_paths.contains(&output),
                    );
                }
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    text,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                let mut nested = nested;
                convert_fragment_outputs_to_dependency_outputs(&mut nested);
                for meta in nested.output.values_mut() {
                    meta.guards.extend(active_output_guards.iter().cloned());
                }
                analysis.extend(nested);
                let set_default_paths =
                    Self::set_default_chart_paths_for_text(text, Some(bindings), current_dot);
                analysis.chart_defaults.extend(set_default_paths);
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    Self::direct_bound_paths_from_text_in_context(cond, bindings, current_dot);
                branch_guard_paths.extend(Self::local_bound_paths_from_text(cond, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    cond,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_dependency_paths(&nested));
                analysis
                    .guard_paths
                    .extend(branch_guard_paths.iter().cloned());
                let mut then_output_guards = active_output_guards.clone();
                then_output_guards.extend(branch_guard_paths);
                let mut then_bindings = local_bindings.clone();
                let mut then_default_paths = local_default_paths.clone();
                for item in then_branch {
                    Self::collect_bound_helper_values_from_ast(
                        item,
                        bindings,
                        current_dot,
                        &then_output_guards,
                        &mut then_bindings,
                        &mut then_default_paths,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        analysis,
                    );
                }
                let mut else_bindings = local_bindings.clone();
                let mut else_default_paths = local_default_paths.clone();
                for item in else_branch {
                    Self::collect_bound_helper_values_from_ast(
                        item,
                        bindings,
                        current_dot,
                        active_output_guards,
                        &mut else_bindings,
                        &mut else_default_paths,
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        seen,
                        analysis,
                    );
                }
                *local_bindings = Self::merge_fragment_locals(then_bindings, else_bindings);
                *local_default_paths =
                    merge_local_default_paths(then_default_paths, else_default_paths);
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
                let is_with = matches!(node, HelmAst::With { .. });
                let mut branch_guard_paths =
                    Self::direct_bound_paths_from_text_in_context(header, bindings, current_dot);
                branch_guard_paths
                    .extend(Self::local_bound_paths_from_text(header, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    header,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    defines,
                    define_sources,
                    define_tree_cache,
                    fragment_helper_body_cache,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_dependency_paths(&nested));
                analysis
                    .guard_paths
                    .extend(branch_guard_paths.iter().cloned());

                let mut range_fragment_binding = None;
                let mut range_binding = None;
                if !is_with {
                    let current_dot_fragment =
                        current_dot.map(Self::fragment_binding_from_helper_binding);
                    let mut seen_range = HashSet::new();
                    range_fragment_binding = Self::range_iterable_binding(
                        header,
                        local_bindings,
                        current_dot_fragment.as_ref(),
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        &mut seen_range,
                    );
                    range_binding = range_fragment_binding
                        .as_ref()
                        .and_then(Self::helper_binding_from_fragment_binding);
                }
                // Inside a `with` body the dot re-roots to the value of
                // the with-header. Inside a `range` body the dot re-roots
                // to the per-iteration item, even when the template does
                // not bind the item to a variable.
                let body_dot = if is_with {
                    Self::computed_with_body_dot(header, bindings, current_dot)
                } else {
                    range_binding.as_ref().and_then(Self::helper_item_binding)
                };
                let mut body_output_guards = active_output_guards.clone();
                body_output_guards.extend(branch_guard_paths);
                let mut body_bindings = local_bindings.clone();
                let mut body_default_paths = local_default_paths.clone();
                if !is_with {
                    let header_dot_fragment =
                        current_dot.map(Self::fragment_binding_from_helper_binding);
                    let mut seen_range = HashSet::new();
                    if let Some((var, binding)) = Self::range_variable_item_binding(
                        header,
                        &body_bindings,
                        header_dot_fragment.as_ref(),
                        defines,
                        define_sources,
                        define_tree_cache,
                        fragment_helper_body_cache,
                        &mut seen_range,
                    ) {
                        body_bindings.insert(var, binding);
                    }
                }
                if !is_with && let Some(FragmentBinding::List(items)) = &range_fragment_binding {
                    let range_var = Self::range_variable_name(header);
                    for item_binding in items {
                        if let Some(range_var) = &range_var {
                            body_bindings.insert(range_var.clone(), item_binding.clone());
                        }
                        let item_dot = Self::helper_binding_from_fragment_binding(item_binding);
                        let mut item_seen = seen.clone();
                        for item in body {
                            Self::collect_bound_helper_values_from_ast(
                                item,
                                bindings,
                                item_dot.as_ref(),
                                &body_output_guards,
                                &mut body_bindings,
                                &mut body_default_paths,
                                defines,
                                define_sources,
                                define_tree_cache,
                                fragment_helper_body_cache,
                                &mut item_seen,
                                analysis,
                            );
                        }
                    }
                } else {
                    for item in body {
                        Self::collect_bound_helper_values_from_ast(
                            item,
                            bindings,
                            body_dot.as_ref(),
                            &body_output_guards,
                            &mut body_bindings,
                            &mut body_default_paths,
                            defines,
                            define_sources,
                            define_tree_cache,
                            fragment_helper_body_cache,
                            seen,
                            analysis,
                        );
                    }
                }
                if !is_with
                    && range_binding
                        .as_ref()
                        .is_some_and(Self::helper_binding_definitely_nonempty_iterable)
                {
                    *local_bindings = body_bindings;
                    *local_default_paths = body_default_paths;
                } else {
                    let mut else_bindings = local_bindings.clone();
                    let mut else_default_paths = local_default_paths.clone();
                    for item in else_branch {
                        // `with ... else ...` else-branch executes with
                        // the outer `.`, not the with-shifted one.
                        Self::collect_bound_helper_values_from_ast(
                            item,
                            bindings,
                            current_dot,
                            active_output_guards,
                            &mut else_bindings,
                            &mut else_default_paths,
                            defines,
                            define_sources,
                            define_tree_cache,
                            fragment_helper_body_cache,
                            seen,
                            analysis,
                        );
                    }
                    *local_bindings = Self::merge_fragment_locals(body_bindings, else_bindings);
                    *local_default_paths =
                        merge_local_default_paths(body_default_paths, else_default_paths);
                }
            }
            HelmAst::Scalar { .. } | HelmAst::HelmComment { .. } => {}
        }
    }

    /// Resolve a `with` header's value to the binding callers should
    /// use as `current_dot` while walking the body. Returns `None` when
    /// the header can't be statically pinned to a single Values-rooted
    /// path (e.g. `with $variable`, `with .Chart.Name`, multi-expression
    /// headers): the body's dot is then unknown and the caller should
    /// leave `current_dot` untouched rather than guess.
    fn computed_with_body_dot(
        header: &str,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
    ) -> Option<HelperBinding> {
        let exprs = Self::parse_expr_text(header);
        let [expr] = exprs.as_slice() else {
            return None;
        };
        // Bare `.Values` — `single_resolved_values_path` returns None
        // for this because the strict path extractor requires at least
        // one segment after `Values`. Treat it as a Values-root binding
        // so the body's `.X` resolves to `.Values.X`.
        if matches!(expr, TemplateExpr::Field(path) if matches!(path.as_slice(), [head] if head == "Values"))
            || matches!(
                expr,
                TemplateExpr::Selector { operand, path }
                    if matches!(operand.as_ref(), TemplateExpr::Variable(v) if v.is_empty())
                        && matches!(path.as_slice(), [head] if head == "Values"),
            )
        {
            return Some(HelperBinding::ValuesPath(String::new()));
        }
        Self::binding_from_expr(expr, Some(bindings), current_dot)
    }

    fn resolved_default_fallback_paths_for_text(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();

        for expr in Self::parse_expr_text(text) {
            out.extend(Self::resolved_default_fallback_paths_for_expr(
                &expr,
                bindings,
                current_dot,
            ));
        }

        out
    }

    fn resolved_default_fallback_paths_for_expr(
        expr: &TemplateExpr,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        expr.walk(|node| match node {
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                out.extend(Self::resolve_expr_to_values_paths_in_context(
                    &args[1],
                    bindings,
                    current_dot,
                ));
            }
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let TemplateExpr::Call { function, .. } = &window[1] else {
                        continue;
                    };
                    if function != "default" {
                        continue;
                    }
                    out.extend(Self::resolve_expr_to_values_paths_in_context(
                        &window[0],
                        bindings,
                        current_dot,
                    ));
                }
            }
            _ => {}
        });
        out
    }

    fn resolved_type_is_paths_for_text(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if function != "typeIs" || args.len() < 2 {
                    return;
                }
                let Some(schema_type) = Self::type_is_schema_type(args.first()) else {
                    return;
                };
                let Some(binding) = Self::binding_from_expr(&args[1], bindings, current_dot) else {
                    return;
                };
                for path in Self::helper_binding_paths(&binding) {
                    if !path.trim().is_empty() {
                        out.entry(path).or_default().insert(schema_type.clone());
                    }
                }
            });
        }
        out
    }

    fn type_is_schema_type(expr: Option<&TemplateExpr>) -> Option<String> {
        let TemplateExpr::Literal(Literal::String(type_name) | Literal::RawString(type_name)) =
            expr?.deparen()
        else {
            return None;
        };
        let schema_type = match type_name.as_str() {
            "bool" | "boolean" => "boolean",
            "float64" | "number" => "number",
            "int" | "int64" | "integer" => "integer",
            "list" | "slice" | "array" => "array",
            "map" | "dict" | "object" => "object",
            "string" => "string",
            _ => return None,
        };
        Some(schema_type.to_string())
    }

    /// Extract Values-rooted paths that this helper-body text declares
    /// as chart-level defaults via the canonical pattern
    ///
    /// ```text
    /// {{- $_ := set OPERAND "KEY" (OPERAND.KEY | default V) }}
    /// ```
    ///
    /// This is the chart writer asserting "this path is defaulted before
    /// any subsequent read." Matched here (and *only* here, not by the
    /// broader `resolved_default_fallback_paths_for_text` which fires
    /// on every `| default` regardless of context — including condition
    /// fallbacks like `if X | default false` that do not mutate values).
    ///
    /// Requirements for a match:
    ///   - The action is a `set` call with exactly three arguments.
    ///   - The first argument resolves to a Values-rooted path through
    ///     the active bindings and `current_dot` (so `with .Values` shifts
    ///     correctly).
    ///   - The second argument is a string literal — the dict key being
    ///     defaulted.
    ///   - The third argument's expression tree contains a `default` call
    ///     anywhere within it. The chart writer's `| default V` (or
    ///     `default V .` form) is what signals the path is optional; a
    ///     bare `set X "K" V` without a default falls outside this rule
    ///     because it unconditionally overwrites and tells us nothing
    ///     about the values.yaml schema's null-tolerance.
    ///
    /// The emitted path is `<resolved-operand>.<key>` (or just `<key>`
    /// when the operand resolves to the Values root, i.e. inside a bare
    /// `with .Values` block).
    fn set_default_chart_paths_for_text(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        fn literal_string(expr: &TemplateExpr) -> Option<&str> {
            match expr {
                TemplateExpr::Literal(Literal::String(s) | Literal::RawString(s)) => Some(s),
                TemplateExpr::Parenthesized(inner) => literal_string(inner),
                _ => None,
            }
        }

        let mut out = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if function != "set" || args.len() != 3 {
                    return;
                }
                let Some(operand_path) =
                    Self::resolve_expr_to_values_path_in_context(&args[0], bindings, current_dot)
                else {
                    return;
                };
                let Some(key) = literal_string(&args[1]) else {
                    return;
                };
                let target_path = if operand_path.is_empty() {
                    key.to_string()
                } else {
                    format!("{operand_path}.{key}")
                };
                let defaulted_paths =
                    Self::resolved_default_fallback_paths_for_expr(&args[2], bindings, current_dot);
                if !defaulted_paths.contains(&target_path) {
                    return;
                }
                out.insert(target_path);
            });
        }
        out
    }

    fn collect_if_with_guards(&mut self, cond_text: &str) {
        let cond_guards = self.condition_guards_in_context(cond_text);

        for v in self.extract_bound_values(cond_text) {
            self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
        }

        for g in &cond_guards {
            for path in g.value_paths() {
                self.emit_use(path.to_string(), YamlPath(Vec::new()), ValueKind::Scalar);
                if matches!(g, Guard::Eq { .. } | Guard::TypeIs { .. }) {
                    self.emit_use_with_extra_guards(
                        path.to_string(),
                        YamlPath(Vec::new()),
                        ValueKind::Scalar,
                        std::slice::from_ref(g),
                    );
                }
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
                Guard::Default { .. } => vec![g],
                Guard::TypeIs { .. } => vec![g],
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
        let mut locals = self.template_bindings.clone();
        for (key, value) in &self.root_bindings {
            locals.insert(
                key.clone(),
                Self::fragment_binding_from_helper_binding(value),
            );
        }
        let current_dot = self.current_dot_fragment();
        let current_dot_binding = self.current_dot_binding();
        let exprs = Self::parse_expr_text(header_text);
        let binding = match exprs.as_slice() {
            [expr] => Self::fragment_binding_from_outer_expr(
                expr,
                Some(&locals),
                Some(&self.root_bindings),
                current_dot_binding.as_ref(),
            )
            .or_else(|| self.fragment_binding_in_context(expr, current_dot.as_ref())),
            _ => None,
        };
        self.dot_stack.push(binding);
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
            | "variable"
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
        if let Ok(txt) = node.utf8_text(self.source.as_bytes()) {
            if let Some((var, binding)) = Self::parse_get_binding(txt) {
                self.get_bindings.insert(var, binding);
            }

            if let Some((var, _declares, rhs)) = Self::parse_helper_assignment(txt) {
                let mut locals = self.template_bindings.clone();
                for (key, value) in &self.root_bindings {
                    locals.insert(
                        key.clone(),
                        Self::fragment_binding_from_helper_binding(value),
                    );
                }
                let current_dot = self
                    .current_dot_binding()
                    .map(|binding| Self::fragment_binding_from_helper_binding(&binding));
                let mut seen = HashSet::new();
                if let Some(binding) = Self::fragment_binding_from_text(
                    &rhs,
                    &locals,
                    current_dot.as_ref(),
                    self.defines,
                    &self.ir_context.inner.define_body_sources,
                    &self.ir_context.inner.define_tree_cache,
                    &self.fragment_helper_body_cache,
                    &mut seen,
                ) {
                    self.template_bindings.insert(var.clone(), binding);
                }
                let default_paths = self.resolved_default_fallback_paths_in_context(&rhs);
                if default_paths.is_empty() {
                    self.template_default_paths.remove(&var);
                } else {
                    self.template_default_paths
                        .insert(var.clone(), default_paths);
                }

                let helper_meta = self.helper_output_meta_for_text(&rhs);
                if helper_meta.is_empty() {
                    self.template_output_meta.remove(&var);
                } else {
                    self.template_output_meta.insert(var, helper_meta);
                }
            }
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
        let saved_template_bindings = self.template_bindings.clone();
        let saved_template_default_paths = self.template_default_paths.clone();
        let saved_template_output_meta = self.template_output_meta.clone();

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
        self.template_bindings = saved_template_bindings;
        self.template_default_paths = saved_template_default_paths;
        self.template_output_meta = saved_template_output_meta;

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
        let saved_template_bindings = self.template_bindings.clone();
        let saved_template_default_paths = self.template_default_paths.clone();
        let saved_template_output_meta = self.template_output_meta.clone();

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
        self.template_bindings = saved_template_bindings;
        self.template_default_paths = saved_template_default_paths;
        self.template_output_meta = saved_template_output_meta;

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
        let saved_template_bindings = self.template_bindings.clone();
        let saved_template_default_paths = self.template_default_paths.clone();
        let saved_template_output_meta = self.template_output_meta.clone();

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

        // If the range header is a single `.Values.*` path, treat `.` inside
        // the range body as one item of that collection:
        //   {{- range .Values.someList }}
        //     {{ .name }}
        //   {{- end }}
        let dot_prefix = header_text
            .as_deref()
            .and_then(|raw| self.direct_iterable_header_path(raw))
            .map(|path| FragmentBinding::ValuesPath(format!("{path}.*")));

        self.dot_stack.push(dot_prefix);

        let body = Self::children_with_field(node, "body");
        for ch in body {
            self.walk(ch);
        }

        self.guards.truncate(saved);
        self.dot_stack.truncate(saved_dot);
        self.range_domains = saved_domains;
        self.get_bindings = saved_bindings;
        self.template_bindings = saved_template_bindings;
        self.template_default_paths = saved_template_default_paths;
        self.template_output_meta = saved_template_output_meta;

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
    /// branch. A source-order primary would collapse mutually-exclusive
    /// branches before the provider chain can evaluate them.
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
    /// structurally (`{{ template "X" . | quote }}`,
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
            // `apiVersion` and `kind` can appear in either order in a K8s YAML
            // document header; neither field gates the other. This matters for
            // charts that render `kind: NetworkPolicy` before
            // `apiVersion: networking.k8s.io/v1`.
            if let Some(v) = Self::parse_literal_value(trimmed, "apiVersion") {
                det.attribute_api_version_literal(v);
            } else if let Some(name) = Self::parse_helper_call_value(trimmed, "apiVersion") {
                // `apiVersion: {{ template "X" . }}` and
                // `apiVersion: {{ include "X" . }}` are statically resolved
                // to the helper's typed output. Vendored charts commonly use
                // `<chart>.<resource>.apiVersion` helpers for resource headers.
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
                        // Helper bodies can carry typed branch structure.
                        // Propagate it verbatim so the chain can evaluate
                        // guards against its K8s cache and pick the first
                        // live branch. Also flatten into `api_versions` for
                        // callers that only consume candidate literals.
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
            // Typed branches come from helper-returned `HelperOutput::Branched`
            // values or inline `{{- if .Capabilities.APIVersions.Has "X" -}}`
            // chains around `apiVersion:`. The chain layer evaluates branch
            // guards against its K8s cache and selects the live branch. If only
            // a flat list of multi-branch literals is available, preserve every
            // alternative as a candidate with an empty primary so resolution is
            // delegated to the schema-provider chain.
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
mod fragment_binding_unit_tests {
    use super::{
        FragmentBinding, HelperBinding, SymbolicIrGenerator, SymbolicWalker,
        bound_helper_dependency_paths,
    };
    use crate::{IrGenerator, ValueKind};
    use helm_schema_ast::{
        DefineIndex, FusedRustParser, HelmAst, HelmParser, Literal, TemplateExpr, TreeSitterParser,
    };
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

    #[test]
    fn fragment_choice_deduplicates_identical_bindings() {
        let binding = FragmentBinding::Dict(BTreeMap::from([(
            "ctx".to_string(),
            FragmentBinding::ValuesPath("serviceAccount".to_string()),
        )]));

        let choice = SymbolicWalker::fragment_choice(vec![
            binding.clone(),
            binding.clone(),
            FragmentBinding::Choice([binding.clone()].into_iter().collect()),
        ]);

        assert_eq!(choice, Some(binding));
    }

    #[test]
    fn fragment_union_of_identical_bindings_stays_scalar() {
        let binding = FragmentBinding::ValuesPath("fullnameOverride".to_string());
        let merged = SymbolicWalker::fragment_union(Some(binding.clone()), Some(binding.clone()));
        assert_eq!(merged, Some(binding));
    }

    #[test]
    fn fragment_binding_tracks_defaulted_storage_class_assignment_rhs() {
        let rhs = "(.Values.global).storageClass | default .Values.persistence.storageClass | default (.Values.global).defaultStorageClass | default \"\"";
        let binding = SymbolicWalker::fragment_binding_from_text(
            rhs,
            &Default::default(),
            None,
            &DefineIndex::new(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &mut Default::default(),
        );
        assert_eq!(
            binding,
            Some(FragmentBinding::Choice(
                [
                    FragmentBinding::ValuesPath("global.defaultStorageClass".to_string()),
                    FragmentBinding::ValuesPath("global.storageClass".to_string()),
                    FragmentBinding::ValuesPath("persistence.storageClass".to_string()),
                    FragmentBinding::StringSet(["".to_string()].into_iter().collect()),
                ]
                .into_iter()
                .collect()
            ))
        );
    }

    #[test]
    fn values_root_bindings_apply_selectors_without_leading_dot() {
        let rest = ["serviceAccount".to_string(), "name".to_string()];

        assert_eq!(
            SymbolicWalker::apply_binding(&HelperBinding::ValuesPath(String::new()), &rest),
            Some("serviceAccount.name".to_string())
        );
        assert_eq!(
            SymbolicWalker::apply_binding(&HelperBinding::RootContext, &["Values".to_string()]),
            Some(String::new())
        );
        assert_eq!(
            SymbolicWalker::apply_binding(
                &HelperBinding::RootContext,
                &[
                    "Values".to_string(),
                    "serviceAccount".to_string(),
                    "name".to_string()
                ]
            ),
            Some("serviceAccount.name".to_string())
        );

        assert_eq!(
            SymbolicWalker::fragment_apply_binding(
                &FragmentBinding::ValuesPath(String::new()),
                &rest
            ),
            Some(FragmentBinding::ValuesPath(
                "serviceAccount.name".to_string()
            ))
        );

        assert_eq!(
            SymbolicWalker::fragment_apply_binding(
                &FragmentBinding::PathSet([String::new()].into_iter().collect()),
                &rest
            ),
            Some(FragmentBinding::PathSet(
                ["serviceAccount.name".to_string()].into_iter().collect()
            ))
        );
    }

    #[test]
    fn helper_fragment_outputs_include_to_yaml_root_fragment() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &TreeSitterParser,
                r#"
{{- define "labels" -}}
{{- if .Values.global.commonLabels }}
{{ toYaml .Values.global.commonLabels }}
{{- end }}
{{- end -}}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(".");
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "labels",
            arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        assert!(
            analysis.fragment_output_uses.iter().any(|output| {
                output.source_expr == "global.commonLabels"
                    && output.relative_path.0.is_empty()
                    && output.kind == ValueKind::Fragment
                    && output.meta.guards.contains("global.commonLabels")
            }),
            "helper toYaml fragment should be exposed as a guarded root-relative fragment use, got {:?}",
            analysis.fragment_output_uses
        );
    }

    #[test]
    fn helper_fragment_render_inside_with_stays_self_guarded_in_chart_facts() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &TreeSitterParser,
                r#"
{{- define "common.tplvalues.render" -}}
{{- .value | toYaml -}}
{{- end -}}
"#,
            )
            .expect("parse helpers");
        let src = r#"
apiVersion: networking.k8s.io/v1
kind: IngressClass
spec:
  {{- with .Values.controller.ingressClassResource.parameters }}
  parameters: {{ include "common.tplvalues.render" (dict "value" . "context" $) | nindent 4 }}
  {{- end }}
"#;
        let ast = TreeSitterParser.parse(src).expect("parse template");
        let ir = crate::IrGenerator::generate(&crate::SymbolicIrGenerator, src, &ast, &defines);
        let facts = crate::derive_chart_facts(&ir);
        let fact = facts
            .path_facts
            .get("controller.ingressClassResource.parameters")
            .unwrap_or_else(|| {
                panic!(
                    "expected chart facts for ingressClass parameters; ir={ir:#?}; facts={facts:#?}"
                )
            });

        assert!(
            fact.has_render_use,
            "helper-rendered ingressClass parameters should be a render use; ir={ir:#?}; facts={facts:#?}"
        );
        assert!(
            fact.all_render_uses_self_guarded,
            "the with-header guard must survive helper-fragment interpretation; ir={ir:#?}; facts={facts:#?}"
        );
    }

    #[test]
    fn non_destructured_range_merge_does_not_project_default_dict_values_to_root_fragment() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &TreeSitterParser,
                r#"
{{- define "common.names.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "common.tplvalues.render" -}}
{{- .value | toYaml -}}
{{- end -}}

{{- define "common.tplvalues.merge" -}}
{{- $dst := dict -}}
{{- range .values -}}
{{- $dst = include "common.tplvalues.render" (dict "value" . "context" $.context) | fromYaml | merge $dst -}}
{{- end -}}
{{ $dst | toYaml }}
{{- end -}}

{{- define "common.labels.standard" -}}
{{- $default := dict "app.kubernetes.io/name" (include "common.names.name" .context) -}}
{{ template "common.tplvalues.merge" (dict "values" (list .customLabels $default) "context" .context) }}
{{- end -}}
"#,
            )
            .expect("parse helpers");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(
            r#"(dict "customLabels" .Values.commonLabels "context" $)"#,
        );
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "common.labels.standard",
            arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        assert!(
            !analysis.fragment_output.contains("nameOverride"),
            "single-variable range reducers must not project default dict scalar fields as root fragments: structured={:?}, legacy={:?}",
            analysis.fragment_output_uses,
            analysis.fragment_output
        );
        assert!(
            analysis.fragment_output_uses.iter().all(|output| {
                output.source_expr != "nameOverride" || !output.relative_path.0.is_empty()
            }),
            "default label name must not be projected as a root fragment, got structured={:?}, legacy={:?}",
            analysis.fragment_output_uses,
            analysis.fragment_output
        );
        assert!(
            analysis.fragment_output_uses.iter().any(|output| {
                output.source_expr == "commonLabels"
                    && output.relative_path.0.is_empty()
                    && output.kind == ValueKind::Fragment
            }),
            "custom labels merged into common labels should render as a root-relative fragment, got structured={:?}, legacy={:?}",
            analysis.fragment_output_uses,
            analysis.fragment_output
        );
        assert!(
            analysis.fragment_output_uses.iter().all(|output| {
                output.source_expr != "commonLabels"
                    || !output.relative_path.0.contains(&"values[*]".to_string())
            }),
            "helper argument list envelopes must not leak into output paths, got structured={:?}",
            analysis.fragment_output_uses
        );
    }

    #[test]
    fn direct_rendered_outputs_skip_helper_call_arguments() {
        let bindings = HashMap::new();

        let helper_arg_only = r#"{{ include "common.images.pullSecrets" (dict "images" (list .Values.image .Values.volumePermissions.image) "global" .Values.global) }}"#;
        assert_eq!(
            SymbolicWalker::direct_bound_paths_from_text(helper_arg_only, &bindings),
            BTreeSet::new()
        );

        let rendered_by_printf = r#"{{ printf "%s" .Values.image.repository }}"#;
        assert_eq!(
            SymbolicWalker::direct_bound_paths_from_text(rendered_by_printf, &bindings),
            BTreeSet::from(["image.repository".to_string()])
        );
    }

    #[test]
    fn assignment_rhs_nested_helper_call_surfaces_dependencies() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &TreeSitterParser,
                r#"
{{- define "common.names.namespace" -}}
{{- default .Release.Namespace .Values.namespaceOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "common.secrets.lookup" -}}
{{- $secretData := (lookup "v1" "Secret" (include "common.names.namespace" .context) .secret).data -}}
{{- if and $secretData (hasKey $secretData .key) -}}
  {{- index $secretData .key -}}
{{- end -}}
{{- end -}}
"#,
            )
            .expect("parse helpers");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(
            r#"{{ include "common.secrets.lookup" (dict "secret" .Values.auth.existingSecret "key" "postgres-password" "context" $) }}"#,
        );
        let TemplateExpr::Call { args, .. } = &arg_exprs[0] else {
            panic!("expected include call");
        };
        assert!(
            defines.get("common.names.namespace").is_some(),
            "namespace helper should be registered"
        );
        assert!(
            defines.get("common.secrets.lookup").is_some(),
            "lookup helper should be registered"
        );
        let lookup_bindings = SymbolicWalker::bindings_for_helper_arg(args.get(1), None, None);
        assert!(
            matches!(
                lookup_bindings.get("context"),
                Some(HelperBinding::RootContext)
            ),
            "dict context argument should bind to the root context, got {lookup_bindings:?}"
        );
        let lookup_dot = SymbolicWalker::binding_from_expr(args.get(1).unwrap(), None, None);
        let namespace_arg = SymbolicWalker::parse_expr_text(".context");
        let direct_namespace = SymbolicWalker::analyze_bound_helper_call(
            "common.names.namespace",
            namespace_arg.first(),
            Some(&lookup_bindings),
            lookup_dot.as_ref(),
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut HashSet::new(),
        );
        let namespace_body_paths = SymbolicWalker::direct_bound_paths_from_text_in_context(
            r#"default .Release.Namespace .Values.namespaceOverride | trunc 63 | trimSuffix "-""#,
            &HashMap::new(),
            Some(&HelperBinding::RootContext),
        );
        assert!(
            namespace_body_paths.contains("namespaceOverride"),
            "direct path extraction should see namespaceOverride in namespace helper body, got {namespace_body_paths:?}"
        );
        assert!(
            bound_helper_dependency_paths(&direct_namespace).contains("namespaceOverride"),
            "direct .context-bound namespace helper should surface namespaceOverride as a dependency, body={}, got output={:?}, fragments={:?}",
            HelmAst::Document {
                items: defines
                    .get("common.names.namespace")
                    .unwrap_or_default()
                    .to_vec(),
            }
            .to_sexpr(),
            direct_namespace.output,
            direct_namespace.fragment_output_uses
        );
        let rhs_exprs = SymbolicWalker::parse_expr_text(
            r#"(lookup "v1" "Secret" (include "common.names.namespace" .context) .secret).data"#,
        );
        let mut nested_include_seen = false;
        for expr in &rhs_exprs {
            expr.walk(|node| {
                if let TemplateExpr::Call { function, args } = node
                    && function == "include"
                    && matches!(
                        args.first(),
                        Some(TemplateExpr::Literal(Literal::String(name)))
                            if name == "common.names.namespace"
                    )
                {
                    nested_include_seen = true;
                }
            });
        }
        assert!(
            nested_include_seen,
            "expression parser should expose nested include calls, got {rhs_exprs:?}"
        );
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "common.secrets.lookup",
            args.get(1),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        assert!(
            analysis.output.contains_key("namespaceOverride"),
            "nested helper call inside assignment RHS should surface namespaceOverride, got {:?}",
            analysis.output
        );
        assert!(
            analysis.output.contains_key("auth.existingSecret"),
            "assignment RHS should also retain direct helper argument dependencies, got {:?}",
            analysis.output
        );
    }

    #[test]
    fn tuple_bound_helper_dot_resolves_indexed_image_values() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &TreeSitterParser,
                r#"
{{- define "image" -}}
{{- $defaultTag := index . 1 -}}
{{- with index . 0 -}}
{{- if .registry -}}{{ printf "%s/%s" .registry .repository }}{{- else -}}{{- .repository -}}{{- end -}}
{{- if .digest -}}{{ printf "@%s" .digest }}{{- else -}}{{ printf ":%s" (default $defaultTag .tag) }}{{- end -}}
{{- end }}
{{- end }}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let body_sexpr = helm_schema_ast::HelmAst::Document {
            items: defines.get("image").unwrap_or_default().to_vec(),
        }
        .to_sexpr();
        let arg_exprs = SymbolicWalker::parse_expr_text(
            r#"{{ template "image" (tuple .Values.image $.Chart.AppVersion) }}"#,
        );
        let TemplateExpr::Call { args, .. } = &arg_exprs[0] else {
            panic!("expected template call");
        };
        let helper_dot = args
            .get(1)
            .and_then(|expr| SymbolicWalker::binding_from_expr(expr, None, None));
        assert!(
            matches!(
                helper_dot,
                Some(HelperBinding::List(ref items))
                    if matches!(items.first(), Some(HelperBinding::ValuesPath(path)) if path == "image")
            ),
            "tuple helper arg should preserve image in first slot, got {helper_dot:?}",
        );
        let empty_bindings = HashMap::new();
        let with_dot = SymbolicWalker::computed_with_body_dot(
            "index . 0",
            &empty_bindings,
            helper_dot.as_ref(),
        );
        assert!(
            matches!(with_dot, Some(HelperBinding::ValuesPath(ref path)) if path == "image"),
            "with index . 0 should shift dot to image, got {with_dot:?}",
        );
        assert_eq!(
            SymbolicWalker::direct_bound_paths_from_text_in_context(
                "printf \"%s/%s\" .registry .repository",
                &empty_bindings,
                with_dot.as_ref(),
            ),
            BTreeSet::from(["image.registry".to_string(), "image.repository".to_string()])
        );
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "image",
            args.get(1),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        let output_source = |path: &str| {
            analysis
                .fragment_output_uses
                .iter()
                .find(|output| output.source_expr == path)
        };
        assert!(
            output_source("image.repository").is_some(),
            "tuple-bound helper should surface image.repository, body={body_sexpr}, got {:?}",
            analysis.fragment_output_uses
        );
        assert!(
            output_source("image.registry").is_some(),
            "tuple-bound helper should surface image.registry, got {:?}",
            analysis.fragment_output_uses
        );
        assert!(
            output_source("image.digest").is_some(),
            "tuple-bound helper should surface image.digest, got {:?}",
            analysis.fragment_output_uses
        );
        assert!(
            output_source("image.tag").is_some_and(|output| output.meta.defaulted),
            "tuple-bound helper should surface defaulted image.tag, got {:?}",
            analysis.fragment_output_uses
        );
    }

    #[test]
    fn wrapper_helper_fragment_outputs_preserve_nested_local_assignments() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                r#"
{{- define "common.images.image" -}}
{{- $registryName := .imageRoot.registry -}}
{{- $repositoryName := .imageRoot.repository -}}
{{- $termination := .imageRoot.tag | toString -}}
{{- if .global }}
  {{- if .global.imageRegistry }}
    {{- $registryName = .global.imageRegistry -}}
  {{- end -}}
{{- end -}}
{{- printf "%s/%s:%s" $registryName $repositoryName $termination -}}
{{- end -}}

{{- define "app.image" -}}
{{ include "common.images.image" (dict "imageRoot" .Values.image "global" .Values.global) }}
{{- end -}}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(".");
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "app.image",
            arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        let outputs: BTreeSet<_> = analysis
            .fragment_output_uses
            .iter()
            .map(|output| output.source_expr.as_str())
            .collect();
        assert!(
            outputs.contains("image.registry")
                && outputs.contains("image.repository")
                && outputs.contains("image.tag"),
            "wrapper helper should surface nested local-assignment outputs, got structured={:?}, legacy_fragment={:?}, scalar={:?}, guards={:?}",
            analysis.fragment_output_uses,
            analysis.fragment_output,
            analysis.output,
            analysis.guard_paths
        );
    }

    #[test]
    fn image_pull_secret_helper_does_not_surface_image_root_as_output() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                r#"
{{- define "common.tplvalues.render" -}}
{{- $value := typeIs "string" .value | ternary .value (.value | toYaml) }}
{{- if contains "{{" (toJson .value) }}
  {{- tpl $value .context }}
{{- else }}
  {{- $value }}
{{- end }}
{{- end -}}

{{- define "common.images.renderPullSecrets" -}}
  {{- $pullSecrets := list }}
  {{- range .images -}}
    {{- range .pullSecrets -}}
      {{- if kindIs "map" . -}}
        {{- $pullSecrets = append $pullSecrets (include "common.tplvalues.render" (dict "value" .name "context" $.context)) -}}
      {{- else -}}
        {{- $pullSecrets = append $pullSecrets (include "common.tplvalues.render" (dict "value" . "context" $.context)) -}}
      {{- end -}}
    {{- end -}}
  {{- end -}}
  {{- if (not (empty $pullSecrets)) -}}
imagePullSecrets:
    {{- range $pullSecrets | uniq }}
  - name: {{ . }}
    {{- end }}
  {{- end }}
{{- end -}}

{{- define "minio.imagePullSecrets" -}}
{{- include "common.images.renderPullSecrets" (dict "images" (list .Values.image .Values.console.image .Values.clientImage .Values.defaultInitContainers.volumePermissions.image) "context" $) -}}
{{- end -}}
"#,
            )
            .expect("parse helpers");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(".");
        let fragment_locals = HashMap::new();
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call_with_fragment_locals(
            "minio.imagePullSecrets",
            arg_exprs.first(),
            None,
            Some(&HelperBinding::RootContext),
            &fragment_locals,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        assert!(
            !analysis.output.contains_key("image"),
            "image root should remain a dependency, not a rendered output: output={:#?}, fragment={:#?}",
            analysis.output,
            analysis.fragment_output_uses
        );
    }

    #[test]
    fn printf_yaml_key_fragment_places_local_value_under_emitted_key() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                r#"
{{- define "common.storage.class" -}}
{{- $storageClass := .persistence.storageClass -}}
{{- if .global -}}
  {{- if .global.storageClass -}}
    {{- $storageClass = .global.storageClass -}}
  {{- end -}}
{{- end -}}
{{- if $storageClass -}}
  {{- printf "storageClassName: %s" $storageClass -}}
{{- end -}}
{{- end -}}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(
            r#"(dict "persistence" .Values.persistence "global" .Values.global)"#,
        );
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "common.storage.class",
            arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        for source_expr in ["persistence.storageClass", "global.storageClass"] {
            assert!(
                analysis.fragment_output_uses.iter().any(|output| {
                    output.source_expr == source_expr
                        && output.relative_path.0 == vec!["storageClassName".to_string()]
                }),
                "{source_expr} should render under storageClassName, got {:?}",
                analysis.fragment_output_uses
            );
        }
    }

    #[test]
    fn appended_pull_secrets_render_as_image_pull_secret_names() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                r#"
{{- define "common.images.pullSecrets" -}}
  {{- $pullSecrets := list }}
  {{- if .global }}
    {{- range .global.imagePullSecrets -}}
      {{- $pullSecrets = append $pullSecrets . -}}
    {{- end -}}
  {{- end -}}
  {{- range .images -}}
    {{- range .pullSecrets -}}
      {{- $pullSecrets = append $pullSecrets . -}}
    {{- end -}}
  {{- end -}}
  {{- if (not (empty $pullSecrets)) }}
imagePullSecrets:
    {{- range $pullSecrets | uniq }}
  - name: {{ . }}
{{- end }}
{{- end }}
{{- end -}}

{{- define "app.imagePullSecrets" -}}
{{ include "common.images.pullSecrets" (dict "images" (list .Values.image .Values.volumePermissions.image) "global" .Values.global) }}
{{- end -}}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(
            r#"(dict "images" (list .Values.image .Values.volumePermissions.image) "global" .Values.global)"#,
        );
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "common.images.pullSecrets",
            arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        for source_expr in [
            "global.imagePullSecrets.*",
            "image.pullSecrets.*",
            "volumePermissions.image.pullSecrets.*",
        ] {
            assert!(
                !analysis.output.contains_key(source_expr),
                "{source_expr} should not also emit as a broad legacy helper output, got scalar={:?}",
                analysis.output
            );
            assert!(
                analysis.fragment_output_uses.iter().any(|output| {
                    output.source_expr == source_expr
                        && output.relative_path.0
                            == vec!["imagePullSecrets[*]".to_string(), "name".to_string()]
                }),
                "{source_expr} should render as imagePullSecrets[*].name, got structured={:?}, legacy={:?}, scalar={:?}, guards={:?}",
                analysis.fragment_output_uses,
                analysis.fragment_output,
                analysis.output,
                analysis.guard_paths
            );
        }

        let wrapper_arg_exprs = SymbolicWalker::parse_expr_text(".");
        let mut seen = HashSet::new();
        let wrapper_analysis = SymbolicWalker::analyze_bound_helper_call(
            "app.imagePullSecrets",
            wrapper_arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );
        for source_expr in [
            "global.imagePullSecrets.*",
            "image.pullSecrets.*",
            "volumePermissions.image.pullSecrets.*",
        ] {
            assert!(
                !wrapper_analysis.output.contains_key(source_expr),
                "{source_expr} should not emit as a broad wrapper helper output, got scalar={:?}",
                wrapper_analysis.output
            );
            assert!(
                wrapper_analysis.fragment_output_uses.iter().any(|output| {
                    output.source_expr == source_expr
                        && output.relative_path.0
                            == vec!["imagePullSecrets[*]".to_string(), "name".to_string()]
                }),
                "{source_expr} should render through wrapper as imagePullSecrets[*].name, got structured={:?}, legacy={:?}, scalar={:?}, guards={:?}",
                wrapper_analysis.fragment_output_uses,
                wrapper_analysis.fragment_output,
                wrapper_analysis.output,
                wrapper_analysis.guard_paths
            );
            assert!(
                wrapper_analysis.fragment_output_uses.iter().all(|output| {
                    output.source_expr != source_expr || !output.relative_path.0.is_empty()
                }),
                "{source_expr} should not have a root-relative structured wrapper output, got {:?}",
                wrapper_analysis.fragment_output_uses
            );
        }
    }

    #[test]
    fn forwarded_dot_helper_preserves_nested_affinity_value_slots() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                r#"
{{- define "common.affinities.nodes.soft" -}}
preferredDuringSchedulingIgnoredDuringExecution:
  - preference:
      matchExpressions:
        - key: {{ .key }}
          operator: In
          values:
            {{- range .values }}
            - {{ . | quote }}
            {{- end }}
    weight: 1
{{- end -}}

{{- define "common.affinities.nodes.hard" -}}
requiredDuringSchedulingIgnoredDuringExecution:
  nodeSelectorTerms:
    - matchExpressions:
        - key: {{ .key }}
          operator: In
          values:
            {{- range .values }}
            - {{ . | quote }}
            {{- end }}
{{- end -}}

{{- define "common.affinities.nodes" -}}
  {{- if eq .type "soft" }}
    {{- include "common.affinities.nodes.soft" . -}}
  {{- else if eq .type "hard" }}
    {{- include "common.affinities.nodes.hard" . -}}
  {{- end -}}
{{- end -}}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(
            r#"(dict "type" .Values.nodeAffinityPreset.type "key" .Values.nodeAffinityPreset.key "values" .Values.nodeAffinityPreset.values)"#,
        );
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "common.affinities.nodes",
            arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        let values_paths: BTreeSet<Vec<String>> = analysis
            .fragment_output_uses
            .iter()
            .filter(|output| output.source_expr == "nodeAffinityPreset.values.*")
            .map(|output| output.relative_path.0.clone())
            .collect();
        assert!(
            values_paths.contains(&vec![
                "preferredDuringSchedulingIgnoredDuringExecution[*]".to_string(),
                "preference".to_string(),
                "matchExpressions[*]".to_string(),
                "values[*]".to_string(),
            ]) && values_paths.contains(&vec![
                "requiredDuringSchedulingIgnoredDuringExecution".to_string(),
                "nodeSelectorTerms[*]".to_string(),
                "matchExpressions[*]".to_string(),
                "values[*]".to_string(),
            ]),
            "forwarded dot should preserve nested affinity values slots, got structured={:?}, scalar={:?}, legacy={:?}",
            analysis.fragment_output_uses,
            analysis.output,
            analysis.fragment_output
        );
        assert!(
            analysis.fragment_output_uses.iter().all(|output| {
                output.source_expr != "nodeAffinityPreset.values.*"
                    || !output.relative_path.0.is_empty()
            }),
            "nodeAffinityPreset.values.* should not have a broad root-relative output, got {:?}",
            analysis.fragment_output_uses
        );
    }

    #[test]
    fn helper_else_branch_default_fallback_surfaces_values_path() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                r#"
{{- define "common.capabilities.kubeVersion" -}}
{{- if .Values.global }}
    {{- if .Values.global.kubeVersion }}
    {{- .Values.global.kubeVersion -}}
    {{- else }}
    {{- default .Capabilities.KubeVersion.Version .Values.kubeVersion -}}
    {{- end -}}
{{- else }}
{{- default .Capabilities.KubeVersion.Version .Values.kubeVersion -}}
{{- end -}}
{{- end -}}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(".");
        let mut seen = HashSet::new();
        let empty_bindings = HashMap::new();

        assert_eq!(
            SymbolicWalker::direct_bound_paths_from_text(
                ".Values.global.kubeVersion",
                &empty_bindings,
            ),
            BTreeSet::from(["global.kubeVersion".to_string()])
        );
        assert_eq!(
            SymbolicWalker::direct_bound_paths_from_text(
                "default .Capabilities.KubeVersion.Version .Values.kubeVersion",
                &empty_bindings,
            ),
            BTreeSet::from(["kubeVersion".to_string()])
        );

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "common.capabilities.kubeVersion",
            arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );
        let helper_body_dump = defines
            .get("common.capabilities.kubeVersion")
            .map(|body| helm_schema_ast::HelmAst::Document {
                items: body.to_vec(),
            })
            .map(|ast| ast.to_sexpr());

        let exposed_sources: BTreeSet<String> = analysis
            .output
            .keys()
            .chain(
                analysis
                    .fragment_output_uses
                    .iter()
                    .map(|output| &output.source_expr),
            )
            .cloned()
            .collect();
        assert!(
            exposed_sources.contains("global.kubeVersion"),
            "global kubeVersion branch should be surfaced, got scalar={:?}, structured={:?}, body={:?}",
            analysis.output,
            analysis.fragment_output_uses,
            helper_body_dump
        );
        assert!(
            exposed_sources.contains("kubeVersion"),
            "default fallback branch should surface top-level kubeVersion, got scalar={:?}, structured={:?}, body={:?}",
            analysis.output,
            analysis.fragment_output_uses,
            helper_body_dump
        );
    }

    #[test]
    fn helper_set_default_mutation_declares_chart_default_path() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                r#"
{{- define "synth.defaultValues" }}
{{- $name := include "synth.fullname" . }}
{{- with .Values }}
{{- $_ := set .serviceAccount "name" (.serviceAccount.name | default $name) }}
{{- end }}
{{- end }}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(".");
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "synth.defaultValues",
            arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        assert_eq!(
            analysis.chart_defaults,
            BTreeSet::from(["serviceAccount.name".to_string()])
        );
    }

    #[test]
    fn local_set_mutation_shadows_computed_patch_keys() {
        let defines = DefineIndex::new();
        let walker = SymbolicWalker::new("", &defines);
        let mut local_bindings = HashMap::from([
            (
                "patch".to_string(),
                FragmentBinding::ValuesPath("serviceAccount.patch.*".to_string()),
            ),
            (
                "opPathKey".to_string(),
                FragmentBinding::StringSet(["path".to_string(), "from".to_string()].into()),
            ),
        ]);
        let mut seen = HashSet::new();

        assert!(SymbolicWalker::apply_local_set_mutations(
            r#"$_ := set $patch (printf "%sKey" $opPathKey) $key"#,
            &mut local_bindings,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        ));

        let patch = local_bindings.get("patch").expect("patch binding");
        let path_key = SymbolicWalker::fragment_apply_binding(patch, &["pathKey".to_string()])
            .expect("pathKey binding");
        assert!(
            SymbolicWalker::fragment_paths(&path_key).is_empty(),
            "set-created pathKey must not remain a values input: {path_key:?}"
        );
        let op =
            SymbolicWalker::fragment_apply_binding(patch, &["op".to_string()]).expect("op binding");
        assert_eq!(
            SymbolicWalker::fragment_paths(&op),
            BTreeSet::from(["serviceAccount.patch.*.op".to_string()])
        );
    }

    #[test]
    fn range_variable_item_binding_uses_local_list_items() {
        let defines = DefineIndex::new();
        let walker = SymbolicWalker::new("", &defines);
        let local_bindings = HashMap::from([(
            "opPathKeys".to_string(),
            FragmentBinding::List(vec![
                FragmentBinding::StringSet(["path".to_string()].into()),
                FragmentBinding::StringSet(["from".to_string()].into()),
            ]),
        )]);
        let mut seen = HashSet::new();

        let binding = SymbolicWalker::range_variable_item_binding(
            "range $opPathKey := $opPathKeys",
            &local_bindings,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        assert!(
            matches!(
                binding,
                Some((ref var, FragmentBinding::Choice(ref choices)))
                    if var == "opPathKey"
                        && choices.iter().flat_map(SymbolicWalker::fragment_strings).collect::<BTreeSet<_>>()
                            == BTreeSet::from(["path".to_string(), "from".to_string()])
            ),
            "range item should bind opPathKey to the list item string set, got {binding:?}"
        );
    }

    #[test]
    fn range_variable_item_binding_handles_patch_variable_name() {
        let defines = DefineIndex::new();
        let walker = SymbolicWalker::new("", &defines);
        let local_bindings = HashMap::from([(
            "patches".to_string(),
            FragmentBinding::ValuesPath("config.patch".to_string()),
        )]);
        let mut seen = HashSet::new();

        let binding = SymbolicWalker::range_variable_item_binding(
            "$patch := $patches",
            &local_bindings,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        assert!(
            matches!(
                binding,
                Some((ref var, FragmentBinding::ValuesPath(ref path)))
                    if var == "patch" && path == "config.patch.*"
            ),
            "range item should bind patch to config.patch.*, got {binding:?}"
        );
    }

    #[test]
    fn local_bound_paths_resolve_patch_item_selectors() {
        let local_bindings = HashMap::from([(
            "patch".to_string(),
            FragmentBinding::ValuesPath("config.patch.*".to_string()),
        )]);

        assert_eq!(
            SymbolicWalker::local_bound_paths_from_text(
                r#"or (eq $patch.op "copy") (eq $patch.op "test")"#,
                &local_bindings,
            ),
            BTreeSet::from([
                "config.patch.*".to_string(),
                "config.patch.*.op".to_string()
            ])
        );
    }

    #[test]
    fn helper_analysis_shadows_computed_set_keys_through_local_aliases() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                r#"
{{- define "synth.jsonpatch" -}}
{{- $params := . -}}
{{- $patches := $params.patch -}}
{{- range $patch := $patches -}}
  {{- $opPathKeys := list "path" -}}
  {{- if eq $patch.op "copy" -}}
    {{- $opPathKeys = append $opPathKeys "from" -}}
  {{- end -}}
  {{- range $opPathKey := $opPathKeys -}}
    {{- $_ := set $patch (printf "%sKey" $opPathKey) "doc" -}}
  {{- end -}}
  {{- if or (eq $patch.op "copy") (eq $patch.op "test") -}}
    {{- $key := $patch.fromKey -}}
    {{- if eq $patch.op "test" -}}
      {{- $key = $patch.pathKey -}}
      {{- $_ := set $patch "testValue" "doc" -}}
    {{- end -}}
  {{- end -}}
  {{- $op := $patch.op -}}
  {{- $path := $patch.path -}}
  {{- $from := $patch.from -}}
{{- end -}}
{{- end -}}

{{- define "synth.loadMergePatch" -}}
{{- $doc := dict -}}
{{- get (include "synth.jsonpatch" (dict "doc" $doc "patch" (.patch | default list)) | fromJson) "doc" | toYaml -}}
{{- end -}}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(
            r#"(merge (dict "file" "service-account.yaml" "ctx" $) .Values.serviceAccount)"#,
        );
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call(
            "synth.loadMergePatch",
            arg_exprs.first(),
            None,
            None,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        let paths = bound_helper_dependency_paths(&analysis);
        assert!(
            paths.contains("serviceAccount.patch.*.op"),
            "patch op is a real input field and should remain visible: {paths:#?}"
        );
        assert!(
            paths.contains("serviceAccount.patch.*.path"),
            "patch path is a real input field and should remain visible: {paths:#?}"
        );
        assert!(
            paths.contains("serviceAccount.patch.*.from"),
            "patch from is a real input field and should remain visible: {paths:#?}"
        );
        assert!(
            !paths.iter().any(|path| path.ends_with("Key")),
            "computed set-created keys are helper-local state, not values inputs: {paths:#?}"
        );
    }

    #[test]
    fn helper_analysis_shadows_computed_set_keys_for_path_set_patch_sources() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                r#"
{{- define "synth.jsonpatch" -}}
{{- $params := . -}}
{{- $patches := $params.patch -}}
{{- range $patch := $patches -}}
  {{- $opPathKeys := list "path" -}}
  {{- $opPathKeys = append $opPathKeys "from" -}}
  {{- range $opPathKey := $opPathKeys -}}
    {{- $_ := set $patch (printf "%sKey" $opPathKey) "doc" -}}
  {{- end -}}
  {{- $fromKey := $patch.fromKey -}}
  {{- $pathKey := $patch.pathKey -}}
  {{- $op := $patch.op -}}
{{- end -}}
{{- end -}}
"#,
            )
            .expect("parse helper");
        let walker = SymbolicWalker::new("", &defines);
        let patch_sources = FragmentBinding::PathSet(BTreeSet::from([
            "config.patch".to_string(),
            "serviceAccount.patch".to_string(),
        ]));
        let arg = TemplateExpr::Call {
            function: "dict".to_string(),
            args: vec![
                TemplateExpr::Literal(Literal::String("patch".to_string())),
                TemplateExpr::Variable("patches".to_string()),
            ],
        };
        let fragment_locals = HashMap::from([("patches".to_string(), patch_sources)]);
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call_with_fragment_locals(
            "synth.jsonpatch",
            Some(&arg),
            None,
            None,
            &fragment_locals,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        let paths = bound_helper_dependency_paths(&analysis);
        assert!(
            paths.contains("config.patch.*.op") && paths.contains("serviceAccount.patch.*.op"),
            "real patch item fields should remain visible: {paths:#?}"
        );
        assert!(
            !paths.iter().any(|path| path.ends_with("Key")),
            "computed set-created keys are helper-local state, not values inputs: {paths:#?}"
        );
    }

    #[test]
    fn nats_default_values_does_not_surface_jsonpatch_scratch_fields() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                include_str!("../../../testdata/charts/nats/templates/_helpers.tpl"),
            )
            .expect("parse helpers");
        defines
            .add_source(
                &FusedRustParser,
                include_str!("../../../testdata/charts/nats/templates/_jsonpatch.tpl"),
            )
            .expect("parse jsonpatch");
        defines
            .add_source(
                &FusedRustParser,
                include_str!("../../../testdata/charts/nats/templates/_tplYaml.tpl"),
            )
            .expect("parse tplYaml");
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(".");
        let fragment_locals = HashMap::new();
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call_with_fragment_locals(
            "nats.defaultValues",
            arg_exprs.first(),
            None,
            None,
            &fragment_locals,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        let paths = bound_helper_dependency_paths(&analysis);
        assert!(
            !paths.iter().any(|path| {
                path.ends_with(".pathKey")
                    || path.ends_with(".fromKey")
                    || path.ends_with(".testValue")
            }),
            "jsonpatch scratch fields must not be inferred as values inputs: {paths:#?}"
        );
    }

    #[test]
    fn nats_jsonpatch_does_not_surface_scratch_fields() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &FusedRustParser,
                include_str!("../../../testdata/charts/nats/templates/_jsonpatch.tpl"),
            )
            .expect("parse jsonpatch");
        let jsonpatch_ast = FusedRustParser
            .parse(include_str!(
                "../../../testdata/charts/nats/templates/_jsonpatch.tpl"
            ))
            .expect("parse jsonpatch ast");
        assert!(
            jsonpatch_ast
                .to_sexpr()
                .contains("(Range \"$patch := $patches\""),
            "jsonpatch range binder should be preserved in the AST"
        );
        let walker = SymbolicWalker::new("", &defines);
        let arg_exprs = SymbolicWalker::parse_expr_text(r#"(dict "patch" .Values.config.patch)"#);
        let fragment_locals = HashMap::new();
        let mut seen = HashSet::new();

        let analysis = SymbolicWalker::analyze_bound_helper_call_with_fragment_locals(
            "jsonpatch",
            arg_exprs.first(),
            None,
            None,
            &fragment_locals,
            &defines,
            &walker.ir_context.inner.define_body_sources,
            &walker.ir_context.inner.define_tree_cache,
            &walker.fragment_helper_body_cache,
            &mut seen,
        );

        let paths = bound_helper_dependency_paths(&analysis);
        assert!(
            paths.contains("config.patch"),
            "jsonpatch should still surface the caller-provided patch input: {paths:#?}"
        );
        assert!(
            !paths.iter().any(|path| {
                path.ends_with(".pathKey")
                    || path.ends_with(".fromKey")
                    || path.ends_with(".testValue")
            }),
            "jsonpatch scratch fields must not be inferred as values inputs: {paths:#?}"
        );
    }

    #[test]
    fn local_alias_value_path_resolution_uses_template_bindings() {
        let defines = DefineIndex::new();
        let mut walker = SymbolicWalker::new("", &defines);
        walker.template_bindings.insert(
            "storageClass".to_string(),
            FragmentBinding::ValuesPath("global.storageClass".to_string()),
        );
        let resolved = walker.resolved_values_paths_in_context("$storageClass");
        assert_eq!(resolved, vec!["global.storageClass".to_string()]);
    }

    #[test]
    fn default_function_binding_preserves_given_and_fallback_paths() {
        let defines = DefineIndex::new();
        let mut seen = HashSet::new();
        let binding = SymbolicWalker::fragment_binding_from_text(
            "default .Values.persistence.storageClass .Values.global.storageClass",
            &HashMap::new(),
            None,
            &defines,
            &HashMap::new(),
            &std::cell::RefCell::new(HashMap::new()),
            &std::cell::RefCell::new(BTreeMap::new()),
            &mut seen,
        )
        .expect("binding");

        let paths = SymbolicWalker::fragment_paths(&binding);
        assert_eq!(
            paths,
            BTreeSet::from([
                "global.storageClass".to_string(),
                "persistence.storageClass".to_string(),
            ])
        );
    }

    #[test]
    fn eq_condition_on_default_alias_surfaces_all_alias_paths() {
        let defines = DefineIndex::new();
        let mut walker = SymbolicWalker::new("", &defines);
        walker.template_bindings.insert(
            "storageClass".to_string(),
            FragmentBinding::Choice(BTreeSet::from([
                FragmentBinding::ValuesPath("global.storageClass".to_string()),
                FragmentBinding::ValuesPath("persistence.storageClass".to_string()),
            ])),
        );

        for condition in [r#"(eq "-" $storageClass)"#, r#"eq "-" $storageClass"#] {
            let guards = walker.condition_guards_in_context(condition);
            assert_eq!(
                guards,
                vec![
                    crate::Guard::Eq {
                        path: "global.storageClass".to_string(),
                        value: "-".to_string(),
                    },
                    crate::Guard::Eq {
                        path: "persistence.storageClass".to_string(),
                        value: "-".to_string(),
                    },
                ],
                "condition {condition}"
            );
        }
    }

    #[test]
    fn ne_condition_on_default_alias_does_not_invent_truthy_guard() {
        let defines = DefineIndex::new();
        let mut walker = SymbolicWalker::new("", &defines);
        walker.template_bindings.insert(
            "storageClass".to_string(),
            FragmentBinding::Choice(BTreeSet::from([
                FragmentBinding::ValuesPath("global.storageClass".to_string()),
                FragmentBinding::ValuesPath("persistence.storageClass".to_string()),
            ])),
        );

        let guards = walker.condition_guards_in_context(r#"ne "-" $storageClass"#);
        assert!(
            guards.is_empty(),
            "`ne` has no exact guard representation and must not be widened to truthiness: {guards:#?}"
        );
    }

    #[test]
    fn multi_arg_eq_condition_on_default_alias_does_not_pick_one_literal() {
        let defines = DefineIndex::new();
        let mut walker = SymbolicWalker::new("", &defines);
        walker.template_bindings.insert(
            "storageClass".to_string(),
            FragmentBinding::Choice(BTreeSet::from([
                FragmentBinding::ValuesPath("global.storageClass".to_string()),
                FragmentBinding::ValuesPath("persistence.storageClass".to_string()),
            ])),
        );

        let guards = walker.condition_guards_in_context(r#"eq $storageClass "-" "" "#);
        assert!(
            guards.is_empty(),
            "multi-argument `eq` is a disjunction and must not become a single equality guard: {guards:#?}"
        );
    }

    #[test]
    fn not_condition_on_default_alias_surfaces_not_guards() {
        let defines = DefineIndex::new();
        let mut walker = SymbolicWalker::new("", &defines);
        walker.template_bindings.insert(
            "storageClass".to_string(),
            FragmentBinding::Choice(BTreeSet::from([
                FragmentBinding::ValuesPath("global.storageClass".to_string()),
                FragmentBinding::ValuesPath("persistence.storageClass".to_string()),
            ])),
        );

        let guards = walker.condition_guards_in_context(r#"not $storageClass"#);
        assert_eq!(
            guards,
            vec![
                crate::Guard::Not {
                    path: "global.storageClass".to_string(),
                },
                crate::Guard::Not {
                    path: "persistence.storageClass".to_string(),
                },
            ]
        );
    }

    #[test]
    fn or_condition_on_default_alias_surfaces_or_guard() {
        let defines = DefineIndex::new();
        let mut walker = SymbolicWalker::new("", &defines);
        walker.template_bindings.insert(
            "storageClass".to_string(),
            FragmentBinding::Choice(BTreeSet::from([
                FragmentBinding::ValuesPath("global.storageClass".to_string()),
                FragmentBinding::ValuesPath("persistence.storageClass".to_string()),
            ])),
        );

        let guards = walker.condition_guards_in_context(r#"or $storageClass .Values.enabled"#);
        assert_eq!(
            guards,
            vec![crate::Guard::Or {
                paths: vec![
                    "enabled".to_string(),
                    "global.storageClass".to_string(),
                    "persistence.storageClass".to_string(),
                ],
            }]
        );
    }

    #[test]
    fn default_alias_render_surfaces_given_and_fallback_paths() {
        let src = r#"
apiVersion: example.com/v1
kind: Widget
spec:
  {{- $storageClass := default .Values.persistence.storageClass .Values.global.storageClass -}}
  {{- if $storageClass }}
  {{- if (eq "-" $storageClass) }}
  storageClassName: ""
  {{- else }}
  storageClassName: {{ $storageClass }}
  {{- end }}
  {{- end }}
"#;
        let ast = TreeSitterParser.parse(src).expect("parse");
        let defines = DefineIndex::new();
        let uses = SymbolicIrGenerator.generate(src, &ast, &defines);
        let rendered_paths: BTreeSet<String> = uses
            .iter()
            .filter(|use_| !use_.path.0.is_empty())
            .map(|use_| use_.source_expr.clone())
            .collect();

        assert!(
            rendered_paths.contains("global.storageClass"),
            "global storage class should render through alias: {uses:#?}"
        );
        assert!(
            rendered_paths.contains("persistence.storageClass"),
            "persistence storage class should render through alias: {uses:#?}"
        );
        let eq_guard_paths: BTreeSet<String> = uses
            .iter()
            .flat_map(|use_| &use_.guards)
            .filter_map(|guard| match guard {
                crate::Guard::Eq { path, value } if value == "-" => Some(path.clone()),
                _ => None,
            })
            .collect();
        assert!(
            eq_guard_paths.contains("global.storageClass"),
            "global storage class should carry eq guard: {uses:#?}"
        );
        assert!(
            eq_guard_paths.contains("persistence.storageClass"),
            "persistence storage class should carry eq guard: {uses:#?}"
        );
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

    /// Inline `{{- if .Capabilities.APIVersions.Has "policy/v1" -}}` /
    /// `{{- else }}` chains around an `apiVersion:` line are mutually
    /// exclusive runtime branches. The detector captures both literals
    /// as typed branches so the chain layer can attribute MissingSchema
    /// to the selected fallback branch instead of emitting one
    /// diagnostic per alternative.
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

    /// Opaque `{{- if -}}` conditions do not produce typed branches:
    /// there is no decodable guard for the provider chain to evaluate,
    /// so resolution falls back to source-order primary plus candidate
    /// alternatives.
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

    /// Nested opaque `{{- if -}}` blocks do not interfere with an inner
    /// capability-guarded branch chain. The depth counter keeps the
    /// inner branch tracker scoped to its own `{{- end }}`.
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
