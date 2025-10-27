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

    // fn collect_define_info(node: &Node, src: &str) -> DefineInfo {
    //     let mut values = BTreeSet::<String>::new();
    //     let mut includes = BTreeSet::<String>::new();
    //
    //     let mut stack = vec![*node];
    //     while let Some(n) = stack.pop() {
    //         match n.kind() {
    //             "selector_expression" => {
    //                 // OUTERMOST selector only
    //                 let parent_is_selector = n
    //                     .parent()
    //                     .map(|p| p.kind() == "selector_expression")
    //                     .unwrap_or(false);
    //                 if !parent_is_selector {
    //                     if let Some(segs) =
    //                         helm_schema_template::values::parse_selector_expression(&n, src)
    //                     {
    //                         if !segs.is_empty() {
    //                             values.insert(segs.join("."));
    //                         }
    //                     }
    //                 }
    //             }
    //             "function_call" => {
    //                 if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
    //                     if !segs.is_empty() {
    //                         values.insert(segs.join("."));
    //                     }
    //                 }
    //                 if let Some(name) = include_call_name(&n, src) {
    //                     includes.insert(name);
    //                 }
    //             }
    //             "define_action" => {
    //                 // don’t recurse into nested defines
    //                 continue;
    //             }
    //             _ => {}
    //         }
    //         let mut c = n.walk();
    //         for ch in n.children(&mut c) {
    //             if ch.is_named() {
    //                 stack.push(ch);
    //             }
    //         }
    //     }
    //
    //     DefineInfo { values, includes }
    // }

    // fn expand_include(
    //     idx: &DefineIndex,
    //     call_node: Node,
    //     caller_scope: &Scope,
    //     state: &mut EmitState,
    //     guard: &mut ExpansionGuard,
    // ) -> Result<(), Error> {
    //     let (tpl_name, arg_expr) = parse_include_call(call_node, src)?;
    //     if guard.stack.iter().any(|n| n == &tpl_name) {
    //         // recursion; give up safely
    //         return Ok(());
    //     }
    //     let entry = match idx.get(&tpl_name) {
    //         Some(e) => e,
    //         None => return Ok(()), // unknown include; skip
    //     };
    //
    //     let mut callee = Scope {
    //         dot: caller_scope.dot.clone(),
    //         dollar: caller_scope.dollar.clone(),
    //         bindings: HashMap::new(),
    //     };
    //
    //     match arg_expr {
    //         // include "x" .
    //         ArgExpr::Dot => {
    //             callee.dot = caller_scope.dot.clone();
    //         }
    //         // include "x" $
    //         ArgExpr::Dollar => {
    //             callee.dot = caller_scope.dollar.clone();
    //         }
    //         // include "x" .Values.foo
    //         ArgExpr::Selector(sel) => {
    //             callee.dot = ExprOrigin::Selector(sel);
    //         }
    //         // include "x" (dict "config" . "ctx" $)
    //         ArgExpr::Dict(kvs) => {
    //             for (k, v) in kvs {
    //                 let origin = match v {
    //                     ArgExpr::Dot => caller_scope.dot.clone(),
    //                     ArgExpr::Dollar => caller_scope.dollar.clone(),
    //                     ArgExpr::Selector(s) => ExprOrigin::Selector(s),
    //                     _ => ExprOrigin::Opaque,
    //                 };
    //                 callee.bind_key(&k, origin);
    //             }
    //         }
    //         _ => {}
    //     }
    //
    //     guard.stack.push(tpl_name.clone());
    //     // Recurse into the define body with the callee scope
    //     let src = &entry.source;
    //     let body_src = &src[entry.body_byte_range.clone()];
    //     let tree = parse_gotmpl(body_src);
    //     walk_template_ast(&tree, &callee, state, guard)?;
    //     guard.stack.pop();
    //     Ok(())
    // }

    // /// Expand a single `include` node by finding the define body and recursively visiting it with a callee scope
    // fn expand_include(
    //     include_node: Node,
    //     caller_src: &str,
    //     caller_scope: &Scope,
    //     inline: &InlineState<'_>,
    //     guard: &mut ExpansionGuard,
    //     mut on_include: impl FnMut(Node, &Scope, &mut ExpansionGuard) -> Result<(), IncludeError>,
    //     mut on_fallback: impl FnMut(Node, &Scope, &mut ExpansionGuard) -> Result<(), IncludeError>,
    // ) -> Result<(), Error> {
    //     let (tpl_name, arg_expr) = parse_include_call(include_node, caller_src)?;
    //     if guard.stack.iter().any(|n| n == &tpl_name) {
    //         // recursion guard
    //         return Ok(());
    //     }
    //     let entry = match inline.define_index.get(&tpl_name) {
    //         Some(e) => e,
    //         None => {
    //             // unknown define; let fallback handle it (your emitter will insert placeholder)
    //             return on_fallback(include_node, caller_scope, guard).map_err(Into::into);
    //         }
    //     };
    //
    //     // Build callee scope
    //     let mut callee = Scope {
    //         dot: caller_scope.dot.clone(),
    //         dollar: caller_scope.dollar.clone(),
    //         bindings: HashMap::new(),
    //     };
    //
    //     match arg_expr {
    //         ArgExpr::Dot => {
    //             callee.dot = caller_scope.dot.clone();
    //         }
    //         ArgExpr::Dollar => {
    //             callee.dot = caller_scope.dollar.clone();
    //         }
    //         ArgExpr::Selector(sel) => {
    //             callee.dot = ExprOrigin::Selector(sel);
    //         }
    //         ArgExpr::Dict(kvs) => {
    //             for (k, v) in kvs {
    //                 let origin = match v {
    //                     ArgExpr::Dot => caller_scope.dot.clone(),
    //                     ArgExpr::Dollar => caller_scope.dollar.clone(),
    //                     ArgExpr::Selector(s) => ExprOrigin::Selector(s),
    //                     _ => ExprOrigin::Opaque,
    //                 };
    //                 callee.bind_key(&k, origin);
    //             }
    //         }
    //         _ => { /* keep defaults */ }
    //     }
    //
    //     // Recurse into define body
    //     guard.stack.push(tpl_name.to_owned());
    //     let body_src = &entry.source[entry.body_byte_range.clone()];
    //     let parsed = parse_gotmpl_document(body_src).ok_or(Error::ParseTemplate)?;
    //     let inline2 = InlineState {
    //         define_index: inline.define_index,
    //     };
    //     walk_template_ast(
    //         &parsed.tree,
    //         body_src,
    //         &callee,
    //         &inline2,
    //         guard,
    //         |inc_node, sc, guard| {
    //             expand_include(
    //                 inc_node,
    //                 body_src,
    //                 sc,
    //                 &inline2,
    //                 guard,
    //                 on_include,
    //                 on_fallback,
    //             )
    //         },
    //         |node, sc, guard| on_fallback(node, sc, guard).map_err(Into::into),
    //     )?;
    //     guard.stack.pop();
    //     Ok(())
    // }

    // "selector_expression" => {
    //     if let Some(segs) = helm_schema_template::values::parse_selector_expression(&n, src)
    //     {
    //         // Also handle tokenization as ["$", "var", ...] (some grammars split it this way)
    //         if !segs.is_empty() && segs[0] == "$" && segs.len() >= 2 {
    //             let var = segs[1].trim_start_matches('.');
    //             if let Some(bases) = env.get(var) {
    //                 let tail = if segs.len() > 2 {
    //                     segs[2..]
    //                         .iter()
    //                         .map(|s| s.trim_start_matches('.'))
    //                         .collect::<Vec<_>>()
    //                         .join(".")
    //                 } else {
    //                     String::new()
    //                 };
    //                 for base in bases {
    //                     let key = if tail.is_empty() {
    //                         base.clone()
    //                     } else {
    //                         format!("{base}.{tail}")
    //                     };
    //                     out.insert(key);
    //                 }
    //                 continue;
    //             }
    //         }
    //
    //         // NEW: $var.foo — expand using env
    //         if let Some(first) = segs.first() {
    //             if first.starts_with('$') {
    //                 let var = first.trim_start_matches('$');
    //                 if let Some(bases) = env.get(var) {
    //                     let tail = if segs.len() > 1 {
    //                         segs[1..].join(".")
    //                     } else {
    //                         String::new()
    //                     };
    //                     for base in bases {
    //                         let key = if tail.is_empty() {
    //                             base.clone()
    //                         } else {
    //                             format!("{base}.{tail}")
    //                         };
    //                         out.insert(key);
    //                     }
    //                     // done with this selector
    //                     continue;
    //                 }
    //             }
    //         }
    //
    //         // Wouldn't it make more sense to just get the text of the variable node?
    //         if let Some(op) = n.child_by_field_name("operand").or_else(|| n.child(0)) {
    //             if op.kind() == "variable" {
    //                 if let Some(var) = variable_ident_of(&op, src) {
    //                     if let Some(bases) = env.get(&var) {
    //                         // compute tail after the variable using parse_selector_expression if available
    //                         let tail = if let Some(segs2) =
    //                             helm_schema_template::values::parse_selector_expression(
    //                                 &n, src,
    //                             ) {
    //                             if segs2.len() > 1 {
    //                                 segs2[1..].join(".")
    //                             } else {
    //                                 String::new()
    //                             }
    //                         } else {
    //                             String::new()
    //                         };
    //                         for base in bases {
    //                             let key = if tail.is_empty() {
    //                                 base.clone()
    //                             } else {
    //                                 format!("{base}.{tail}")
    //                             };
    //                             out.insert(key);
    //                         }
    //                         continue; // handled
    //                     }
    //                 }
    //             }
    //         }
    //
    //         // Normal: resolve via scope (. / $ / bindings / .Values)
    //         let origin = scope.resolve_selector(&segs);
    //         if let Some(vp) = origin_to_value_path(&origin) {
    //             out.insert(vp);
    //         }
    //     }
    // }

    // if !segs.is_empty() && segs[0] == "Values" {
    //     out.insert(segs[1..].join("."));
    // }

    // ArgExpr::Selector(sel) => {
    //     callee.dot = ExprOrigin::Selector(sel);
    // }

    // ArgExpr::Dict(kvs) => {
    //     for (k, v) in kvs {
    //         let origin = match v {
    //             ArgExpr::Dot => caller_scope.dot.clone(),
    //             ArgExpr::Dollar => caller_scope.dollar.clone(),
    //             ArgExpr::Selector(s) => ExprOrigin::Selector(s),
    //             _ => ExprOrigin::Opaque,
    //         };
    //         callee.bind_key(&k, origin);
    //     }
    // }

    // // Intercept include calls
    // if node.kind() == "function_call" && is_include_call(node, src) {
    //     self.expand_include(node, src, scope, guard)?;
    //     return Ok(());
    // }
    //
    // // Default behavior: let your emitter handle this node
    // (self.on_fallback)(node, scope, src, guard)?;
    //
    // // Recurse
    // for i in 0..node.child_count() {
    //     if let Some(ch) = node.child(i) {
    //         self.walk_node(ch, src, scope, guard)?;
    //     }
    // }
    // Ok(())

    // "selector_expression" => {
    //     // e.g. ".Values.persistentVolumeClaims"
    //     // Flatten into ["Values", "persistentVolumeClaims", ...]
    //     let mut segs = Vec::<String>::new();
    //     // Walk left-to-right collecting field identifiers and the first field ".Values"
    //     fn collect(mut n: Node, src: &str, segs: &mut Vec<String>) {
    //         match n.kind() {
    //             "selector_expression" => {
    //                 // left . right
    //                 let left = n
    //                     .child_by_field_name("operand")
    //                     .unwrap_or_else(|| n.child(0).unwrap());
    //                 let right = n
    //                     .child_by_field_name("field")
    //                     .unwrap_or_else(|| n.child(1).unwrap());
    //                 collect(left, src, segs);
    //                 let ident = right
    //                     .utf8_text(src.as_bytes())
    //                     .unwrap_or("")
    //                     .trim()
    //                     .to_string();
    //                 if ident != "" {
    //                     segs.push(ident);
    //                 }
    //             }
    //             "field" | "identifier" | "field_identifier" => {
    //                 let ident =
    //                     n.utf8_text(src.as_bytes()).unwrap_or("").trim().to_string();
    //                 if ident != "" {
    //                     segs.push(ident);
    //                 }
    //             }
    //             "dot" => { /* ignore leading dot */ }
    //             _ => {}
    //         }
    //     }
    //     collect(node, src, &mut segs);
    //     // Prepend "Values" if the selector started with .Values
    //     // The chain normally is ["Values", "foo", "bar"] already; if not, keep as seen.
    //     ArgExpr::Selector(segs)
    // }

    // // everything before the first 'text' child is the guard region
    // let mut origins = Vec::new();
    // let mut body_start: Option<usize> = None;
    //
    // {
    //     let mut w = action.walk();
    //     for ch in action.children(&mut w) {
    //         if ch.is_named() && ch.kind() == "text" {
    //             body_start = Some(ch.start_byte());
    //             break;
    //         }
    //     }
    // }
    // let cutoff = body_start.unwrap_or(usize::MAX);
    // let mut w = action.walk();
    // for ch in action.children(&mut w) {
    //     if !ch.is_named() || ch.start_byte() >= cutoff {
    //         break;
    //     }
    //     // Reuse your scoped collector so `.config.*` resolves through include bindings
    //     for vp in collect_values_with_scope(ch, src, scope, &BTreeMap::new()) {
    //         // promote only selectors
    //         origins.push(ExprOrigin::Selector(
    //             std::iter::once("Values".to_string())
    //                 .chain(vp.split('.').map(|s| s.to_string()))
    //                 .collect(),
    //         ));
    //     }
    // }
    // origins

    // fn extend_origin(&self, base: ExprOrigin, tail: &[String]) -> ExprOrigin {
    //     match base {
    //         ExprOrigin::Selector(mut v) => {
    //             v.extend(tail.iter().cloned());
    //             ExprOrigin::Selector(v)
    //         }
    //         ExprOrigin::Dot | ExprOrigin::Dollar | ExprOrigin::Opaque => ExprOrigin::Opaque,
    //     }
    // }

    // let selector = match segments[0].as_str() {
    //     "." => {
    //         if segments.len() == 1 {
    //             return self.dot.clone();
    //         }
    //         // If `.something` where `something` is a binding (e.g. `.config`), use the binding as base
    //         let first_field = segments[1].as_str();
    //         if let Some(bound) = self.bindings.get(first_field) {
    //             if segments.len() == 2 {
    //                 return bound.clone();
    //             }
    //             return self.extend_origin(bound.clone(), &segments[2..]);
    //         }
    //         // Otherwise, extend the current dot
    //         self.extend_origin(self.dot.clone(), &segments[1..])
    //     }
    //     "$" => {
    //         if segments.len() == 1 {
    //             return self.dollar.clone();
    //         }
    //         self.extend_origin(self.dollar.clone(), &segments[1..])
    //     }
    //     first => {
    //         // binding like `.config.*`
    //         if let Some(bound) = self.bindings.get(first) {
    //             if segments.len() == 1 {
    //                 return bound.clone();
    //             }
    //             return self.extend_origin(bound.clone(), &segments[1..]);
    //         }
    //         // explicit `.Values.*` (rare with our parser, but keep it)
    //         if first == "Values" {
    //             let mut v = vec!["Values".to_string()];
    //             v.extend_from_slice(&segments[1..]);
    //             return ExprOrigin::Selector(v);
    //         }
    //         // NEW: treat bare segments as relative to the current dot (usually `["Values", ...]`)
    //         if let ExprOrigin::Selector(_) = self.dot {
    //             return self.extend_origin(self.dot.clone(), segments);
    //         }
    //         ExprOrigin::Opaque
    //         // // Direct binding without a leading '.' (rare but possible)
    //         // if let Some(bound) = self.bindings.get(first) {
    //         //     if segments.len() == 1 {
    //         //         return bound.clone();
    //         //     }
    //         //     return self.extend_origin(bound.clone(), &segments[1..]);
    //         // }
    //         // // Canonical .Values.* → Selector(["Values", ...])
    //         // if first == "Values" {
    //         //     let mut v = vec!["Values".to_string()];
    //         //     v.extend_from_slice(&segments[1..]);
    //         //     return ExprOrigin::Selector(v);
    //         // }
    //         // ExprOrigin::Opaque
    //     }
    // };

    // if segments.is_empty() {
    //     return self.dot.clone();
    // }
    // // first segment is one of ".", "$", or identifier
    // match segments[0].as_str() {
    //     "." => {
    //         if segments.len() == 1 {
    //             return self.dot.clone();
    //         }
    //         self.extend_origin(self.dot.clone(), &segments[1..])
    //     }
    //     "$" => {
    //         if segments.len() == 1 {
    //             return self.dollar.clone();
    //         }
    //         self.extend_origin(self.dollar.clone(), &segments[1..])
    //     }
    //     "Values" => {
    //         let mut v = vec!["Values".into()];
    //         v.extend_from_slice(&segments[1..]);
    //         return ExprOrigin::Selector(v);
    //     }
    //     first => {
    //         // treat `.first...` as lookup in bindings (e.g. `.config`, `.ctx`)
    //         let base = self
    //             .bindings
    //             .get(first)
    //             .cloned()
    //             .unwrap_or(ExprOrigin::Opaque);
    //         if segments.len() == 1 {
    //             return base;
    //         }
    //         self.extend_origin(base, &segments[1..])
    //     }
    // }

    // fn function_name_of(node: &Node, src: &str) -> Option<String> {
    //     if node.kind() != "function_call" {
    //         return None;
    //     }
    //
    //     // Try the field first (some grammars expose it)…
    //     if let Some(f) = node.child_by_field_name("function") {
    //         if let Ok(s) = f.utf8_text(src.as_bytes()) {
    //             return Some(s.trim().to_string());
    //         }
    //     }
    //
    //     // Otherwise fall back to the first identifier child.
    //     for i in 0..node.child_count() {
    //         if let Some(ch) = node.child(i) {
    //             if ch.is_named() && ch.kind() == "identifier" {
    //                 if let Ok(s) = ch.utf8_text(src.as_bytes()) {
    //                     return Some(s.trim().to_string());
    //                 }
    //             }
    //         }
    //     }
    //     None
    // }

    // fn function_name_of(node: &Node, src: &str) -> Option<String> {
    //     if node.kind() != "function_call" {
    //         return None;
    //     }
    //     let f = node.child_by_field_name("function")?;
    //     Some(f.utf8_text(src.as_bytes()).ok()?.to_string())
    // }

    // /// Analyze a *single* template file from VFS.
    // pub fn analyze_template_file(path: &VfsPath) -> Result<Vec<ValueUse>, Error> {
    //     let source = path.read_to_string()?;
    //
    //     // Parse whole document as go-template
    //     let parsed = parse_gotmpl_document(&source).ok_or(Error::ParseTemplate)?;
    //     let tree = &parsed.tree;
    //     let root = tree.root_node();
    //
    //     // Index defines in same directory and compute closure
    //     let defines = index_defines_in_dir(&path.parent())?;
    //     let define_closure = compute_define_closure(&defines);
    //
    //     // Wrapped collector that augments include() found anywhere in the subtree.
    //     let collect = |n: &Node| -> Vec<String> {
    //         // Start with the regular .Values found under this subtree
    //         let mut set: BTreeSet<String> = collect_values_in_subtree(n, &source).into_iter().collect();
    //
    //         // NEW: scan the subtree for any include() and merge its define-closure values
    //         let mut stack = vec![*n];
    //         while let Some(q) = stack.pop() {
    //             if q.kind() == "function_call" {
    //                 if let Some(name) = include_call_name(&q, &source) {
    //                     if let Some(vals) = define_closure.get(&name) {
    //                         set.extend(vals.iter().cloned());
    //                     }
    //                 }
    //             }
    //             let mut c = q.walk();
    //             for ch in q.children(&mut c) {
    //                 if ch.is_named() {
    //                     stack.push(ch);
    //                 }
    //             }
    //         }
    //
    //         set.into_iter().collect()
    //     };
    //
    //     // Build sanitized YAML with placeholders (roles inferred later from YAML structure)
    //     let mut placeholders: Vec<crate::sanitize::Placeholder> = Vec::new();
    //     let sanitized = build_sanitized_with_placeholders(&source, tree, &mut placeholders, collect);
    //
    //     println!("SANITIZED YAML:\n\n{sanitized}");
    //
    //     // STRICT CHECK (syntax only)
    //     if let Err(e) = crate::sanitize::validate_yaml_strict_all_docs(&sanitized) {
    //         eprintln!("{}", crate::sanitize::pretty_yaml_error(&sanitized, &e));
    //         // Bubble up a clean error for your caller/tests
    //         return Err(Error::YamlParse);
    //     }
    //
    //     // Map placeholders back to YAML paths (+ roles)
    //     let ph_to_yaml =
    //         compute_yaml_paths_for_placeholders(&sanitized).map_err(|_| Error::YamlParse)?;
    //
    //     // Collect output uses
    //     let mut out = Vec::new();
    //     for ph in placeholders {
    //         // let role = ph.role.clone();
    //         // let yaml_path = ph_to_yaml.get(&ph.id).and_then(|b| b.path.clone()).clone();
    //         let yaml_path = ph_to_yaml.get(&ph.id).and_then(|b| b.path.clone()).clone();
    //         let mut role = ph_to_yaml
    //             .get(&ph.id)
    //             .map(|b| b.role.clone())
    //             .unwrap_or(ph.role);
    //         if ph.is_fragment_output {
    //             // Force fragment role; yaml_path stays as the insertion site (parent path)
    //             role = Role::Fragment;
    //         }
    //         for v in ph.values {
    //             out.push(ValueUse {
    //                 value_path: v,
    //                 // role: ph_to_yaml
    //                 //     .get(&ph.id)
    //                 //     .map(|b| b.role.clone())
    //                 //     .unwrap_or(role.clone()),
    //                 role,
    //                 action_span: ph.action_span.clone(),
    //                 yaml_path: yaml_path.clone(),
    //             });
    //         }
    //     }
    //
    //     // Merge guard uses captured directly from AST
    //     let guard_uses = collect_guards(tree, &source);
    //     out.extend(guard_uses);
    //
    //     let mut seen = HashSet::new();
    //     out.retain(|v| {
    //         seen.insert((
    //             v.value_path.as_str().to_owned(),
    //             v.role,
    //             v.action_span.start,
    //             v.action_span.end,
    //         ))
    //     });
    //     // out.sort_by_key(|v| (v.value_path.clone(), v.role, v.action_span.clone()))
    //     //     .dedup_by_key(|v| (v.value_path.clone(), v.role, v.action_span.clone()));
    //
    //     dbg!(
    //         &out.iter()
    //             .filter(|v| v.value_path == "ingress.ingressClassName")
    //             .collect::<Vec<_>>()
    //     );
    //     // dbg!(&out);
    //
    //     Ok(out)
    // }

    // if function_name_of(&n, src).as_deref() == Some("index") {
    //     if let Some(args) = n.child_by_field_name("argument_list") {
    //         // find first named child = base, then subsequent string keys
    //         let mut w = args.walk();
    //         let mut named = args.named_children(&mut w);
    //         if let Some(base) = named.next() {
    //             // gather string keys
    //             let mut keys: Vec<String> = Vec::new();
    //             let mut w2 = args.walk();
    //             for ch in args.named_children(&mut w2) {
    //                 if ch.id() == base.id() {
    //                     continue;
    //                 }
    //                 match ch.kind() {
    //                     "interpreted_string_literal"
    //                     | "raw_string_literal"
    //                     | "string" => {
    //                         let k = ch
    //                             .utf8_text(src.as_bytes())
    //                             .unwrap_or("")
    //                             .trim()
    //                             .trim_matches('"')
    //                             .trim_matches('`')
    //                             .to_string();
    //                         if !k.is_empty() {
    //                             keys.push(k);
    //                         }
    //                     }
    //                     _ => {}
    //                 }
    //             }
    //
    //             // resolve base -> list of base paths
    //             let mut bases: Vec<String> = Vec::new();
    //             match base.kind() {
    //                 "selector_expression" => {
    //                     if let Some(segs) =
    //                         helm_schema_template::values::parse_selector_expression(
    //                             &base, src,
    //                         )
    //                     {
    //                         let origin = scope.resolve_selector(&segs);
    //                         if let Some(vp) = origin_to_value_path(&origin) {
    //                             bases.push(vp);
    //                         }
    //                     }
    //                 }
    //                 "dot" => {
    //                     if let Some(vp) = origin_to_value_path(&scope.dot) {
    //                         bases.push(vp);
    //                     }
    //                 }
    //                 "variable" => {
    //                     let raw = base.utf8_text(src.as_bytes()).unwrap_or("").trim();
    //                     if raw == "$" {
    //                         if let Some(vp) = origin_to_value_path(&scope.dollar) {
    //                             bases.push(vp); // "" at top-level (root .Values)
    //                         }
    //                     } else {
    //                         let name = raw.trim_start_matches('$');
    //                         if let Some(set) = env.get(name) {
    //                             bases.extend(set.iter().cloned());
    //                         }
    //                     }
    //                 }
    //                 // "variable" => {
    //                 //     // $var
    //                 //     let vtxt = base
    //                 //         .utf8_text(src.as_bytes())
    //                 //         .unwrap_or("")
    //                 //         .trim()
    //                 //         .trim_start_matches('$')
    //                 //         .to_string();
    //                 //     if let Some(set) = env.get(&vtxt) {
    //                 //         bases.extend(set.iter().cloned());
    //                 //     }
    //                 // }
    //                 _ => {}
    //             }
    //
    //             for b in bases {
    //                 if keys.is_empty() {
    //                     out.insert(b);
    //                     continue;
    //                 }
    //                 // Base is the root of .Values -> don't prefix a dot; also drop a leading "Values" key.
    //                 if b.is_empty() {
    //                     if !keys.is_empty() && keys[0] == "Values" {
    //                         out.insert(keys[1..].join("."));
    //                     } else {
    //                         out.insert(keys.join("."));
    //                     }
    //                 } else {
    //                     out.insert(format!("{b}.{}", keys.join(".")));
    //                 }
    //             }
    //             // for b in bases {
    //             //     if keys.is_empty() {
    //             //         out.insert(b);
    //             //     } else {
    //             //         out.insert(format!("{b}.{}", keys.join(".")));
    //             //     }
    //             // }
    //         }
    //     }
    // }

    // while let Some(n) = stack.pop() {
    //     match n.kind() {
    //         "dot" => {
    //             if let Some(vp) = origin_to_value_path(&scope.dot) {
    //                 out.insert(vp);
    //             }
    //         }
    //         "selector_expression" => {
    //             if let Some(segs) = helm_schema_template::values::parse_selector_expression(&n, src)
    //             {
    //                 // resolve through scope (bindings .config / .ctx)
    //                 let origin = scope.resolve_selector(&segs);
    //                 if let Some(vp) = origin_to_value_path(&origin) {
    //                     out.insert(vp);
    //                 }
    //             }
    //         }
    //         "function_call" => {
    //             if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
    //                 if !segs.is_empty() && segs[0] == "Values" {
    //                     out.insert(segs[1..].join("."));
    //                 }
    //             }
    //             if let Some(k) = parse_values_field_call(&n, src) {
    //                 out.insert(k);
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

    // let mut acc = BTreeSet::<String>::new();
    // let mut stack = vec![node];
    // while let Some(n) = stack.pop() {
    //     match n.kind() {
    //         "selector_expression" => {
    //             // outermost only
    //             let parent_is_selector = n
    //                 .parent()
    //                 .map(|p| p.kind() == "selector_expression")
    //                 .unwrap_or(false);
    //             if !parent_is_selector {
    //                 if let Some(segs) = parse_selector_chain(n, src) {
    //                     let origin = scope.resolve_selector(&segs);
    //                     if let Some(p) = origin_to_value_path(&origin) {
    //                         acc.insert(p);
    //                     }
    //                 }
    //             }
    //         }
    //         "variable" => {
    //             if let Some(name) = variable_ident_of(&n, src) {
    //                 if let Some(set) = env.get(&name) {
    //                     acc.extend(set.iter().cloned());
    //                 }
    //             }
    //         }
    //         "function_call" => {
    //             // `.Values` grammar variants (index, Values .foo) — reuse your helpers
    //             if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
    //                 if !segs.is_empty() {
    //                     // segs is like ["Values","foo","bar"] → "foo.bar"
    //                     if segs[0] == "Values" {
    //                         acc.insert(segs[1..].join("."));
    //                     }
    //                 }
    //             }
    //             if let Some(k) = parse_values_field_call(&n, src) {
    //                 acc.insert(k);
    //             }
    //             // don't expand include() here — include is handled at the call site
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
    // acc

    // // treat toYaml (and include used for YAML chunks) as fragment emitters
    // if node.kind() == "function_call" {
    //     if let Some(name) = function_name_of(node, src) {
    //         return name == "toYaml";
    //     }
    // }
    // if node.kind() == "chained_pipeline" {
    //     let mut c = node.walk();
    //     for ch in node.children(&mut c) {
    //         if ch.is_named() && ch.kind() == "function_call" {
    //             if let Some(name) = function_name_of(&ch, src) {
    //                 if name == "toYaml" {
    //                     return true;
    //                 }
    //             }
    //         }
    //     }
    // }
    // false

    // fn current_slot_in_buf(buf: &str) -> Slot {
    //     let mut it = buf.rsplit('\n');
    //     let cur = it.next().unwrap_or("");
    //     let prev_non_empty = it.find(|l| !l.trim().is_empty()).unwrap_or("");
    //     let cur_trim = cur.trim_start();
    //     if cur_trim.starts_with("- ") {
    //         return Slot::SequenceItem;
    //     }
    //     if prev_non_empty.trim_end().ends_with(':') {
    //         return Slot::MappingValue;
    //     }
    //     Slot::Plain
    // }

    // let id = out.next_id;
    // out.next_id += 1;
    //
    // // Always write the scalar marker into YAML (even for fragments) — this anchors parent paths.
    // out.buf.push('"');
    // out.buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
    // out.buf.push('"');
    //
    // out.placeholders.push(Placeholder {
    //     id,
    //     role: Role::Unknown,
    //     action_span: node.byte_range(),
    //     values: values.into_iter().collect(),
    //     is_fragment_output,
    // });

    // fn walk_container(
    //     container: Node,
    //     src: &str,
    //     scope: &Scope,
    //     inline: &InlineState<'_>,
    //     guard: &mut ExpansionGuard,
    //     out: &mut InlineOut,
    // ) -> Result<(), Error> {
    //     let mut c = container.walk();
    //
    //     for ch in container.children(&mut c) {
    //         if !ch.is_named() {
    //             continue;
    //         }
    //
    //         // raw text
    //         if ch.kind() == "text" {
    //             out.buf.push_str(&src[ch.byte_range()]);
    //             continue;
    //         }
    //
    //         // guards (no YAML output)
    //         if is_guard_piece(&ch) {
    //             let vals = collect_values_with_scope(ch, src, scope, &out.env);
    //             for v in vals {
    //                 out.guards.push(ValueUse {
    //                     value_path: v,
    //                     role: Role::Guard,
    //                     action_span: ch.byte_range(),
    //                     yaml_path: None,
    //                 });
    //             }
    //             continue;
    //         }
    //
    //         // Nested containers / control flow blocks
    //         if crate::sanitize::is_container(ch.kind())
    //             || crate::sanitize::is_control_flow(ch.kind())
    //         {
    //             if ch.kind() == "with_action" || ch.kind() == "range_action" {
    //                 let origins = guard_selectors_for_action(ch, src, scope);
    //                 let mut body_scope = scope.clone();
    //                 if let Some(sel) = origins.iter().find_map(|o| {
    //                     if let ExprOrigin::Selector(v) = o {
    //                         Some(v.clone())
    //                     } else {
    //                         None
    //                     }
    //                 }) {
    //                     body_scope.dot = ExprOrigin::Selector(sel);
    //                 }
    //                 walk_container(ch, src, &body_scope, inline, guard, out)?;
    //             } else {
    //                 walk_container(ch, src, scope, inline, guard, out)?;
    //             }
    //             continue;
    //             // walk_container(ch, src, scope, inline, guard, out)?;
    //             // continue;
    //         }
    //
    //         // Output expressions (+ include handling)
    //         if matches!(
    //             ch.kind(),
    //             // "selector_expression" | "function_call" | "chained_pipeline" | "variable"
    //             "selector_expression" | "function_call" | "chained_pipeline" | "variable" | "dot"
    //         ) {
    //             // Special case: include
    //             if ch.kind() == "function_call" && is_include_call(ch, src) {
    //                 // classify use site (value vs fragment-ish)
    //                 let ctx = classify_action(src, &ch.byte_range());
    //                 let (tpl, arg) = parse_include_call(ch, src)?;
    //                 // recursion guard
    //                 if guard.stack.iter().any(|n| n == &tpl) {
    //                     continue;
    //                 }
    //                 let entry = inline
    //                     .define_index
    //                     .get(&tpl)
    //                     .ok_or_else(|| IncludeError::UnknownDefine(tpl.clone()))?;
    //
    //                 // Build callee scope from arg
    //                 let mut callee = Scope {
    //                     dot: scope.dot.clone(),
    //                     dollar: scope.dollar.clone(),
    //                     bindings: Default::default(),
    //                 };
    //                 match arg {
    //                     ArgExpr::Dot => callee.dot = scope.dot.clone(),
    //                     ArgExpr::Dollar => callee.dot = scope.dollar.clone(),
    //                     ArgExpr::Selector(sel) => callee.dot = ExprOrigin::Selector(sel),
    //                     ArgExpr::Dict(kvs) => {
    //                         for (k, v) in kvs {
    //                             let origin = match v {
    //                                 ArgExpr::Dot => scope.dot.clone(),
    //                                 ArgExpr::Dollar => scope.dollar.clone(),
    //                                 ArgExpr::Selector(s) => ExprOrigin::Selector(s),
    //                                 _ => ExprOrigin::Opaque,
    //                             };
    //                             callee.bind_key(&k, origin);
    //                         }
    //                     }
    //                     _ => {}
    //                 }
    //
    //                 // Parse callee body
    //                 let body_src = &entry.source[entry.body_byte_range.clone()];
    //                 let body = parse_gotmpl_document(body_src).ok_or(Error::ParseTemplate)?;
    //
    //                 match ctx {
    //                     Role::ScalarValue => {
    //                         // (scalar position) → emit one placeholder carrying callee .Values (including nested includes)
    //                         let body_src = &entry.source[entry.body_byte_range.clone()];
    //                         let body =
    //                             parse_gotmpl_document(body_src).ok_or(Error::ParseTemplate)?;
    //                         let vals = collect_values_following_includes(
    //                             body.tree.root_node(),
    //                             body_src,
    //                             &callee,
    //                             inline,
    //                             guard,
    //                             &out.env,
    //                         );
    //                         write_placeholder(
    //                             out, ch, src, vals, /*is_fragment_output=*/ false,
    //                         );
    //
    //                         // Also scan callee for guards (no YAML emission)
    //                         let mut tmp = InlineOut::default();
    //                         inline_emit_tree(
    //                             &body.tree, body_src, &callee, inline, guard, &mut tmp,
    //                         )?;
    //                         out.guards.extend(tmp.guards.into_iter());
    //                     }
    //                     // MappingKey/Unknown/Fragment/Guard → treat as fragment include: inline the body
    //                     _ => {
    //                         guard.stack.push(tpl.clone());
    //                         let body_src = &entry.source[entry.body_byte_range.clone()];
    //                         let body =
    //                             parse_gotmpl_document(body_src).ok_or(Error::ParseTemplate)?;
    //                         inline_emit_tree(&body.tree, body_src, &callee, inline, guard, out)?;
    //                         guard.stack.pop();
    //                     }
    //                 }
    //
    //                 // match ctx {
    //                 //     // Include used in a scalar "value" position → emit one scalar placeholder
    //                 //     Role::ScalarValue | Role::MappingKey | Role::Unknown => {
    //                 //         // collect all callee .Values.* w.r.t. callee scope
    //                 //         let vals = collect_values_with_scope(
    //                 //             body.tree.root_node(),
    //                 //             body_src,
    //                 //             &callee,
    //                 //             &out.env,
    //                 //         );
    //                 //         write_placeholder(
    //                 //             out, ch, src, vals, /*is_fragment_output=*/ false,
    //                 //         );
    //                 //         // Also descend the callee to record guards/assignments (no YAML emission)
    //                 //         // We do not splice callee text here (keeps the scalar line valid).
    //                 //         // Scan only — reusing the same walk, but into a throwaway buffer.
    //                 //         let mut tmp = InlineOut::default();
    //                 //         inline_emit_tree(
    //                 //             &body.tree, body_src, &callee, inline, guard, &mut tmp,
    //                 //         )?;
    //                 //         // merge guard uses & env side effects (env is only relevant for $vars inside caller; callee's $vars are ephemeral)
    //                 //         out.guards.extend(tmp.guards.into_iter());
    //                 //     }
    //                 //     // Include as a YAML fragment (top-level / block) → inline callee body into buf
    //                 //     Role::Fragment | Role::Guard => {
    //                 //         guard.stack.push(tpl.clone());
    //                 //         inline_emit_tree(&body.tree, body_src, &callee, inline, guard, out)?;
    //                 //         guard.stack.pop();
    //                 //     }
    //                 // }
    //                 continue;
    //             }
    //
    //             // Non-include output: emit a scalar placeholder (fragment aware)
    //             let is_frag = looks_like_fragment(&ch, src);
    //             let vals =
    //                 collect_values_following_includes(ch, src, scope, inline, guard, &out.env);
    //             write_placeholder(out, ch, src, vals, is_frag);
    //             continue;
    //         }
    //
    //         // Assignments (`$x := ...`) — record uses and bind env; no YAML output
    //         if crate::sanitize::is_assignment_kind(ch.kind()) {
    //             // let vals = collect_values_with_scope(ch, src, scope, &out.env);
    //             let vals =
    //                 collect_values_following_includes(ch, src, scope, inline, guard, &out.env);
    //
    //             // record as a fragment use (no yaml_path)
    //             let id = out.next_id;
    //             out.next_id += 1;
    //             out.placeholders.push(Placeholder {
    //                 id,
    //                 role: Role::Fragment,
    //                 action_span: ch.byte_range(),
    //                 values: vals.iter().cloned().collect(),
    //                 is_fragment_output: true,
    //             });
    //             // bind the first variable on LHS to RHS values
    //             let mut vc = ch.walk();
    //             for sub in ch.children(&mut vc) {
    //                 if sub.is_named() && sub.kind() == "variable" {
    //                     if let Some(name) = variable_ident_of(&sub, src) {
    //                         out.env.insert(name, vals);
    //                     }
    //                     break;
    //                 }
    //             }
    //             continue;
    //         }
    //
    //         // Everything else: skip (keeps YAML valid)
    //     }
    //
    //     Ok(())
    // }

    // Role::ScalarValue => {
    //     let vals = collect_values_following_includes(
    //         body.tree.root_node(),
    //         body_src,
    //         &callee,
    //         inline,
    //         guard,
    //         &out.env,
    //     );
    //     write_placeholder(
    //         out, ch, src, vals, /*is_fragment_output=*/ false,
    //     );
    //
    //     let mut tmp = InlineOut::default();
    //     inline_emit_tree(
    //         &body.tree, body_src, &callee, inline, guard, &mut tmp,
    //     )?;
    //     out.guards.extend(tmp.guards.into_iter());
    // }

    // #[cfg(test)]
    // mod tests {
    //     use test_util::prelude::*;
    //
    //     #[test]
    //     fn resolves_dot_values_as_absolute() {
    //         use super::{ExprOrigin, Scope};
    //
    //         let mut s = Scope {
    //             dot: ExprOrigin::Selector(vec!["Values".into(), "persistentVolumeClaims".into()]),
    //             dollar: ExprOrigin::Selector(vec!["Values".into()]),
    //             bindings: Default::default(),
    //         };
    //         let got = s.resolve_selector(&[".".into(), "Values".into(), "fullnameOverride".into()]);
    //         assert!(matches!(got, ExprOrigin::Selector(v) if v == vec!["Values","fullnameOverride"]));
    //     }
    // }

    //     // Existing: index with .Values …
    //     if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
    //         if !segs.is_empty() {
    //             if segs[0] == "Values" {
    //                 out.insert(segs[1..].join("."));
    //             } else if segs.len() >= 2 && segs[0] == "$" && segs[1] == "Values" {
    //                 out.insert(segs[2..].join("."));
    //             }
    //         }
    //     }
    //
    //     // NEW: generic index base: . / binding (.config) / $var
    //     if function_name_of(&n, src).as_deref() == Some("index") {
    //         if let Some(args) = arg_list(&n) {
    //             let mut w = args.walk();
    //             let mut named = args.named_children(&mut w);
    //             if let Some(base) = named.next() {
    //                 // gather string keys
    //                 let mut keys: Vec<String> = Vec::new();
    //                 let mut w2 = args.walk();
    //                 for ch in args.named_children(&mut w2) {
    //                     if ch.id() == base.id() {
    //                         continue;
    //                     }
    //                     if matches!(
    //                         ch.kind(),
    //                         "interpreted_string_literal" | "raw_string_literal" | "string"
    //                     ) {
    //                         let k = ch
    //                             .utf8_text(src.as_bytes())
    //                             .unwrap_or("")
    //                             .trim()
    //                             .trim_matches('"')
    //                             .trim_matches('`')
    //                             .to_string();
    //                         if !k.is_empty() {
    //                             keys.push(k);
    //                         }
    //                     }
    //                 }
    //
    //                 // resolve base -> list of base paths
    //                 let mut bases: Vec<String> = Vec::new();
    //                 match base.kind() {
    //                     "selector_expression" => {
    //                         if let Some(segs) =
    //                             helm_schema_template::values::parse_selector_expression(
    //                                 &base, src,
    //                             )
    //                         {
    //                             if let Some(vp) =
    //                                 origin_to_value_path(&scope.resolve_selector(&segs))
    //                             {
    //                                 bases.push(vp);
    //                             }
    //                         }
    //                     }
    //                     "dot" => {
    //                         if let Some(vp) = origin_to_value_path(&scope.dot) {
    //                             bases.push(vp);
    //                         }
    //                     }
    //                     "variable" => {
    //                         let raw = base.utf8_text(src.as_bytes()).unwrap_or("").trim();
    //                         if raw == "$" {
    //                             if let Some(vp) = origin_to_value_path(&scope.dollar) {
    //                                 bases.push(vp); // "" at root
    //                             }
    //                         } else {
    //                             let name = raw.trim_start_matches('$');
    //                             if let Some(set) = env.get(name) {
    //                                 bases.extend(set.iter().cloned());
    //                             }
    //                         }
    //                     }
    //                     "field" => {
    //                         // Prefer explicit identifier nodes if present, otherwise use the full node text.
    //                         let name = base
    //                             .child_by_field_name("identifier")
    //                             .or_else(|| base.child_by_field_name("field_identifier"))
    //                             .and_then(|id| {
    //                                 id.utf8_text(src.as_bytes())
    //                                     .ok()
    //                                     .map(|s| s.trim().to_string())
    //                             })
    //                             .unwrap_or_else(|| {
    //                                 // Fallback: ".config" → "config"
    //                                 base.utf8_text(src.as_bytes())
    //                                     .unwrap_or("")
    //                                     .trim()
    //                                     .trim_start_matches('.')
    //                                     .to_string()
    //                             });
    //
    //                         if let Some(bound) = scope.bindings.get(&name) {
    //                             if let Some(vp) = origin_to_value_path(bound) {
    //                                 bases.push(vp);
    //                             } else {
    //                                 // Binding exists but isn't a .Values selector: resolve relative to dot.
    //                                 let segs = vec![".".to_string(), name.clone()];
    //                                 if let Some(vp) =
    //                                     origin_to_value_path(&scope.resolve_selector(&segs))
    //                                 {
    //                                     bases.push(vp);
    //                                 }
    //                             }
    //                         } else {
    //                             // No binding: resolve `.name` relative to current dot/bindings.
    //                             let segs = vec![".".to_string(), name];
    //                             if let Some(vp) =
    //                                 origin_to_value_path(&scope.resolve_selector(&segs))
    //                             {
    //                                 bases.push(vp);
    //                             }
    //                         }
    //                     }
    //                     // // NEW: `.config` arrives as a `field` node in this grammar
    //                     // "field" => {
    //                     //     // Field nodes look like `.config` and usually have an inner identifier `config`.
    //                     //     let name = base
    //                     //         .child_by_field_name("identifier")
    //                     //         .or_else(|| base.child(0))
    //                     //         .and_then(|id| id.utf8_text(src.as_bytes()).ok())
    //                     //         .map(|s| s.trim().trim_start_matches('.').to_string())
    //                     //         .unwrap_or_else(|| {
    //                     //             // Fallback to the full node text if no child, and strip the leading dot.
    //                     //             base.utf8_text(src.as_bytes())
    //                     //                 .unwrap_or("")
    //                     //                 .trim()
    //                     //                 .trim_start_matches('.')
    //                     //                 .to_string()
    //                     //         });
    //                     //
    //                     //     // 1) Prefer a direct binding: `.config` should resolve to whatever the caller bound.
    //                     //     if let Some(bound) = scope.bindings.get(&name) {
    //                     //         if let Some(vp) = origin_to_value_path(bound) {
    //                     //             bases.push(vp);
    //                     //             // We’re done for this base.
    //                     //             // (No `continue` here because we’re already inside the match arm.)
    //                     //         } else {
    //                     //             // If the binding isn’t a .Values selector, fall back to scoped resolution below.
    //                     //             let segs = vec![".".to_string(), name.clone()];
    //                     //             if let Some(vp) =
    //                     //                 origin_to_value_path(&scope.resolve_selector(&segs))
    //                     //             {
    //                     //                 bases.push(vp);
    //                     //             }
    //                     //         }
    //                     //     } else {
    //                     //         // 2) No binding: resolve `.name` relative to the current dot/bindings.
    //                     //         let segs = vec![".".to_string(), name];
    //                     //         if let Some(vp) =
    //                     //             origin_to_value_path(&scope.resolve_selector(&segs))
    //                     //         {
    //                     //             bases.push(vp);
    //                     //         }
    //                     //     }
    //                     // }
    //                     // "field" => {
    //                     //     if let Some(id) = base
    //                     //         .child_by_field_name("identifier")
    //                     //         .or_else(|| base.child(0))
    //                     //     {
    //                     //         if let Ok(name) = id.utf8_text(src.as_bytes()) {
    //                     //             // Treat as ["." , name] and resolve via bindings/dot
    //                     //             let segs =
    //                     //                 vec![".".to_string(), name.trim().to_string()];
    //                     //             if let Some(vp) =
    //                     //                 origin_to_value_path(&scope.resolve_selector(&segs))
    //                     //             {
    //                     //                 bases.push(vp);
    //                     //             }
    //                     //         }
    //                     //     }
    //                     // }
    //                     _ => {}
    //                 }
    //
    //                 for mut b in bases {
    //                     // Guard against a stray literal "." base (tokenization oddity).
    //                     if b == "." {
    //                         b.clear();
    //                     }
    //                     if keys.is_empty() {
    //                         out.insert(b);
    //                     } else if b.is_empty() {
    //                         if !keys.is_empty() && keys[0] == "Values" {
    //                             out.insert(keys[1..].join("."));
    //                         } else {
    //                             out.insert(keys.join("."));
    //                         }
    //                     } else {
    //                         out.insert(format!("{b}.{}", keys.join(".")));
    //                     }
    //                 }
    //                 // for b in bases {
    //                 //     if keys.is_empty() {
    //                 //         out.insert(b);
    //                 //     } else if b.is_empty() {
    //                 //         // root .Values
    //                 //         if !keys.is_empty() && keys[0] == "Values" {
    //                 //             out.insert(keys[1..].join("."));
    //                 //         } else {
    //                 //             out.insert(keys.join("."));
    //                 //         }
    //                 //     } else {
    //                 //         out.insert(format!("{b}.{}", keys.join(".")));
    //                 //     }
    //                 // }
    //             }
    //         }
    //     }

    // // NEW: `.config` arrives as a `field` node in this grammar
    // "field" => {
    //     // Field nodes look like `.config` and usually have an inner identifier `config`.
    //     let name = base
    //         .child_by_field_name("identifier")
    //         .or_else(|| base.child(0))
    //         .and_then(|id| id.utf8_text(src.as_bytes()).ok())
    //         .map(|s| s.trim().trim_start_matches('.').to_string())
    //         .unwrap_or_else(|| {
    //             // Fallback to the full node text if no child, and strip the leading dot.
    //             base.utf8_text(src.as_bytes())
    //                 .unwrap_or("")
    //                 .trim()
    //                 .trim_start_matches('.')
    //                 .to_string()
    //         });
    //
    //     // 1) Prefer a direct binding: `.config` should resolve to whatever the caller bound.
    //     if let Some(bound) = scope.bindings.get(&name) {
    //         if let Some(vp) = origin_to_value_path(bound) {
    //             bases.push(vp);
    //             // We’re done for this base.
    //             // (No `continue` here because we’re already inside the match arm.)
    //         } else {
    //             // If the binding isn’t a .Values selector, fall back to scoped resolution below.
    //             let segs = vec![".".to_string(), name.clone()];
    //             if let Some(vp) =
    //                 origin_to_value_path(&scope.resolve_selector(&segs))
    //             {
    //                 bases.push(vp);
    //             }
    //         }
    //     } else {
    //         // 2) No binding: resolve `.name` relative to the current dot/bindings.
    //         let segs = vec![".".to_string(), name];
    //         if let Some(vp) =
    //             origin_to_value_path(&scope.resolve_selector(&segs))
    //         {
    //             bases.push(vp);
    //         }
    //     }
    // }
    // "field" => {
    //     if let Some(id) = base
    //         .child_by_field_name("identifier")
    //         .or_else(|| base.child(0))
    //     {
    //         if let Ok(name) = id.utf8_text(src.as_bytes()) {
    //             // Treat as ["." , name] and resolve via bindings/dot
    //             let segs =
    //                 vec![".".to_string(), name.trim().to_string()];
    //             if let Some(vp) =
    //                 origin_to_value_path(&scope.resolve_selector(&segs))
    //             {
    //                 bases.push(vp);
    //             }
    //         }
    //     }
    // }

    // if let Some(segs) =
    //     helm_schema_template::values::parse_selector_expression(&head, src)
    // {
    //     let origin = scope.resolve_selector(&segs);
    //     if let Some(vp) = origin_to_value_path(&origin) {
    //         out.insert(vp);
    //     }
    // }

    // // A single, reusable walker that does the recursion.
    // // It holds a *reference* to your fallback (= existing emitter) so we never move it.
    // struct AstWalker<'a> {
    //     inline: &'a InlineState<'a>,
    //     // on_fallback: your existing node handler (emits YAML + ValueUse)
    //     on_fallback: &'a mut dyn FnMut(Node, &Scope, &str, &mut ExpansionGuard) -> Result<(), Error>,
    // }
    //
    // impl<'a> AstWalker<'a> {
    //     fn walk_tree(
    //         &mut self,
    //         tree: &tree_sitter::Tree,
    //         src: &str,
    //         scope: &Scope,
    //         guard: &mut ExpansionGuard,
    //     ) -> Result<(), Error> {
    //         self.walk_node(tree.root_node(), src, scope, guard)
    //     }
    //
    //     fn walk_node(
    //         &mut self,
    //         node: Node,
    //         src: &str,
    //         scope: &Scope,
    //         guard: &mut ExpansionGuard,
    //     ) -> Result<(), Error> {
    //         // --- NEW: handle with/range so that `.` in the body points to the guard selector ---
    //         if node.kind() == "with_action" || node.kind() == "range_action" {
    //             // 1) Find guard selectors (anything before the first text node in the action)
    //             let guard_origins = guard_selectors_for_action(node, src, scope);
    //
    //             // (optional) record guards here if you want; otherwise your existing collect_guards
    //             // pass already gathers them.
    //
    //             // 2) Derive the body scope: if there is a single clear selector, make `.` that selector
    //             let mut body_scope = scope.clone();
    //             if let Some(sel) = guard_origins.iter().find_map(|o| match o {
    //                 ExprOrigin::Selector(v) => Some(v.clone()),
    //                 _ => None,
    //             }) {
    //                 body_scope.dot = ExprOrigin::Selector(sel);
    //             }
    //
    //             // 3) Recurse into children with the adjusted `.` scope
    //             for i in 0..node.child_count() {
    //                 if let Some(ch) = node.child(i) {
    //                     self.walk_node(ch, src, &body_scope, guard)?;
    //                 }
    //             }
    //             return Ok(());
    //         }
    //
    //         // existing include interception
    //         if node.kind() == "function_call" && is_include_call(node, src) {
    //             self.expand_include(node, src, scope, guard)?;
    //             return Ok(());
    //         }
    //
    //         // default: hand off and recurse
    //         (self.on_fallback)(node, scope, src, guard)?;
    //         for i in 0..node.child_count() {
    //             if let Some(ch) = node.child(i) {
    //                 self.walk_node(ch, src, scope, guard)?;
    //             }
    //         }
    //         Ok(())
    //     }
    //
    //     fn expand_include(
    //         &mut self,
    //         include_node: Node,
    //         caller_src: &str,
    //         caller_scope: &Scope,
    //         guard: &mut ExpansionGuard,
    //     ) -> Result<(), Error> {
    //         let (tpl_name, arg_expr) = parse_include_call(include_node, caller_src)?;
    //         if guard.stack.iter().any(|n| n == &tpl_name) {
    //             // recursion guard
    //             return Ok(());
    //         }
    //
    //         let entry = self
    //             .inline
    //             .define_index
    //             .get(&tpl_name)
    //             .ok_or_else(|| IncludeError::UnknownDefine(tpl_name.clone()))?;
    //
    //         // Build callee scope from the include arg
    //         let mut callee = Scope {
    //             dot: caller_scope.dot.clone(),
    //             dollar: caller_scope.dollar.clone(),
    //             bindings: Default::default(),
    //         };
    //         match arg_expr {
    //             ArgExpr::Dot => {
    //                 callee.dot = caller_scope.dot.clone();
    //             }
    //             ArgExpr::Dollar => {
    //                 callee.dot = caller_scope.dollar.clone();
    //             }
    //             ArgExpr::Selector(sel) => {
    //                 callee.dot = caller_scope.resolve_selector(&sel);
    //             }
    //             ArgExpr::Dict(kvs) => {
    //                 for (k, v) in kvs {
    //                     let origin = match v {
    //                         ArgExpr::Dot => caller_scope.dot.clone(),
    //                         ArgExpr::Dollar => caller_scope.dollar.clone(),
    //                         ArgExpr::Selector(s) => caller_scope.resolve_selector(&s),
    //                         _ => ExprOrigin::Opaque,
    //                     };
    //                     callee.bind_key(&k, origin);
    //                 }
    //             }
    //             _ => {}
    //         }
    //
    //         // Inline: parse the define body and walk it with the derived scope
    //         guard.stack.push(tpl_name.clone());
    //         let body_src = &entry.source[entry.body_byte_range.clone()];
    //         let parsed = parse_gotmpl_document(body_src).ok_or(Error::ParseTemplate)?;
    //         self.walk_tree(&parsed.tree, body_src, &callee, guard)?;
    //         guard.stack.pop();
    //         Ok(())
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

    // fn current_slot(buf: &str) -> Slot {
    //     // look at the current and previous *non-empty* lines only
    //     let mut it = buf.rsplit('\n');
    //     let cur = it.next().unwrap_or("");
    //     let prev_non_empty = it
    //         .find(|l| !l.trim().is_empty())
    //         .map(|n| n.to_string())
    //         .unwrap_or_default();
    //
    //     let cur_trim = cur.trim_start();
    //     if cur_trim.starts_with("- ") {
    //         return Slot::SequenceItem;
    //     }
    //     if prev_non_empty.trim_end().ends_with(':') {
    //         return Slot::MappingValue;
    //     }
    //     Slot::Plain
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
