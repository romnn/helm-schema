pub mod analyze {
    // // (4) functions that usually emit YAML fragments
    // if snippet.contains("toYaml") || snippet.contains("include ") || snippet.contains("tpl ") {
    //     // could still be scalar, but often multi-line; keep as fragment for now
    //     return Role::Fragment;
    // }

    // // Collect .Values paths + *which action child* they live under
    // // (we do one more pass to map each selector/index occurrence to its top-level action child)
    // let mut occurrences: Vec<(String, Node)> = Vec::new();
    // {
    //     let mut stack = vec![root];
    //     while let Some(node) = stack.pop() {
    //         if node.kind() == "selector_expression" || node.kind() == "function_call" {
    //             // Reuse template extractor to decide if this node contributes a .Values path
    //             if node.kind() == "selector_expression" {
    //                 if let Some(segs) =
    //                     helm_schema_template::values::parse_selector_expression(&node, &source)
    //                 {
    //                     let full = segs.join(".");
    //                     if let Some(owner) = owning_action_child(tree, node.byte_range()) {
    //                         occurrences.push((full, owner));
    //                     }
    //                 }
    //             } else if node.kind() == "function_call" {
    //                 if let Some(segs) =
    //                     helm_schema_template::values::parse_index_call(&node, &source)
    //                 {
    //                     let full = segs.join(".");
    //                     if let Some(owner) = owning_action_child(tree, node.byte_range()) {
    //                         occurrences.push((full, owner));
    //                     }
    //                 }
    //             }
    //         }
    //         let mut c = node.walk();
    //         for ch in node.children(&mut c) {
    //             if ch.is_named() {
    //                 stack.push(ch);
    //             }
    //         }
    //     }
    // }
    //
    // // Group by owning action
    // let mut per_action: BTreeMap<usize, (Node, BTreeSet<String>)> = BTreeMap::new();
    // for (vp, act) in occurrences {
    //     per_action
    //         .entry(act.id())
    //         .or_insert_with(|| (act, BTreeSet::new()))
    //         .1
    //         .insert(vp);
    // }
    //
    // // (2) Recursively sanitize: insert placeholders at *any* action in value position,
    // //     and attach .Values paths collected from that action's subtree.
    // let mut placeholders: Vec<crate::sanitize::Placeholder> = Vec::new();
    // let sanitized = crate::sanitize::build_sanitized_with_placeholders(
    //     &source,
    //     tree,
    //     &mut placeholders,
    //     // |src, act| classify_action(&source, &act.byte_range()),
    //     |act| collect_values_in_subtree(act, &source),
    // );
    //
    // println!("SANITIZED YAML:\n\n{sanitized}");
    //
    // // Let YAML AST decide roles and paths
    // let bindings = compute_yaml_paths_for_placeholders(&sanitized).map_err(|_| Error::YamlParse)?;
    //
    // // // (4) parse YAML and map placeholders to YAML paths
    // // let ph_to_yaml =
    // //     compute_yaml_paths_for_placeholders(&sanitized).map_err(|_| Error::YamlParse)?;
    //
    // let mut out = Vec::new();
    // for ph in placeholders {
    //     let (role, path) = if let Some(b) = bindings.get(&ph.id) {
    //         // (b.role.clone(), b.path.clone())
    //         if ph.is_fragment_output {
    //             // fragment placeholders should NOT upgrade to ScalarValue via YAML binding
    //             (ph.role.clone(), None)
    //         } else {
    //             (b.role.clone(), b.path.clone())
    //         }
    //     } else {
    //         (ph.role.clone(), None)
    //     };
    //     for v in ph.values {
    //         out.push(ValueUse {
    //             value_path: v,
    //             role: role.clone(),
    //             action_span: ph.action_span.clone(),
    //             yaml_path: path.clone(),
    //         });
    //     }
    //
    //     // let binding = bindings.get(&ph.id).cloned().unwrap_or_default();
    //     // for v in ph.values {
    //     //     out.push(ValueUse {
    //     //         value_path: v,
    //     //         role: binding.role.clone(),
    //     //         action_span: ph.action_span.clone(),
    //     //         yaml_path: binding.path.clone(),
    //     //     });
    //     // }
    // }
    //
    // // // (5) collect ValueUses
    // // let mut out = Vec::new();
    // // for ph in placeholders {
    // //     let role = ph.role.clone();
    // //     let yaml_path = ph_to_yaml.get(&ph.id).cloned();
    // //     for v in ph.values {
    // //         out.push(ValueUse {
    // //             value_path: v,
    // //             role: role.clone(),
    // //             action_span: ph.action_span.clone(),
    // //             yaml_path: yaml_path.clone(),
    // //         });
    // //     }
    // // }
    //
    // Ok(out)

    // let mut out = BTreeSet::<String>::new();
    // let mut stack = vec![*node];
    //
    // while let Some(n) = stack.pop() {
    //     match n.kind() {
    //         "selector_expression" => {
    //             // NEW: only collect at the OUTERMOST selector_expression in a chain
    //             let parent_is_selector = n
    //                 .parent()
    //                 .map(|p| p.kind() == "selector_expression")
    //                 .unwrap_or(false);
    //             if !parent_is_selector {
    //                 if let Some(segs) =
    //                     helm_schema_template::values::parse_selector_expression(&n, src)
    //                 {
    //                     if !segs.is_empty() {
    //                         out.insert(segs.join("."));
    //                     }
    //                 }
    //             }
    //         }
    //         "function_call" => {
    //             if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
    //                 if !segs.is_empty() {
    //                     out.insert(segs.join("."));
    //                 }
    //             }
    //         }
    //         _ => {}
    //     }
    //
    //     let mut c = n.walk();
    //     for ch in n.children(&mut c) {
    //         if ch.is_named() {
    //             stack.push(ch);
    //         }
    //     }
    // }
    //
    // out.into_iter().collect()

    // let mut out = std::collections::BTreeSet::<String>::new();
    // let mut stack = vec![*node];
    // while let Some(n) = stack.pop() {
    //     match n.kind() {
    //         "function_call" => {
    //             // Skip traversing into printf(...) — strings derived from Values should not rebind paths.
    //             if let Some(fname) = function_name_of(&n, src) {
    //                 if fname == "printf" {
    //                     continue; // do not collect inside
    //                 }
    //             }
    //             // still allow traversal for other functions
    //         }
    //         "selector_expression" => {
    //             if let Some(segs) = helm_schema_template::values::parse_selector_expression(&n, src)
    //             {
    //                 if !segs.is_empty() {
    //                     out.insert(segs.join("."));
    //                 }
    //             }
    //         }
    //         "function_call" => {
    //             if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
    //                 if !segs.is_empty() {
    //                     out.insert(segs.join("."));
    //                 }
    //             }
    //         }
    //         _ => {}
    //     }
    //     let mut c = n.walk();
    //     for ch in n.children(&mut c) {
    //         if ch.is_named() {
    //             stack.push(ch);
    //         }
    //     }
    // }
    // out.into_iter().collect()

    // let mut out = std::collections::BTreeSet::<String>::new();
    // let mut stack = vec![*node];
    // while let Some(n) = stack.pop() {
    //     match n.kind() {
    //         "selector_expression" if helm_schema_template::values::is_terminal_selector(&n) => {
    //             if let Some(segs) = helm_schema_template::values::parse_selector_expression(&n, src)
    //             {
    //                 if !segs.is_empty() {
    //                     out.insert(segs.join("."));
    //                 }
    //             }
    //         }
    //         "function_call" => {
    //             if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
    //                 if !segs.is_empty() {
    //                     out.insert(segs.join("."));
    //                 }
    //             }
    //         }
    //         _ => {}
    //     }
    //     let mut c = n.walk();
    //     for ch in n.children(&mut c) {
    //         if ch.is_named() {
    //             stack.push(ch);
    //         }
    //     }
    // }
    // out.into_iter().collect()

    // // Build sanitized YAML, inserting placeholders ONLY for scalar-value actions
    // let mut placeholders: Vec<Placeholder> = Vec::new();
    // let sanitized = build_sanitized_with_placeholders(
    //     &source,
    //     &tree,
    //     &per_action,
    //     &mut placeholders,
    //     |src, act| classify_action(src, &act.byte_range()),
    // );
    //
    // // Parse YAML and map placeholders to YAML paths
    // let ph_to_yaml =
    //     compute_yaml_paths_for_placeholders(&sanitized).map_err(|_| Error::YamlParse)?;
    //
    // // Collect ValueUses
    // let mut out = Vec::new();
    // for ph in placeholders {
    //     let role = ph.role.clone();
    //     let yaml_path = ph_to_yaml.get(&ph.id);
    //     for v in ph.values {
    //         out.push(ValueUse {
    //             value_path: v,
    //             role: role.clone(),
    //             action_span: ph.action_span.clone(),
    //             yaml_path: yaml_path.cloned(),
    //         });
    //     }
    // }
}

pub mod sanitize {
    // pub fn build_sanitized_with_placeholders(
    //     src: &str,
    //     gtree: &tree_sitter::Tree,
    //     out_placeholders: &mut Vec<Placeholder>,
    //     collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
    // ) -> String {
    //     let mut next_id = 0usize;
    //
    //     fn walk(
    //         node: tree_sitter::Node,
    //         src: &str,
    //         buf: &mut String,
    //         next_id: &mut usize,
    //         out: &mut Vec<Placeholder>,
    //         collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
    //     ) {
    //         if node.kind() == "text" {
    //             buf.push_str(&src[node.byte_range()]);
    //             return;
    //         }
    //
    //         // Always recurse into control-flow containers; skip guard children
    //         if is_container(node.kind()) || is_control_flow(node.kind()) {
    //             let mut c = node.walk();
    //             for ch in node.children(&mut c) {
    //                 if !ch.is_named() {
    //                     continue;
    //                 }
    //                 if is_control_flow(node.kind()) && is_guard_child(&ch) {
    //                     // skip guard expressions
    //                     continue;
    //                 }
    //                 walk(ch, src, buf, next_id, out, collect_values);
    //             }
    //             return;
    //         }
    //
    //         // For every other non-text node (selector, function_call, chained_pipeline, …),
    //         // insert a quoted scalar placeholder and record its values.
    //         let id = *next_id;
    //         *next_id += 1;
    //         buf.push('"');
    //         buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
    //         buf.push('"');
    //
    //         let values = collect_values(&node);
    //         out.push(Placeholder {
    //             id,
    //             role: Role::Unknown, // filled later from YAML AST
    //             action_span: node.byte_range(),
    //             values,
    //         });
    //     }
    //
    //     let mut out = String::new();
    //     walk(
    //         gtree.root_node(),
    //         src,
    //         &mut out,
    //         &mut next_id,
    //         out_placeholders,
    //         collect_values,
    //     );
    //     out
    // }

    // if {
    //     let mut c = node.walk();
    //     for ch in node.children(&mut c) {
    //         if ch.is_named() {
    //             if is_guard_child(&ch) {
    //                 // don't emit anything; also don't record a use for guards
    //                 continue;
    //             }
    //             walk(ch, src, buf, next_id, out, collect_values);
    //         }
    //     }
    //     return;
    // }

    #[cfg(false)]
    pub mod v1 {
        pub fn build_sanitized_with_placeholders(
            src: &str,
            gtree: &tree_sitter::Tree,
            out_placeholders: &mut Vec<Placeholder>,
            classify: impl Fn(&str, &tree_sitter::Node) -> Role + Copy,
            collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
        ) -> String {
            let mut next_id = 0usize;

            fn walk(
                node: tree_sitter::Node,
                src: &str,
                buf: &mut String,
                next_id: &mut usize,
                out: &mut Vec<Placeholder>,
                classify: impl Fn(&str, &tree_sitter::Node) -> Role + Copy,
                collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
            ) {
                if node.kind() == "text" {
                    buf.push_str(&src[node.byte_range()]);
                    return;
                }

                // For any non-text node, check if it's in a scalar value position.
                let role = classify(src, &node);
                dbg!(node.utf8_text(src.as_bytes()), &role);

                match role {
                    Role::ScalarValue => {
                        let id = *next_id;
                        *next_id += 1;
                        buf.push('"');
                        buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
                        buf.push('"');

                        let values = collect_values(&node);
                        out.push(Placeholder {
                            id,
                            role: Role::ScalarValue,
                            action_span: node.byte_range(),
                            values,
                        });
                        return; // we don't descend further; the action is replaced as a whole
                    }
                    Role::MappingKey => {
                        // DO NOT insert a placeholder (we’re not mapping keys yet).
                        // Just record the usage so tests can see Role::MappingKey.
                        let id = *next_id;
                        *next_id += 1;

                        let values = collect_values(&node);
                        out.push(Placeholder {
                            id,
                            role: Role::MappingKey,
                            action_span: node.byte_range(),
                            values,
                        });
                        return; // drop the action text for sanitized YAML
                    }
                    _ => {
                        // Descend and aggregate children (e.g., inside if/with/range/include)
                        let mut c = node.walk();
                        for ch in node.children(&mut c) {
                            if ch.is_named() {
                                walk(ch, src, buf, next_id, out, classify, collect_values);
                            }
                        }
                    }
                };

                // // Otherwise, descend and emit whatever the children contribute.
                // let mut c = node.walk();
                // for ch in node.children(&mut c) {
                //     if ch.is_named() {
                //         walk(ch, src, buf, next_id, out, classify, collect_values);
                //     }
                // }
            }

            let mut out = String::new();
            let root = gtree.root_node();
            walk(
                root,
                src,
                &mut out,
                &mut next_id,
                out_placeholders,
                classify,
                collect_values,
            );
            out
        }
    }

    // /// Build sanitized YAML from the go-template tree by:
    // ///  - appending `text` nodes verbatim,
    // ///  - for each non-text top-level action: insert a scalar placeholder **if** classifier says ScalarValue.
    // ///    Otherwise, insert nothing (effectively removing the action).
    // ///
    // /// `per_action` groups action-node-id -> (node, set_of_value_paths)
    // pub fn build_sanitized_with_placeholders(
    //     src: &str,
    //     gtree: &tree_sitter::Tree,
    //     per_action: &BTreeMap<usize, (tree_sitter::Node, BTreeSet<String>)>,
    //     out_placeholders: &mut Vec<Placeholder>,
    //     classify: impl Fn(&str, &tree_sitter::Node) -> Role,
    // ) -> String {
    //     let mut out = String::new();
    //     let root = gtree.root_node();
    //     let mut children = {
    //         let mut c = root.walk();
    //         root.children(&mut c).collect::<Vec<_>>()
    //     };
    //
    //     let mut next_id = 0usize;
    //
    //     for ch in children {
    //         if ch.kind() == "text" {
    //             let r = ch.byte_range();
    //             out.push_str(&src[r]);
    //             continue;
    //         }
    //
    //         // This is a top-level template action (if/with/range/function etc.)
    //         let role = classify(src, &ch);
    //         let (values, span) = per_action
    //             .get(&ch.id())
    //             .map(|(_, set)| (set.iter().cloned().collect::<Vec<_>>(), ch.byte_range()))
    //             .unwrap_or((Vec::new(), ch.byte_range()));
    //
    //         match role {
    //             Role::ScalarValue => {
    //                 // Insert a quoted scalar placeholder; always valid YAML in value position.
    //                 let id = {
    //                     let i = next_id;
    //                     next_id += 1;
    //                     i
    //                 };
    //                 let token = format!("\"__TSG_PLACEHOLDER_{}__\"", id);
    //                 out.push_str(&token);
    //                 out_placeholders.push(Placeholder {
    //                     id,
    //                     role: Role::ScalarValue,
    //                     action_span: span.clone(),
    //                     values,
    //                 });
    //             }
    //             _ => {
    //                 // Drop the action; it’s control flow, key position, or fragment.
    //                 // The surrounding YAML (concat of text) remains parseable.
    //             }
    //         }
    //     }
    //
    //     out
    // }
}

pub mod yaml_path {
    // // ascend from `node` to the DIRECT child under `pair`
    // let n_under_pair = ascend_to_child_of(node, pair);
    //
    // if let Some(k) = pair.child_by_field_name("key") {
    //     return n_under_pair.id() == k.id();
    // }
    // // fallback: first named child is typically the key
    // if let Some(first) = pair.named_child(0) {
    //     return n_under_pair.id() == first.id();
    // }
    // false

    // // find the node that is the direct child under `pair`
    // let n_under_pair = ascend_to_child_of(node, pair);
    //
    // if let Some(k) = pair.child_by_field_name("key") {
    //     return n_under_pair.id() == k.id();
    // }
    // // fallback: first named child is typically the key
    // if let Some(first) = pair.named_child(0) {
    //     return n_under_pair.id() == first.id();
    // }
    // false

    // if let Some(path) = compute_path_for_scalar(n, sanitized) {
    //     map.insert(id, path);
    // }

    #[cfg(false)]
    pub mod v1 {
        use super::{PathElem, YamlPath};
        use std::collections::BTreeMap;
        use thiserror::Error;
        use tree_sitter::{Node, Parser};

        /// compute YAML path for a scalar node by walking up mapping/sequence parents
        fn compute_path_for_scalar(mut node: Node, src: &str) -> Option<YamlPath> {
            let mut elems = Vec::<PathElem>::new();

            loop {
                let parent = node.parent()?;
                match parent.kind() {
                    "block_mapping_pair" | "flow_pair" => {
                        // get key text
                        let key = parent
                            .child_by_field_name("key")
                            .unwrap_or_else(|| parent.named_child(0).unwrap());
                        let key_text = key.utf8_text(src.as_bytes()).ok()?;
                        // sanitize key text to identifier-ish (strip quotes if any)
                        let key_norm = key_text
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        elems.push(PathElem::Key(key_norm));
                        // move to the pair's parent
                        node = parent;
                    }
                    "block_sequence_item" | "flow_node" | "block_node" => {
                        // compute index among siblings if parent is a sequence
                        if let Some(seq) = find_sequence_parent(node) {
                            let idx = index_in_sequence(node, seq);
                            elems.push(PathElem::Index(idx));
                            node = seq;
                        } else {
                            node = parent;
                        }
                    }
                    _ => {
                        node = parent;
                    }
                }

                // stop at document root-ish nodes
                if matches!(node.kind(), "document" | "stream" | "program") {
                    break;
                }
            }

            elems.reverse();
            Some(YamlPath(elems))
        }

        fn find_sequence_parent(mut node: Node) -> Option<Node> {
            let mut p = node.parent();
            while let Some(pp) = p {
                if pp.kind().contains("sequence") {
                    return Some(pp);
                }
                p = pp.parent();
            }
            None
        }

        fn index_in_sequence(item_or_descendant: Node, seq: Node) -> usize {
            // find nearest ancestor that is a child of seq
            let mut node = item_or_descendant;
            while let Some(p) = node.parent() {
                if p.id() == seq.id() {
                    break;
                }
                node = p;
            }
            let mut idx = 0usize;
            let mut c = seq.walk();
            for ch in seq.named_children(&mut c) {
                if ch.id() == node.id() {
                    return idx;
                }
                idx += 1;
            }
            idx
        }
    }
}
