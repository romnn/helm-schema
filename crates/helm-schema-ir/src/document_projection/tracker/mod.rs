use helm_schema_ast::{DefineIndex, TemplateExpr};

use crate::fragment_classification::is_fragment_exprs;
use crate::resource_identity::ResourceIdentityIndex;
use crate::template_expr_cache::parse_expr_text;
use crate::{ResourceRef, ValueKind, YamlPath};

mod attribution;
mod fragment_indent;
mod source_position;
mod state;
mod text_cursor;
mod yaml_tree;

use attribution::{
    AttributionIndex, ResolvedNodeContext, build_attribution_index, is_output_root_kind,
};
use fragment_indent::fragment_indent_width_from_exprs;
use source_position::{line_indent_and_col, starts_template_action_line};
use state::DocumentState;
use text_cursor::TemplateTextCursor;

/// Tracks document-local path and resource attribution while the symbolic
/// interpreter walks mixed YAML and Helm actions.
pub(crate) struct DocumentTracker<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    resource_identity: ResourceIdentityIndex,
    attribution: AttributionIndex,
    current_context: ResolvedNodeContext,
    document_state: DocumentState,
    text_cursor: TemplateTextCursor,
}

impl<'a> DocumentTracker<'a> {
    pub(crate) fn new(source: &'a str, defines: &'a DefineIndex) -> Self {
        Self {
            source,
            defines,
            resource_identity: ResourceIdentityIndex::default(),
            attribution: AttributionIndex::default(),
            current_context: ResolvedNodeContext::default(),
            document_state: DocumentState::default(),
            text_cursor: TemplateTextCursor::default(),
        }
    }

    pub(crate) fn reset_for_tree(&mut self, tree: &tree_sitter::Tree) {
        self.resource_identity = ResourceIdentityIndex::from_source(self.source, self.defines);
        self.attribution = build_attribution_index(self.source, tree.root_node());
        self.current_context = ResolvedNodeContext::default();
        self.text_cursor.reset_for_tree(tree);
        self.document_state = DocumentState::default();
    }

    pub(crate) fn enter_node(&mut self, node: tree_sitter::Node<'_>) {
        self.ingest_text_up_to(node.start_byte());
        self.resource_identity.advance_to(node.start_byte());
        self.current_context = if is_output_root_kind(node.kind()) {
            self.attribution
                .output_context_for_node(node)
                .unwrap_or_default()
        } else if matches!(node.kind(), "if_action" | "with_action" | "range_action") {
            self.attribution
                .control_context_for_node(node)
                .unwrap_or_default()
        } else {
            ResolvedNodeContext::default()
        };
        self.sync_action_for_node(node);
        self.suppress_current_context_inside_block_scalar(node.start_byte());
    }

    pub(crate) fn current_path(&self) -> YamlPath {
        if self.current_context.inside_block_scalar {
            return YamlPath(Vec::new());
        }

        let state_path = self.document_state.current_path();
        if !state_path.0.is_empty() {
            state_path
        } else {
            self.current_context.current_path.clone()
        }
    }

    pub(crate) fn path_at_mapping_entry_indent(&self, indent: usize) -> YamlPath {
        if self.current_context.inside_block_scalar {
            return YamlPath(Vec::new());
        }

        let state_path = self.document_state.path_at_mapping_entry_indent(indent);
        if !state_path.0.is_empty() {
            state_path
        } else {
            self.current_context.mapping_entry_path.clone()
        }
    }

    pub(crate) fn current_resource(&self) -> Option<&ResourceRef> {
        self.resource_identity.current_resource()
    }

    pub(crate) fn ingest_text_up_to(&mut self, target: usize) {
        self.text_cursor
            .ingest_text_up_to(self.source, &mut self.document_state, target);
    }

    pub(crate) fn rebase_path(&self, path: YamlPath) -> YamlPath {
        self.resource_identity.rebase_path(path)
    }

    pub(crate) fn output_inside_block_scalar_at(&self, byte_pos: usize) -> bool {
        let (indent, _) = self.line_indent_and_col(byte_pos);
        self.current_context.inside_block_scalar
            || self.document_state.is_inside_block_scalar_line(indent)
    }

    pub(crate) fn output_in_mapping_key(&self) -> bool {
        self.current_context.in_mapping_key
    }

    pub(crate) fn output_entire_scalar_value(&self) -> bool {
        self.current_context.entire_scalar_value
    }

    pub(crate) fn output_site_path(
        &self,
        node: tree_sitter::Node<'_>,
        kind: ValueKind,
        fragment_indent_width: Option<usize>,
    ) -> YamlPath {
        if self.current_context.inside_block_scalar {
            return YamlPath(Vec::new());
        }

        if self.output_inside_block_scalar_at(node.start_byte()) {
            return YamlPath(Vec::new());
        }

        let (physical_indent, _physical_col) = self.line_indent_and_col(node.start_byte());
        let state_path = if self.starts_template_action_line(node.start_byte()) {
            let logical_indent = fragment_indent_width.unwrap_or(physical_indent);
            self.document_state
                .path_at_mapping_entry_indent(logical_indent)
        } else {
            self.document_state.current_path()
        };

        let context_path = self.current_context.output_path.clone();
        let mut path = if kind == ValueKind::Fragment
            && fragment_state_path_is_more_specific(&context_path, &state_path)
        {
            state_path
        } else {
            preferred_output_path(context_path, state_path)
        };
        if kind == ValueKind::Fragment {
            if let Some(last) = path.0.last_mut()
                && let Some(stripped) = last.strip_suffix("[*]")
            {
                *last = stripped.to_string();
            }
        }
        path
    }

    pub(crate) fn line_indent_and_col(&self, byte_pos: usize) -> (usize, usize) {
        line_indent_and_col(self.source, byte_pos)
    }

    pub(crate) fn starts_template_action_line(&self, byte_pos: usize) -> bool {
        starts_template_action_line(self.source, byte_pos)
    }

    pub(crate) fn fragment_indent_width_for_exprs(exprs: &[TemplateExpr]) -> Option<usize> {
        fragment_indent_width_from_exprs(exprs)
    }

    fn sync_action_for_node(&mut self, node: tree_sitter::Node<'_>) {
        #[derive(Clone, Copy)]
        struct TemplateActionAnalysis {
            is_fragment: bool,
            virtual_indent: Option<usize>,
        }

        if matches!(node.kind(), "text" | "yaml_no_injection_text") {
            return;
        }

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
                self.document_state.ingest(&sanitized);
                self.text_cursor.set_position(pos);
            }
        }

        let (physical_indent, physical_col) = self.line_indent_and_col(pos);

        let template_action_shape = if node.kind() == "template_action" {
            node.utf8_text(self.source.as_bytes())
                .ok()
                .map(parse_expr_text)
                .map(|exprs| TemplateActionAnalysis {
                    is_fragment: is_fragment_exprs(&exprs),
                    virtual_indent: fragment_indent_width_from_exprs(&exprs),
                })
        } else {
            None
        };
        let allow_clear_pending = template_action_shape
            .as_ref()
            .is_none_or(|shape| !shape.is_fragment);

        let (indent, col) = if let Some(virtual_indent) = template_action_shape
            .and_then(|shape| {
                (!allow_clear_pending)
                    .then_some(shape.virtual_indent)
                    .flatten()
            })
            .filter(|virtual_indent| *virtual_indent > physical_indent)
        {
            (virtual_indent, virtual_indent)
        } else {
            (physical_indent, physical_col)
        };

        self.document_state
            .sync_action_position(indent, col, allow_clear_pending);
    }

    fn suppress_current_context_inside_block_scalar(&mut self, byte_pos: usize) {
        let (indent, _) = self.line_indent_and_col(byte_pos);
        if !(self.current_context.inside_block_scalar
            || self.document_state.is_inside_block_scalar_line(indent))
        {
            return;
        }

        self.current_context.current_path = YamlPath(Vec::new());
        self.current_context.output_path = YamlPath(Vec::new());
        self.current_context.mapping_entry_path = YamlPath(Vec::new());
        self.current_context.in_mapping_key = false;
        self.current_context.entire_scalar_value = false;
        self.current_context.inside_block_scalar = true;
    }
}

fn preferred_output_path(context_path: YamlPath, state_path: YamlPath) -> YamlPath {
    match (context_path.0.is_empty(), state_path.0.is_empty()) {
        (true, true) => YamlPath(Vec::new()),
        (true, false) => state_path,
        (false, true) => context_path,
        (false, false) => {
            if path_has_equivalent_suffix(&state_path.0, &context_path.0)
                && state_path.0.len() > context_path.0.len()
            {
                state_path
            } else if path_has_equivalent_suffix(&context_path.0, &state_path.0)
                && context_path.0.len() > state_path.0.len()
            {
                context_path
            } else {
                context_path
            }
        }
    }
}

fn path_has_equivalent_suffix(path: &[String], suffix: &[String]) -> bool {
    if suffix.len() > path.len() {
        return false;
    }
    path[path.len() - suffix.len()..]
        .iter()
        .zip(suffix)
        .all(|(left, right)| path_segments_equivalent(left, right))
}

fn path_segments_equivalent(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_suffix("[*]")
            .is_some_and(|stripped| stripped == right)
        || right
            .strip_suffix("[*]")
            .is_some_and(|stripped| stripped == left)
}

fn fragment_state_path_is_more_specific(context_path: &YamlPath, state_path: &YamlPath) -> bool {
    !context_path.0.is_empty()
        && state_path.0.len() > context_path.0.len()
        && context_path.0.iter().all(|context_segment| {
            state_path
                .0
                .iter()
                .any(|state_segment| path_segments_equivalent(state_segment, context_segment))
        })
}

#[cfg(test)]
mod tests {
    use helm_schema_ast::DefineIndex;

    use crate::ValueKind;

    use super::DocumentTracker;
    use super::attribution::{build_attribution_index, is_output_root_kind};

    fn parse_template(source: &str) -> tree_sitter::Tree {
        let language =
            tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .expect("go-template grammar should load");
        parser.parse(source, None).expect("template should parse")
    }

    fn output_nodes_containing<'tree>(
        node: tree_sitter::Node<'tree>,
        source: &str,
        needle: &str,
        out: &mut Vec<tree_sitter::Node<'tree>>,
    ) {
        if is_output_root_kind(node.kind())
            && node
                .utf8_text(source.as_bytes())
                .is_ok_and(|text| text.contains(needle))
        {
            out.push(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            output_nodes_containing(child, source, needle, out);
        }
    }

    fn output_nodes_with_exact_text<'tree>(
        node: tree_sitter::Node<'tree>,
        source: &str,
        needle: &str,
        out: &mut Vec<tree_sitter::Node<'tree>>,
    ) {
        if is_output_root_kind(node.kind())
            && node
                .utf8_text(source.as_bytes())
                .is_ok_and(|text| text.trim() == needle)
        {
            out.push(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            output_nodes_with_exact_text(child, source, needle, out);
        }
    }

    #[test]
    fn attribution_uses_mapping_key_for_flow_sequence_scalar() {
        let source = r#"livenessProbe:
  exec:
    command: ['/bin/bash', '-c', 'echo "ruok" | timeout {{ .Values.timeout }} nc -w {{ .Values.timeout }} localhost {{ .Values.port }} | grep imok']
"#;
        let tree = parse_template(source);
        let attribution = build_attribution_index(source, tree.root_node());
        let mut nodes = Vec::new();
        output_nodes_containing(tree.root_node(), source, ".Values.timeout", &mut nodes);
        assert!(!nodes.is_empty());

        for node in nodes {
            let context = attribution
                .output_context_for_node(node)
                .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
            assert_eq!(
                context.output_path.0,
                vec!["livenessProbe", "exec", "command"],
                "node kind {}",
                node.kind()
            );
        }
    }

    #[test]
    fn tracker_keeps_outer_prefix_for_fragment_inside_with_body() {
        let source =
            include_str!("../../../../../testdata/charts/cert-manager/templates/deployment.yaml");
        let tree = parse_template(source);
        let defines = DefineIndex::new();
        let mut tracker = DocumentTracker::new(source, &defines);
        tracker.reset_for_tree(&tree);

        let mut actions = Vec::new();
        output_nodes_containing(tree.root_node(), source, "toYaml", &mut actions);
        let action = actions
            .into_iter()
            .find(|node| {
                node.utf8_text(source.as_bytes()).is_ok_and(|text| {
                    text.contains("nindent 8")
                        && source[..node.start_byte()].contains("with .Values.volumes")
                })
            })
            .expect("fragment action");
        drive_tracker_until(&mut tracker, tree.root_node(), action);

        let path = tracker.output_site_path(action, ValueKind::Fragment, Some(8));
        assert_eq!(
            path.0,
            vec!["spec", "template", "spec", "volumes"],
            "current={:?} mapping={:?} context={:?}",
            tracker.current_path().0,
            tracker.path_at_mapping_entry_indent(8).0,
            tracker.current_context.output_path.0,
        );
    }

    #[test]
    fn attribution_marks_mapping_value_action_as_entire_scalar() {
        let source = r#"env:
  - name: HTTP_PROXY
    value: {{ .Values.http_proxy }}
"#;
        let tree = parse_template(source);
        let attribution = build_attribution_index(source, tree.root_node());
        let mut nodes = Vec::new();
        output_nodes_containing(tree.root_node(), source, ".Values.http_proxy", &mut nodes);
        assert!(!nodes.is_empty());

        for node in nodes {
            let context = attribution
                .output_context_for_node(node)
                .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
            assert_eq!(
                context.output_path.0,
                vec!["env[*]", "value"],
                "node kind {}",
                node.kind()
            );
            assert!(
                context.entire_scalar_value,
                "node kind {} should be the entire scalar value",
                node.kind()
            );
        }
    }

    #[test]
    fn attribution_marks_inline_sequence_mapping_value_action_as_entire_scalar() {
        let source = r#"ports:
  - port: {{ .Values.port }}
"#;
        let tree = parse_template(source);
        let attribution = build_attribution_index(source, tree.root_node());
        let mut nodes = Vec::new();
        output_nodes_containing(tree.root_node(), source, ".Values.port", &mut nodes);
        assert!(!nodes.is_empty());

        for node in nodes {
            let context = attribution
                .output_context_for_node(node)
                .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
            assert_eq!(
                context.output_path.0,
                vec!["ports[*]", "port"],
                "node kind {}",
                node.kind()
            );
            assert!(
                context.entire_scalar_value,
                "node kind {} should be the entire scalar value",
                node.kind()
            );
        }
    }

    #[test]
    fn tracker_preserves_entire_scalar_for_inline_sequence_mapping_action() {
        let source = r#"ports:
  - port: {{ .Values.port }}
"#;
        let tree = parse_template(source);
        let defines = DefineIndex::new();
        let mut tracker = DocumentTracker::new(source, &defines);
        tracker.reset_for_tree(&tree);

        let mut actions = Vec::new();
        output_nodes_containing(tree.root_node(), source, ".Values.port", &mut actions);
        let action = actions.into_iter().next().expect("output action");
        drive_tracker_until(&mut tracker, tree.root_node(), action);

        assert_eq!(
            tracker.output_site_path(action, ValueKind::Scalar, None).0,
            vec!["ports[*]", "port"]
        );
        assert!(tracker.output_entire_scalar_value());
    }

    #[test]
    fn tracker_preserves_entire_scalar_for_inline_sequence_mapping_action_in_control_body() {
        let source = r#"{{- if .Values.metrics.enabled }}
ports:
  - port: {{ .Values.metrics.containerPorts.http }}
{{- end }}
"#;
        let tree = parse_template(source);
        let defines = DefineIndex::new();
        let mut tracker = DocumentTracker::new(source, &defines);
        tracker.reset_for_tree(&tree);

        let mut actions = Vec::new();
        output_nodes_containing(
            tree.root_node(),
            source,
            ".Values.metrics.containerPorts.http",
            &mut actions,
        );
        let action = actions.into_iter().next().expect("output action");
        drive_tracker_until(&mut tracker, tree.root_node(), action);

        assert_eq!(
            tracker.output_site_path(action, ValueKind::Scalar, None).0,
            vec!["ports[*]", "port"]
        );
        assert!(tracker.output_entire_scalar_value());
    }

    #[test]
    fn tracker_preserves_entire_scalar_for_bitnami_metrics_port_after_nested_blocks() {
        let source = include_str!(
            "../../../../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
        );
        let tree = parse_template(source);
        let defines = DefineIndex::new();
        let mut tracker = DocumentTracker::new(source, &defines);
        tracker.reset_for_tree(&tree);

        let mut actions = Vec::new();
        output_nodes_containing(
            tree.root_node(),
            source,
            ".Values.metrics.containerPorts.http",
            &mut actions,
        );
        let action = actions
            .into_iter()
            .next()
            .expect("metrics port output action");
        drive_tracker_until(&mut tracker, tree.root_node(), action);

        assert_eq!(
            tracker.output_site_path(action, ValueKind::Scalar, None).0,
            vec!["spec", "ingress[*]", "ports[*]", "port"]
        );
        assert!(
            tracker.output_entire_scalar_value(),
            "current={:?} context={:?}",
            tracker.current_path().0,
            tracker.current_context.output_path.0
        );
    }

    #[test]
    fn tracker_keeps_script_block_scalar_outputs_pathless() {
        let source = r#"args:
  - -ec
  - |
    chown -R {{ .Values.podSecurityContext.fsGroup }} /data
    {{- if .Values.dataLogDir }}
    mkdir -p {{ .Values.dataLogDir }}
    {{- end }}
"#;
        let tree = parse_template(source);
        let defines = DefineIndex::new();
        let mut tracker = DocumentTracker::new(source, &defines);
        tracker.reset_for_tree(&tree);

        let mut actions = Vec::new();
        output_nodes_containing(
            tree.root_node(),
            source,
            ".Values.podSecurityContext.fsGroup",
            &mut actions,
        );
        let action = actions.into_iter().next().expect("script output action");
        drive_tracker_until(&mut tracker, tree.root_node(), action);

        assert!(tracker.output_inside_block_scalar_at(action.start_byte()));
        assert_eq!(
            tracker.output_site_path(action, ValueKind::Scalar, None).0,
            Vec::<String>::new()
        );
        assert!(!tracker.output_entire_scalar_value());
    }

    #[test]
    fn attribution_marks_with_bound_dot_action_as_entire_scalar() {
        let source = r#"env:
  {{- with .Values.http_proxy }}
  - name: HTTP_PROXY
    value: {{ . }}
  {{- end }}
"#;
        let tree = parse_template(source);
        let attribution = build_attribution_index(source, tree.root_node());
        let mut nodes = Vec::new();
        output_nodes_with_exact_text(tree.root_node(), source, ".", &mut nodes);
        assert!(!nodes.is_empty());

        for node in nodes {
            let context = attribution
                .output_context_for_node(node)
                .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
            assert_eq!(
                context.output_path.0,
                vec!["env[*]", "value"],
                "node kind {}",
                node.kind()
            );
            assert!(
                context.entire_scalar_value,
                "node kind {} should be the entire scalar value",
                node.kind()
            );
        }
    }

    #[test]
    fn attribution_marks_embedded_sequence_value_action_as_partial_scalar() {
        let source = r#"args:
  - --v={{ .Values.global.logLevel }}
"#;
        let tree = parse_template(source);
        let attribution = build_attribution_index(source, tree.root_node());
        let mut nodes = Vec::new();
        output_nodes_containing(
            tree.root_node(),
            source,
            ".Values.global.logLevel",
            &mut nodes,
        );
        assert!(!nodes.is_empty());

        for node in nodes {
            let context = attribution
                .output_context_for_node(node)
                .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
            assert_eq!(
                context.output_path.0,
                vec!["args[*]"],
                "node kind {}",
                node.kind()
            );
            assert!(
                !context.entire_scalar_value,
                "node kind {} should be embedded in the scalar value",
                node.kind()
            );
        }
    }

    fn drive_tracker_until(
        tracker: &mut DocumentTracker<'_>,
        node: tree_sitter::Node<'_>,
        target: tree_sitter::Node<'_>,
    ) -> bool {
        tracker.enter_node(node);
        if matches!(node.kind(), "text" | "yaml_no_injection_text") {
            tracker.ingest_text_up_to(node.end_byte());
        }
        if node.id() == target.id() {
            return true;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if drive_tracker_until(tracker, child, target) {
                return true;
            }
        }
        false
    }
}
