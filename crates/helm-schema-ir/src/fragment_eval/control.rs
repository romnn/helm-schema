//! Control-region evaluation: each branch's contributions are evaluated
//! under the branch's decoded condition (plus the negations of prior arms)
//! and dissolve into the surrounding container as guarded arms. Local
//! bindings join across branches with the same rules as the symbolic
//! walker.

use helm_schema_ast::TemplateHeader;
use helm_schema_syntax::{ControlKind, ControlRegion, Node, ScalarPart};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::parse_literal_list_range_expr;
use crate::{Guard, ValueKind};
use helm_schema_core::Predicate;

use super::domain::{AbstractFragment, PathCondition, Splice, SpliceMeta, and_conditions};
use super::eval::{Adopted, ArmSpec, Contributions, Interpreter, NodeView};

impl Interpreter<'_> {
    pub(super) fn eval_control(
        &mut self,
        region: &ControlRegion,
        adopted: &[Adopted<'_>],
    ) -> Contributions {
        if matches!(region.kind, ControlKind::Define | ControlKind::Block) {
            // Define/block bodies render nothing at document scope (they are
            // evaluated when included); escaped siblings adopted into the
            // region are part of that suppressed body.
            return Contributions::default();
        }
        let branch_nodes = branch_node_lists(region, adopted);

        let entry_locals = self.locals.clone();
        let entry_predicates = self.active_predicates.len();
        let entry_dots = self.dot_stack.len();

        let mut out = Contributions::default();
        let mut outcomes = Vec::new();
        let mut prior_conditions: Vec<PathCondition> = Vec::new();
        let mut has_unconditional_else = false;

        for (index, _branch) in region.branches.iter().enumerate() {
            self.locals = entry_locals.clone();
            self.active_predicates.truncate(entry_predicates);
            self.dot_stack.truncate(entry_dots);

            let mut arm_condition = Predicate::True;
            // Later arms run under the negations of every earlier decoded
            // condition (range alternatives decode no condition, so they
            // stay unguarded like the current pipeline's range joins).
            for prior in &prior_conditions {
                let negated = prior.negated();
                self.push_predicate(negated.clone());
                arm_condition = and_conditions(arm_condition, negated);
            }

            let arm = self.classify_branch(region, index);
            if matches!(arm, ArmSpec::Else) && index > 0 {
                has_unconditional_else = true;
            }
            let nodes = branch_nodes.get(index).map_or(&[][..], Vec::as_slice);
            let (own_condition, extra) = self.activate_arm(&arm, nodes);
            if let Some(own) = own_condition {
                arm_condition = and_conditions(arm_condition, own.clone());
                if !matches!(arm, ArmSpec::Range { .. }) {
                    prior_conditions.push(own);
                }
            }

            self.locals.enter_local_scope();
            let mut contributions = self.eval_node_list(nodes);
            self.locals.exit_local_scope();
            outcomes.push(self.locals.clone());

            contributions.extend(extra);
            contributions.guard_all(&arm_condition);
            out.extend(contributions);
        }

        self.locals = entry_locals.clone();
        self.active_predicates.truncate(entry_predicates);
        self.dot_stack.truncate(entry_dots);
        if !has_unconditional_else {
            outcomes.push(entry_locals.clone());
        }
        self.locals.join_branch_outcomes(&entry_locals, outcomes);

        // Descendants of adopted nodes that start after the region end
        // evaluate here, outside the branch scope, re-attached under their
        // parent entry chain (source order: they follow `{{ end }}`).
        let mut deferred_specs = Vec::new();
        for entry in adopted {
            let mut chain = Vec::new();
            collect_deferred(
                entry.view.node,
                region.span.end,
                entry.defer_upper,
                &mut chain,
                &mut deferred_specs,
            );
        }
        for spec in deferred_specs {
            self.eval_deferred(spec, &mut out);
        }
        out
    }

    /// Evaluate one deferred descendant batch and re-attach it under its
    /// parent entry chain, letting explicitly-indented output keep floating
    /// past entries it does not render inside.
    fn eval_deferred(&mut self, spec: DeferredNodes<'_>, out: &mut Contributions) {
        let views: Vec<NodeView<'_>> = spec
            .nodes
            .iter()
            .map(|node| NodeView::plain(node))
            .collect();
        let mut contributions = self.eval_node_list(&views);
        let mut chain = spec.chain.iter().rev();
        let Some(innermost) = chain.next() else {
            out.extend(contributions);
            return;
        };
        let opened_empty = innermost.value.is_none() && innermost.block.is_none();
        let mut value = contributions.take_floating_below(innermost.indent, opened_empty, None);
        let mut floating = std::mem::take(&mut contributions.floating);
        value.extend(contributions.assemble());
        let mut key = self.entry_key(&innermost.key);
        for parent in chain {
            let mut wrapper = Contributions::default();
            wrapper.merge_entry(key, value);
            wrapper.floating = floating;
            let opened_empty = parent.value.is_none() && parent.block.is_none();
            let mut parent_value = wrapper.take_floating_below(parent.indent, opened_empty, None);
            floating = std::mem::take(&mut wrapper.floating);
            parent_value.extend(wrapper.assemble());
            value = parent_value;
            key = self.entry_key(&parent.key);
        }
        let mut re_attached = Contributions::default();
        re_attached.merge_entry(key, value);
        re_attached.floating = floating;
        out.extend(re_attached);
    }

    fn classify_branch(&self, region: &ControlRegion, index: usize) -> ArmSpec {
        if index == 0 {
            let facts = self.control_facts.get(&region.span.start);
            return match region.kind {
                ControlKind::If => ArmSpec::If(facts.and_then(|facts| facts.header.clone())),
                ControlKind::With => ArmSpec::With(facts.and_then(|facts| facts.header.clone())),
                ControlKind::Range => ArmSpec::Range {
                    header: facts.and_then(|facts| facts.header.clone()),
                    destructured: facts.is_some_and(|facts| facts.range_destructured),
                },
                ControlKind::Define | ControlKind::Block => ArmSpec::Else,
            };
        }
        let header_text = region
            .branches
            .get(index)
            .map_or("", |branch| self.text(branch.header));
        parse_else_header(header_text)
    }

    /// Activate one arm: decode its condition, record the condition reads
    /// the current pipeline also records, install dot bindings and range
    /// domains, and return the arm's own condition plus any structural
    /// contributions the arm itself renders (range headers that render list
    /// or mapping content).
    fn activate_arm(
        &mut self,
        arm: &ArmSpec,
        nodes: &[NodeView<'_>],
    ) -> (Option<PathCondition>, Contributions) {
        match arm {
            ArmSpec::Else => (None, Contributions::default()),
            ArmSpec::If(header) => (self.activate_if(header.as_ref()), Contributions::default()),
            ArmSpec::With(header) => (
                self.activate_with(header.as_ref()),
                Contributions::default(),
            ),
            ArmSpec::Range {
                header,
                destructured,
            } => self.activate_range(header.as_ref(), *destructured, nodes),
        }
    }

    fn activate_if(&mut self, header: Option<&TemplateHeader>) -> Option<PathCondition> {
        let header = header?;
        let (predicate, bound_values) = {
            let context = self.value_path_context();
            (
                context.condition_predicate_expr(header.expr()),
                context.bound_output_paths_expr(header.expr()),
            )
        };
        for path in &bound_values {
            self.push_read(path, &[]);
        }
        let guards = predicate.contract_guards();
        for guard in &guards {
            for path in guard.value_paths() {
                self.push_read(path, std::slice::from_ref(guard));
            }
            self.push_predicate(Predicate::from(guard.clone()));
        }
        if guards.is_empty() {
            self.push_predicate(predicate.clone());
        }
        Some(predicate)
    }

    fn activate_with(&mut self, header: Option<&TemplateHeader>) -> Option<PathCondition> {
        let Some(header) = header else {
            self.dot_stack.push(None);
            return None;
        };
        let (predicate, bound_values, dot) = {
            let context = self.value_path_context();
            (
                context.with_condition_predicate_expr(header.expr()),
                context.bound_output_paths_expr(header.expr()),
                context.with_body_fragment_value_expr(header.expr()),
            )
        };
        // The with-predicate is pushed before its reads so the reads carry
        // the `Guard::With` markers, mirroring the current walker.
        let guards = predicate.contract_guards();
        for guard in &guards {
            self.push_predicate(Predicate::from(guard.clone()));
        }
        if guards.is_empty() {
            self.push_predicate(predicate.clone());
        }
        for path in &bound_values {
            self.push_read(path, &[]);
        }
        for guard in &guards {
            for path in guard.value_paths() {
                self.push_read(path, &[]);
            }
        }
        self.dot_stack.push(dot);
        Some(predicate)
    }

    fn activate_range(
        &mut self,
        header: Option<&TemplateHeader>,
        destructured: bool,
        nodes: &[NodeView<'_>],
    ) -> (Option<PathCondition>, Contributions) {
        let Some(header) = header else {
            self.dot_stack.push(None);
            return (None, Contributions::default());
        };
        if let Some((variable, literals)) = parse_literal_list_range_expr(header.expr()) {
            self.locals.insert_range_domain(variable, literals);
        }
        let (source_paths, direct_path) = {
            let context = self.value_path_context();
            (
                context
                    .resolved_values_paths_from_expr(header.expr())
                    .into_iter()
                    .collect::<Vec<_>>(),
                context.single_direct_iterable_range_path_expr(header.expr()),
            )
        };
        let shape = self.range_body_shape(nodes);
        let renders_scalar_items =
            shape.emits_sequence_items && shape.items_all_scalar && direct_path.is_some();
        let emit_header_read = destructured || !shape.emits_sequence_items || renders_scalar_items;
        let renders_mapping_entries =
            destructured && !shape.emits_sequence_items && shape.has_dynamic_entries;

        let mut own = Vec::new();
        let mut extra = Contributions::default();
        for path in &source_paths {
            let guard = Guard::Range { path: path.clone() };
            if emit_header_read && !renders_scalar_items {
                self.push_read(path, std::slice::from_ref(&guard));
            }
            own.push(Predicate::from(guard.clone()));
            self.push_predicate(Predicate::from(guard));
        }
        if renders_scalar_items {
            for path in &source_paths {
                extra.push_value_arm(splice_arm(path, ValueKind::Scalar));
            }
        }
        if renders_mapping_entries {
            for path in &source_paths {
                extra.push_value_arm(splice_arm(path, ValueKind::Fragment));
            }
        }
        let dot = direct_path.map(|path| AbstractValue::ValuesPath(format!("{path}.*")));
        self.dot_stack.push(dot);
        (Some(Predicate::all(own)), extra)
    }

    fn range_body_shape(&mut self, nodes: &[NodeView<'_>]) -> RangeBodyShape {
        let mut shape = RangeBodyShape {
            emits_sequence_items: false,
            items_all_scalar: true,
            has_dynamic_entries: false,
        };
        for view in nodes {
            self.observe_range_body_node(view.node, &mut shape);
        }
        shape
    }

    fn observe_range_body_node(&mut self, node: &Node, shape: &mut RangeBodyShape) {
        match node {
            Node::Sequence(item) => {
                shape.emits_sequence_items = true;
                // Mirrors the scalar-sequence-items rule: only items whose
                // whole content is a non-fragment scalar count (a nested
                // mapping entry, a bare dash, or a fragment-rendering hole
                // disqualifies the body).
                let scalar_item = item.children.is_empty()
                    && (item.block.is_some()
                        || item
                            .value
                            .as_ref()
                            .is_some_and(|value| !self.scalar_parts_render_fragment(value)));
                if !scalar_item {
                    shape.items_all_scalar = false;
                }
                for child in &item.children {
                    self.observe_range_body_node(child, shape);
                }
            }
            Node::Mapping(entry) => {
                if entry
                    .key
                    .parts
                    .iter()
                    .any(|part| matches!(part, ScalarPart::Hole(_)))
                {
                    shape.has_dynamic_entries = true;
                }
                for child in &entry.children {
                    self.observe_range_body_node(child, shape);
                }
            }
            Node::Control(region) => {
                for branch in &region.branches {
                    for child in &branch.body {
                        self.observe_range_body_node(child, shape);
                    }
                }
            }
            Node::Opaque(opaque)
                if opaque.kind == helm_schema_syntax::OpaqueKind::ActionLineText
                    && helm_schema_syntax::structural_mapping_colon(self.text(opaque.span))
                        .is_some() =>
            {
                // `{{ key }}…: value` line shape: a templated mapping entry.
                shape.has_dynamic_entries = true;
            }
            _ => {}
        }
    }
}

fn splice_arm(path: &str, kind: ValueKind) -> (PathCondition, AbstractFragment) {
    (
        Predicate::True,
        AbstractFragment::Splice(Splice {
            values_path: path.to_string(),
            kind,
            meta: SpliceMeta::default(),
        }),
    )
}

/// Assign each region branch its body nodes plus the adopted escaped
/// siblings whose spans fall into the branch's source window.
fn branch_node_lists<'nodes>(
    region: &'nodes ControlRegion,
    adopted: &[Adopted<'nodes>],
) -> Vec<Vec<NodeView<'nodes>>> {
    let mut lists: Vec<Vec<NodeView<'nodes>>> = region
        .branches
        .iter()
        .map(|branch| branch.body.iter().map(NodeView::plain).collect())
        .collect();
    for entry in adopted {
        let start = entry.view.node.span_start();
        let mut target = 0;
        for (index, branch) in region.branches.iter().enumerate() {
            if start >= branch.header.end {
                target = index;
            }
        }
        if let Some(list) = lists.get_mut(target) {
            list.push(entry.view);
        }
    }
    for list in &mut lists {
        list.sort_by_key(|view| view.node.span_start());
    }
    lists
}

/// Classify an `{{ else … }}` branch header. The header span is a single
/// isolated action token, so keyword classification here is a narrow local
/// check; condition text still goes through the typed header parser.
fn parse_else_header(text: &str) -> ArmSpec {
    let mut inner = text.trim();
    if let Some(rest) = inner.strip_prefix("{{") {
        inner = rest.trim_start_matches('-').trim();
    }
    if let Some(rest) = inner.strip_suffix("}}") {
        inner = rest.trim_end_matches('-').trim();
    }
    if inner == "else" {
        return ArmSpec::Else;
    }
    let Some(rest) = inner.strip_prefix("else") else {
        return ArmSpec::Else;
    };
    let rest = rest.trim_start();
    if let Some(condition) = rest.strip_prefix("if ") {
        return ArmSpec::If(Some(TemplateHeader::parse_control(format!(
            "if {condition}"
        ))));
    }
    if let Some(condition) = rest.strip_prefix("with ") {
        return ArmSpec::With(Some(TemplateHeader::parse_control(format!(
            "with {condition}"
        ))));
    }
    ArmSpec::Else
}

/// One batch of deferred descendants: the entry chain they nest under (in
/// document order, outermost first) and the deferred nodes themselves.
struct DeferredNodes<'n> {
    chain: Vec<&'n helm_schema_syntax::MappingEntry>,
    nodes: Vec<&'n Node>,
}

/// Collect descendants of an adopted node whose spans start at or beyond the
/// adopting region's end (and below the enclosing bound, which the enclosing
/// region's own deferral handles), with the mapping-entry chain above them.
fn collect_deferred<'n>(
    node: &'n Node,
    limit: usize,
    upper: Option<usize>,
    chain: &mut Vec<&'n helm_schema_syntax::MappingEntry>,
    out: &mut Vec<DeferredNodes<'n>>,
) {
    let in_window = |child: &Node| {
        let start = child.span_start();
        start >= limit && upper.is_none_or(|upper| start < upper)
    };
    match node {
        Node::Mapping(entry) => {
            chain.push(entry);
            let beyond: Vec<&Node> = entry.children.iter().filter(|c| in_window(c)).collect();
            if !beyond.is_empty() {
                out.push(DeferredNodes {
                    chain: chain.clone(),
                    nodes: beyond,
                });
            }
            for child in &entry.children {
                if child.span_start() < limit {
                    collect_deferred(child, limit, upper, chain, out);
                }
            }
            chain.pop();
        }
        Node::Sequence(item) => {
            let beyond: Vec<&Node> = item.children.iter().filter(|c| in_window(c)).collect();
            if !beyond.is_empty() {
                out.push(DeferredNodes {
                    chain: chain.clone(),
                    nodes: beyond,
                });
            }
            for child in &item.children {
                if child.span_start() < limit {
                    collect_deferred(child, limit, upper, chain, out);
                }
            }
        }
        Node::Control(region) => {
            for branch in &region.branches {
                for child in &branch.body {
                    collect_deferred(child, limit, upper, chain, out);
                }
            }
        }
        _ => {}
    }
}

/// Structural range-body shape read off the CST (replacing the line scans
/// the template-tree pipeline uses for the same decisions).
struct RangeBodyShape {
    emits_sequence_items: bool,
    items_all_scalar: bool,
    has_dynamic_entries: bool,
}
