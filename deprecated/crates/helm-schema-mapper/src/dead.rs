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

    // if segments.is_empty() {
    //     return ExprOrigin::Opaque;
    // }
    //
    // let head = segments[0].as_str();
    //
    // // NEW: variable reference like `$cfg`
    // if let Some(var_name) = head.strip_prefix('$') {
    //     if var_name.is_empty() {
    //         // plain `$` (root); keep existing behavior
    //         return self.resolve_absolute_root(&segments[1..]);
    //     }
    //     if let Some(origin) = self.bindings.get(var_name) {
    //         return Self::append_fields(origin.clone(), &segments[1..]);
    //     } else {
    //         // Unbound variable: do NOT pretend it's `.Values.<name>`
    //         return ExprOrigin::Opaque;
    //     }
    // }

    // // bare "$"
    // if segments.len() == 1 {
    //     return self.dollar.clone();
    // }
    // // "$.Values.*" → absolute .Values chain
    // if segments[1].as_str() == "Values" {
    //     let mut v = vec!["Values".to_string()];
    //     if segments.len() > 2 {
    //         v.extend_from_slice(&segments[2..]);
    //     }
    //     return ExprOrigin::Selector(v);
    // }
    // // Ignore "$.Capabilities.*" / "$.Chart.*" / etc.
    // if is_helm_builtin_root(&segments[1]) {
    //     return ExprOrigin::Opaque;
    // }
    // // "$name[.tail]" → treat like a binding lookup
    // let var = segments[1].as_str();
    // if let Some(bound) = self.bindings.get(var) {
    //     if segments.len() == 2 {
    //         return bound.clone();
    //     }
    //     return self.extend_origin(bound.clone(), &segments[2..]);
    // }
    // // Unbound $var: hard error — mirrors Helm behavior (unknown variable)
    // panic!("unbound template variable ${var} in selector: {segments:?}",);

    // // If this is just `$`, return the bound dollar origin.
    // if segments.len() == 1 {
    //     return self.dollar.clone();
    // }
    //
    // // `$<name>` should resolve through local bindings first.
    // let second = segments[1].as_str();
    //
    // // Ignore Helm builtins like $.Capabilities.*, $.Chart.*, etc.
    // if is_helm_builtin_root(second) {
    //     return ExprOrigin::Opaque;
    // }
    //
    // // Absolute $.Values.*  → treat as absolute Values selector
    // if second == "Values" {
    //     let mut v = vec!["Values".to_string()];
    //     if segments.len() > 2 {
    //         v.extend_from_slice(&segments[2..]);
    //     }
    //     return ExprOrigin::Selector(v);
    // }
    //
    // // Bound variable: `$cfg.foo` → (bindings["cfg"]).foo
    // if let Some(bound) = self.bindings.get(second) {
    //     if segments.len() == 2 {
    //         return bound.clone();
    //     }
    //     return self.extend_origin(bound.clone(), &segments[2..]);
    // }
    //
    // // Fallback: extend the caller's `$` origin.
    // return self.extend_origin(self.dollar.clone(), &segments[1..]);

    // // Ignore `$ .Capabilities.*` / `$ .Chart.*` / etc.
    // if segments.len() >= 2 && is_helm_builtin_root(&segments[1]) {
    //     return ExprOrigin::Opaque;
    // }
    // if segments.len() == 1 {
    //     return self.dollar.clone();
    // }
    // return self.extend_origin(self.dollar.clone(), &segments[1..]);

    // first => {
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

    // // Drop a duplicated leading "Values" (e.g. ["Values"] + ["Values","x"]).
    // let mut t: Vec<String> = tail.to_vec();
    // if !v.is_empty() && v[0] == "Values" && !t.is_empty() && t[0] == "Values" {
    //     t.remove(0);
    // }
    // v.extend(t);
    // ExprOrigin::Selector(v)

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

    // "selector_expression" => {
    //     if let Some(segs) =
    //         helm_schema_template::values::parse_selector_expression(&node, src)
    //     {
    //         ArgExpr::Selector(segs)
    //     } else {
    //         ArgExpr::Opaque
    //     }
    // }

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

    // if let Some(segs) = helm_schema_template::values::parse_selector_expression(&n, src)
    // {
    // Handle tokenizations that include $ in the segments
    // if !segs.is_empty() && segs[0] == "$" && segs.len() >= 2 {
    //     let var = segs[1].trim_start_matches('.');
    //     if let Some(bases) = env.get(var) {

    // ArgExpr::Selector(sel) => callee.dot = ExprOrigin::Selector(sel),
    // ArgExpr::Selector(s) => ExprOrigin::Selector(s),

    // if let Some(sel) = guard_selectors_for_action(container, src, scope)
    //     .into_iter()
    //     .find_map(|o| match o {
    //         ExprOrigin::Selector(v) => Some(v),
    //         _ => None,
    //     })
    // {
    //     s.dot = ExprOrigin::Selector(sel);
    // }

    // // NEW: even inside the guard, capture $var := ... assignments
    // // so the body can resolve $x.foo to the correct .Values base.
    // if crate::sanitize::is_assignment_kind(ch.kind()) {
    //     let vals = collect_values_following_includes(
    //         ch, src, scope, inline, guard, &out.env,
    //     );
    //
    //     // We reuse the same "assignment" handling as below, but with guard scope.
    //     let id = out.next_id;
    //     out.next_id += 1;
    //     out.placeholders.push(Placeholder {
    //         id,
    //         role: Role::Fragment, // assignment produces no scalar
    //         action_span: ch.byte_range(),
    //         values: vals.iter().cloned().collect(),
    //         is_fragment_output: true, // don't emit into YAML buffer
    //     });
    //
    //     // Bind `$var` -> set of .Values.* for later expansion (`$var.tail`)
    //     let mut vc = ch.walk();
    //     for sub in ch.children(&mut vc) {
    //         if sub.is_named() && sub.kind() == "variable" {
    //             if let Some(name) = variable_ident_of(&sub, src) {
    //                 out.env.insert(name, vals.clone());
    //             }
    //             break;
    //         }
    //     }
    //     // fall through: we still emit Guard entries for the container below
    // }

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

    // let vals =
    //     collect_values_following_includes(ch, src, use_scope, inline, guard, &out.env);
    // write_placeholder(out, ch, src, vals, is_frag);
    // continue;

    // let id = out.next_id;
    // out.next_id += 1;
    // out.placeholders.push(Placeholder {
    //     id,
    //     role: Role::Fragment,
    //     action_span: ch.byte_range(),
    //     values: vals.iter().cloned().collect(),
    //     is_fragment_output: true,
    // });

    // {
    //     // Compute the set of .Values sources feeding this action.
    //     let vals: BTreeSet<String> = if let Some(head) = pipeline_head(ch) {
    //         let head_keys = collect_values_with_scope(head, src, use_scope, &out.env);
    //         if head_keys.len() == 1 {
    //             head_keys
    //         } else {
    //             collect_values_following_includes(
    //                 ch, src, use_scope, inline, guard, &out.env,
    //             )
    //         }
    //     } else {
    //         collect_values_following_includes(
    //             ch, src, use_scope, inline, guard, &out.env,
    //         )
    //     };
    //
    //     // Decide if this action is *emitting a YAML fragment* (vs a scalar).
    //     // - toYaml/fromYaml → fragment-ish
    //     // - tpl → fragment only when piped through nindent/indent
    //     let slot = current_slot_in_buf(&out.buf);
    //     let fragmentish_pipeline =
    //         pipeline_contains_any(&ch, src, &["toYaml", "fromYaml"])
    //             || (pipeline_contains_any(&ch, src, &["tpl"])
    //                 && pipeline_contains_any(&ch, src, &["nindent", "indent"]));
    //
    //     // We still only *write* a fragment placeholder when we’re clearly in a mapping/sequence slot.
    //     let is_frag_output = matches!(slot, Slot::MappingValue | Slot::SequenceItem)
    //         && fragmentish_pipeline;
    //
    //     // NEW: force fragment *role* for fromYaml even if we keep a scalar placeholder on the same line.
    //     let force_fragment_role = pipeline_contains_any(&ch, src, &["fromYaml"]);
    //
    //     write_placeholder(out, ch, src, vals, is_frag_output, force_fragment_role);
    //     continue;
    // }

    // {
    //     // Compute the set of .Values sources feeding this action.
    //     // Prefer the pipeline head if it yields a single key; otherwise collect across the whole subtree.
    //     let vals: BTreeSet<String> = if let Some(head) = pipeline_head(ch) {
    //         let head_keys = collect_values_with_scope(head, src, use_scope, &out.env);
    //         if head_keys.len() == 1 {
    //             head_keys
    //         } else {
    //             collect_values_following_includes(
    //                 ch, src, use_scope, inline, guard, &out.env,
    //             )
    //         }
    //     } else {
    //         collect_values_following_includes(
    //             ch, src, use_scope, inline, guard, &out.env,
    //         )
    //     };
    //
    //     // Decide if this action is *emitting a YAML fragment* (vs a scalar).
    //     // Rules:
    //     // - `toYaml` → fragment (when inserted into a YAML slot)
    //     // - `fromYaml` → fragment (when inserted into a YAML slot)
    //     // - `tpl` → fragment only when used in a fragment-ish pipeline (e.g. `| nindent` / `| indent`)
    //     //
    //     // We still *only* mark as fragment when we're in a YAML value/sequence slot; inline scalars remain ScalarValue.
    //     let slot = current_slot_in_buf(&out.buf);
    //     let fragmentish_pipeline =
    //         pipeline_contains_any(&ch, src, &["toYaml", "fromYaml"])
    //             || (pipeline_contains_any(&ch, src, &["tpl"])
    //                 && pipeline_contains_any(&ch, src, &["nindent", "indent"]));
    //
    //     let is_frag_output = matches!(slot, Slot::MappingValue | Slot::SequenceItem)
    //         && fragmentish_pipeline;
    //
    //     write_placeholder(out, ch, src, vals, is_frag_output);
    //     continue;
    // }

    // let is_frag = looks_like_fragment(&ch, src);
    //
    // // Prefer using the pipeline "head" (covers bare `field`, `selector_expression`,
    // // or the head of a `chained_pipeline`). If that yields exactly one key,
    // // use it; otherwise fall back to the full include-aware collector.
    // let vals: BTreeSet<String> = if let Some(head) = pipeline_head(ch) {
    //     let head_keys = collect_values_with_scope(head, src, use_scope, &out.env);
    //     // collect_values_with_scope(head, src, use_scope, &Env::default());
    //     if head_keys.len() == 1 {
    //         head_keys
    //     } else {
    //         collect_values_following_includes(
    //             ch, src, use_scope, inline, guard, &out.env,
    //         )
    //     }
    // } else {
    //     collect_values_following_includes(ch, src, use_scope, inline, guard, &out.env)
    // };
    //
    // write_placeholder(out, ch, src, vals, is_frag);
    // continue;

    // _ => {
    //     guard.stack.push(tpl.clone());
    //     inline_emit_tree(
    //         &body.tree, body_src, &callee, inline, guard, out,
    //     )?;
    //     guard.stack.pop();
    // }

    // // Emit prefixes (like WITH/IF), but still suppress guard == current dot
    // let dot_vp = origin_to_value_path(&scope.dot);
    // let mut seen = std::collections::BTreeSet::new();
    //
    // for o in origins {
    //     if let Some(vp) = origin_to_value_path(&o) {
    //         if vp.is_empty() {
    //             continue;
    //         }
    //         if dot_vp.as_ref() == Some(&vp) {
    //             // range .  → no guard output
    //             continue;
    //         }
    //
    //         // Emit a, a.b, a.b.c (prefix expansion)
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

    // if is_fragment_output {
    //     let slot = current_slot_in_buf(&out.buf);
    //     eprintln!(
    //         "[frag-ph] id={id} slot={:?} after='{}'",
    //         slot,
    //         last_non_empty_line(&out.buf).trim_end()
    //     );
    //     write_fragment_placeholder_line(&mut out.buf, id, slot);
    // } else {
    //     out.buf.push('"');
    //     out.buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
    //     out.buf.push('"');
    // }
    //
    // out.placeholders.push(Placeholder {
    //     id,
    //     role: Role::Unknown,
    //     action_span: node.byte_range(),
    //     values: values.into_iter().collect(),
    //     is_fragment_output,
    // });
    //
    // // NEW: remember to force fragment role for this placeholder id
    // if force_fragment_role {
    //     out.force_fragment_role.insert(id);
    // }

    // fn looks_like_fragment(node: &Node, src: &str) -> bool {
    //     // true if the node itself or any function in its pipeline is include|toYaml
    //     let mut check = |n: &Node| -> bool {
    //         if n.kind() != "function_call" {
    //             return false;
    //         }
    //         if let Some(name) = function_name_of(n, src) {
    //             return name == "include" || name == "toYaml";
    //         }
    //         false
    //     };
    //     match node.kind() {
    //         "function_call" => check(node),
    //         "chained_pipeline" => {
    //             let mut w = node.walk();
    //             for ch in node.children(&mut w) {
    //                 if ch.is_named() && ch.kind() == "function_call" && check(&ch) {
    //                     return true;
    //                 }
    //             }
    //             false
    //         }
    //         _ => false,
    //     }
    // }

    // let mut bases: Vec<String> = Vec::new();
    // match base.kind() {
    //     "selector_expression" => {
    //         if let Some(segs) = parse_selector_expression(&base, src) {
    //             if let Some(vp) =
    //                 origin_to_value_path(&scope.resolve_selector(&segs))
    //             {
    //                 bases.push(vp);
    //             }
    //         }
    //     }
    //     "dot" => {
    //         if let Some(vp) = origin_to_value_path(&scope.dot) {
    //             bases.push(vp);
    //         }
    //     }
    //     "variable" => {
    //         let raw = base.utf8_text(src.as_bytes()).unwrap_or("").trim();
    //         if raw == "$" {
    //             if let Some(vp) = origin_to_value_path(&scope.dollar) {
    //                 bases.push(vp); // "" at root
    //             }
    //         } else {
    //             let name = raw.trim_start_matches('$');
    //             if let Some(set) = env.get(name) {
    //                 bases.extend(set.iter().cloned());
    //             } else {
    //                 panic!(
    //                     "unbound template variable ${name} used as index() base at bytes {:?}",
    //                     n.byte_range()
    //                 );
    //             }
    //         }
    //     }
    //     "field" => {
    //         // Prefer explicit identifier nodes if present, otherwise use the full node text.
    //         let name = base
    //             .child_by_field_name("identifier")
    //             .or_else(|| base.child_by_field_name("field_identifier"))
    //             .and_then(|id| {
    //                 id.utf8_text(src.as_bytes())
    //                     .ok()
    //                     .map(|s| s.trim().to_string())
    //             })
    //             .unwrap_or_else(|| {
    //                 // Fallback: ".config" → "config"
    //                 base.utf8_text(src.as_bytes())
    //                     .unwrap_or("")
    //                     .trim()
    //                     .trim_start_matches('.')
    //                     .to_string()
    //             });
    //
    //         if let Some(bound) = scope.bindings.get(&name) {
    //             if let Some(vp) = origin_to_value_path(bound) {
    //                 // NOTE: allow empty base ("") for root-of-.Values; later code handles it.
    //                 bases.push(vp);
    //             } else {
    //                 // Binding exists but isn't a .Values selector: resolve relative to dot.
    //                 let segs = vec![".".to_string(), name.clone()];
    //                 if let Some(vp) =
    //                     origin_to_value_path(&scope.resolve_selector(&segs))
    //                 {
    //                     bases.push(vp);
    //                 }
    //             }
    //         } else {
    //             // No binding: resolve `.name` relative to current dot/bindings.
    //             let segs = vec![".".to_string(), name];
    //             if let Some(vp) =
    //                 origin_to_value_path(&scope.resolve_selector(&segs))
    //             {
    //                 bases.push(vp);
    //             }
    //         }
    //         // // Prefer explicit identifier nodes if present, otherwise use the full node text.
    //         // let name = base
    //         //     .child_by_field_name("identifier")
    //         //     .or_else(|| base.child_by_field_name("field_identifier"))
    //         //     .and_then(|id| {
    //         //         id.utf8_text(src.as_bytes())
    //         //             .ok()
    //         //             .map(|s| s.trim().to_string())
    //         //     })
    //         //     .unwrap_or_else(|| {
    //         //         // Fallback: ".config" → "config"
    //         //         base.utf8_text(src.as_bytes())
    //         //             .unwrap_or("")
    //         //             .trim()
    //         //             .trim_start_matches('.')
    //         //             .to_string()
    //         //     });
    //         //
    //         // if let Some(bound) = scope.bindings.get(&name) {
    //         //     if let Some(vp) = origin_to_value_path(bound) {
    //         //         debug_assert!(!vp.is_empty());
    //         //         if !vp.is_empty() {
    //         //             bases.push(vp);
    //         //         }
    //         //     } else {
    //         //         // Binding exists but isn't a .Values selector: resolve relative to dot.
    //         //         let segs = vec![".".to_string(), name.clone()];
    //         //         if let Some(vp) =
    //         //             origin_to_value_path(&scope.resolve_selector(&segs))
    //         //         {
    //         //             bases.push(vp);
    //         //         }
    //         //     }
    //         // } else {
    //         //     // No binding: resolve `.name` relative to current dot/bindings.
    //         //     let segs = vec![".".to_string(), name];
    //         //     if let Some(vp) =
    //         //         origin_to_value_path(&scope.resolve_selector(&segs))
    //         //     {
    //         //         debug_assert!(!vp.is_empty());
    //         //         if !vp.is_empty() {
    //         //             bases.push(vp);
    //         //         }
    //         //     }
    //         // }
    //     }
    //
    //     _ => {}
    // }

    // let mut bases: Vec<String> = Vec::new();
    // match base.kind() {
    //     "selector_expression" => {
    //         if let Some(segs) = parse_selector_chain(base, src)
    //         // if let Some(segs) =
    //         //     parse_selector_expression(&base, src)
    //         //         .or_else(|| parse_selector_chain(base, src))
    //         {
    //             if let Some(vp) = origin_to_value_path(
    //                 &scope.resolve_selector(&segs),
    //             ) {
    //                 bases.push(vp);
    //             }
    //         }
    //     }
    //     "dot" => {
    //         if let Some(vp) = origin_to_value_path(&scope.dot) {
    //             bases.push(vp);
    //         }
    //     }
    //     "variable" => {
    //         let raw = base
    //             .utf8_text(src.as_bytes())
    //             .unwrap_or("")
    //             .trim();
    //         if raw == "$" {
    //             if let Some(vp) =
    //                 origin_to_value_path(&scope.dollar)
    //             {
    //                 bases.push(vp);
    //             }
    //         } else {
    //             let name = raw.trim_start_matches('$');
    //             if let Some(set) = env.get(name) {
    //                 bases.extend(set.iter().cloned());
    //             } else {
    //                 panic!(
    //                     "unbound template variable ${name} used as pluck() base at bytes {:?}",
    //                     n.byte_range()
    //                 );
    //             }
    //         }
    //     }
    //     "field" => {
    //         let name = base
    //             .child_by_field_name("identifier")
    //             .or_else(|| {
    //                 base.child_by_field_name("field_identifier")
    //             })
    //             .and_then(|id| {
    //                 id.utf8_text(src.as_bytes())
    //                     .ok()
    //                     .map(|s| s.trim().to_string())
    //             })
    //             .unwrap_or_else(|| {
    //                 base.utf8_text(src.as_bytes())
    //                     .unwrap_or("")
    //                     .trim()
    //                     .trim_start_matches('.')
    //                     .to_string()
    //             });
    //
    //         if let Some(bound) = scope.bindings.get(&name) {
    //             if let Some(vp) = origin_to_value_path(bound) {
    //                 bases.push(vp);
    //             } else {
    //                 let segs =
    //                     vec![".".to_string(), name.clone()];
    //                 if let Some(vp) = origin_to_value_path(
    //                     &scope.resolve_selector(&segs),
    //                 ) {
    //                     bases.push(vp);
    //                 }
    //             }
    //         } else {
    //             let segs = vec![".".to_string(), name];
    //             if let Some(vp) = origin_to_value_path(
    //                 &scope.resolve_selector(&segs),
    //             ) {
    //                 bases.push(vp);
    //             }
    //         }
    //     }
    //     _ => {}
    // }

    // if let Some(segs) = parse_selector_expression(&first, src)
    //     .or_else(|| parse_selector_chain(first, src))

    // let mut bases: Vec<String> = Vec::new();
    // match base.kind() {
    //     "selector_expression" => {
    //         if let Some(segs) = parse_selector_chain(base, src)
    //         // if let Some(segs) = parse_selector_expression(&base, src)
    //         //     .or_else(|| parse_selector_chain(base, src))
    //         {
    //             if let Some(vp) =
    //                 origin_to_value_path(&scope.resolve_selector(&segs))
    //             {
    //                 bases.push(vp);
    //             }
    //         }
    //     }
    //     "dot" => {
    //         if let Some(vp) = origin_to_value_path(&scope.dot) {
    //             bases.push(vp);
    //         }
    //     }
    //     "variable" => {
    //         let raw = base.utf8_text(src.as_bytes()).unwrap_or("").trim();
    //         if raw == "$" {
    //             if let Some(vp) = origin_to_value_path(&scope.dollar) {
    //                 bases.push(vp);
    //             }
    //         } else {
    //             let name = raw.trim_start_matches('$');
    //             if let Some(set) = env.get(name) {
    //                 bases.extend(set.iter().cloned());
    //             } else {
    //                 panic!(
    //                     "unbound template variable ${name} used as get() base at bytes {:?}",
    //                     n.byte_range()
    //                 );
    //             }
    //         }
    //     }
    //     "field" => {
    //         let name = base
    //             .child_by_field_name("identifier")
    //             .or_else(|| base.child_by_field_name("field_identifier"))
    //             .and_then(|id| {
    //                 id.utf8_text(src.as_bytes())
    //                     .ok()
    //                     .map(|s| s.trim().to_string())
    //             })
    //             .unwrap_or_else(|| {
    //                 base.utf8_text(src.as_bytes())
    //                     .unwrap_or("")
    //                     .trim()
    //                     .trim_start_matches('.')
    //                     .to_string()
    //             });
    //
    //         if let Some(bound) = scope.bindings.get(&name) {
    //             if let Some(vp) = origin_to_value_path(bound) {
    //                 bases.push(vp);
    //             } else {
    //                 let segs = vec![".".to_string(), name.clone()];
    //                 if let Some(vp) =
    //                     origin_to_value_path(&scope.resolve_selector(&segs))
    //                 {
    //                     bases.push(vp);
    //                 }
    //             }
    //         } else {
    //             let segs = vec![".".to_string(), name];
    //             if let Some(vp) =
    //                 origin_to_value_path(&scope.resolve_selector(&segs))
    //             {
    //                 bases.push(vp);
    //             }
    //         }
    //     }
    //     _ => {}
    // }

    // if let Some(segs) = parse_selector_expression(&head, src)
    //     .or_else(|| parse_selector_chain(head, src))

    // let var = segs[1].trim_start_matches('.');
    // if var == "Values" {
    //     // treat as "$.Values..." — fall through to normal resolution
    // } else if let Some(bases) = env.get(var) {
    //     let tail = if segs.len() > 2 {
    //         segs[2..]
    //             .iter()
    //             .map(|s| s.trim_start_matches('.'))
    //             .collect::<Vec<_>>()
    //             .join(".")
    //     } else {
    //         String::new()
    //     };
    //     for base in bases {
    //         let key = if tail.is_empty() {
    //             base.clone()
    //         } else {
    //             format!("{base}.{tail}")
    //         };
    //         if !key.is_empty() {
    //             out.insert(key);
    //         }
    //     }
    //     continue;
    // } else {
    //     eprintln!(
    //         "{}",
    //         diagnostics::pretty_error("todo", src, n.byte_range(), None,)
    //     );
    //     panic!(
    //         "unbound template variable ${var} used in selector at bytes {:?}",
    //         n.byte_range()
    //     );
    // }

    // THIS WAS BEFORE:
    // "selector_expression" => {
    //     // Robustly extract segments, then re-introduce the syntactic prefix we care about
    //     // so tests can assert the normalized form:
    //     //   .Values.foo   => Selector([".", "Values", "foo"])
    //     //   .Values       => Selector([".", "Values"])
    //     //   (.Values.foo) => Selector([".", "Values", "foo"])
    //     let text = &src[node.byte_range()];
    //     let trimmed = text.trim();
    //
    //     if let Some(mut segs) =
    //         helm_schema_template::values::parse_selector_expression(&node, src)
    //     {
    //         let mut out = Vec::<String>::new();
    //
    //         // If the original text starts with '.', we want to preserve that as a prefix token.
    //         if trimmed.starts_with('.') {
    //             out.push(".".to_string());
    //
    //             // If it specifically starts with ".Values", ensure "Values" immediately follows.
    //             // parse_selector_expression() may or may not include "Values" as the first segment,
    //             // depending on grammar/tokenization; normalize either way.
    //             if trimmed.starts_with(".Values") {
    //                 // Avoid duplicating "Values"
    //                 if !segs.is_empty() && segs[0] == "Values" {
    //                     // keep segs as-is; we'll just add "."
    //                 } else {
    //                     out.push("Values".to_string());
    //                 }
    //             }
    //         }
    //
    //         // If we already injected ".Values", drop a redundant leading "Values" from segs.
    //         if out.len() >= 2 && out[0] == "." && out[1] == "Values" {
    //             if !segs.is_empty() && segs[0] == "Values" {
    //                 segs.remove(0);
    //             }
    //         }
    //
    //         out.extend(segs);
    //         ArgExpr::Selector(out)
    //     } else {
    //         ArgExpr::Opaque
    //     }
    // }

    // "selector_expression" => {
    //     // OUTERMOST selector only
    //     let parent_is_selector = n
    //         .parent()
    //         .map(|p| p.kind() == "selector_expression")
    //         .unwrap_or(false);
    //     if !parent_is_selector {
    //         if let Some(segs0) = parse_selector_chain(n, src) {
    //             let segs = normalize_segments(&segs0);
    //             if segs.len() >= 2
    //                 && segs[0] == "."
    //                 && segs[1] == "Values"
    //                 && segs.len() > 2
    //             {
    //                 values.insert(segs[2..].join("."));
    //             } else if segs.len() >= 2
    //                 && segs[0] == "$"
    //                 && segs[1] == "Values"
    //                 && segs.len() > 2
    //             {
    //                 values.insert(segs[2..].join("."));
    //             } else if !segs.is_empty() && segs[0] == "Values" && segs.len() > 1 {
    //                 values.insert(segs[1..].join("."));
    //             }
    //         }
    //     }
    // }
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

    // "selector_expression" => {
    //     let parent_is_selector = n
    //         .parent()
    //         .map(|p| p.kind() == "selector_expression")
    //         .unwrap_or(false);
    //     if !parent_is_selector {
    //         if let Some(segs0) = parse_selector_chain(n, src) {
    //             let segs = normalize_segments(&segs0);
    //             // Keep only .Values.* and strip the "Values" root for the returned key
    //             if segs.len() >= 2
    //                 && segs[0] == "."
    //                 && segs[1] == "Values"
    //                 && segs.len() > 2
    //             {
    //                 out.insert(segs[2..].join("."));
    //             } else if segs.len() >= 2
    //                 && segs[0] == "$"
    //                 && segs[1] == "Values"
    //                 && segs.len() > 2
    //             {
    //                 out.insert(segs[2..].join("."));
    //             } else if !segs.is_empty() && segs[0] == "Values" && segs.len() > 1 {
    //                 out.insert(segs[1..].join("."));
    //             }
    //         }
    //     }
    // }

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

    // let role = if ph.is_fragment_output {
    //     Role::Fragment
    // } else {
    //     binding.role
    // };

    // if crate::sanitize::is_assignment_kind(ch.kind()) {
    //     let vals =
    //         collect_values_following_includes(ch, src, use_scope, inline, guard, &out.env);
    //
    //     let mut vc = ch.walk();
    //     for sub in ch.children(&mut vc) {
    //         if sub.is_named() && sub.kind() == "variable" {
    //             if let Some(name) = variable_ident_of(&sub, src) {
    //                 out.env.insert(name, vals.clone());
    //             }
    //             break;
    //         }
    //     }
    //     continue;
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

    // pub fn build_sanitized_with_placeholders(
    //     src: &str,
    //     gtree: &tree_sitter::Tree,
    //     out_placeholders: &mut Vec<Placeholder>,
    //     collect_values: impl Fn(&Node) -> Vec<String> + Copy,
    // ) -> String {
    //     let mut next_id = 0usize;
    //
    //     // Variable environment: var name -> Values set
    //     type Env = BTreeMap<String, BTreeSet<String>>;
    //     let mut env: Env = Env::new();
    //
    //     fn emit_placeholder_for(
    //         node: tree_sitter::Node,
    //         src: &str,
    //         buf: &mut String,
    //         next_id: &mut usize,
    //         out: &mut Vec<Placeholder>,
    //         collect_values: impl Fn(&Node) -> Vec<String> + Copy,
    //         env: &BTreeMap<String, BTreeSet<String>>,
    //         is_fragment_output: bool,
    //     ) {
    //         let id = *next_id;
    //         *next_id += 1;
    //         buf.push('"');
    //         buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
    //         buf.push('"');
    //
    //         let mut values: Vec<String> = if node.kind() == "variable" {
    //             // Pull values from env if we know the variable
    //             if let Some(name) = variable_ident(&node, src) {
    //                 env.get(&name)
    //                     .map(|s| s.iter().cloned().collect())
    //                     .unwrap_or_default()
    //             } else {
    //                 Vec::new()
    //             }
    //         } else {
    //             collect_values(&node)
    //         };
    //         out.push(Placeholder {
    //             id,
    //             role: Role::Unknown, // decided later from YAML AST
    //             action_span: node.byte_range(),
    //             values,
    //             is_fragment_output,
    //         });
    //     }
    //
    //     fn walk(
    //         node: Node,
    //         src: &str,
    //         buf: &mut String,
    //         next_id: &mut usize,
    //         out: &mut Vec<Placeholder>,
    //         collect_values: impl Fn(&Node) -> Vec<String> + Copy,
    //         env: &mut BTreeMap<String, BTreeSet<String>>,
    //     ) {
    //         // 1) pass through raw text
    //         if node.kind() == "text" {
    //             buf.push_str(&src[node.byte_range()]);
    //             return;
    //         }
    //
    //         // 2) containers: descend into their DIRECT children only
    //         if is_container(node.kind()) || is_control_flow(node.kind()) {
    //             let mut c = node.walk();
    //             let parent_is_define = node.kind() == "define_action";
    //             for ch in node.children(&mut c) {
    //                 let parent_is_define = node.kind() == "define_action";
    //
    //                 if !ch.is_named() {
    //                     continue;
    //                 }
    //                 if ch.kind() == "text" {
    //                     // Do NOT emit raw text from define bodies — keeps sanitized YAML valid
    //                     if !parent_is_define {
    //                         buf.push_str(&src[ch.byte_range()]);
    //                     }
    //                     continue;
    //                 }
    //
    //                 // // RECORD guards (no YAML emission)
    //                 if is_control_flow(node.kind()) && is_guard_child(&ch) {
    //                     let id = *next_id;
    //                     *next_id += 1;
    //                     // let values = collect_values(&ch);
    //                     // out.push(Placeholder {
    //                     //     id,
    //                     //     role: Role::Guard,
    //                     //     action_span: ch.byte_range(),
    //                     //     values,
    //                     //     is_fragment_output: false,
    //                     // });
    //                     continue;
    //                 }
    //
    //                 if is_control_flow(ch.kind()) || is_container(ch.kind()) {
    //                     // Nested container (e.g., else_clause); recurse
    //                     walk(ch, src, buf, next_id, out, collect_values, env);
    //                 } else if is_output_expr_kind(ch.kind()) {
    //                     // single placeholder for this direct child expression
    //                     let frag = looks_like_fragment_output(&ch, src);
    //                     if parent_is_define || frag {
    //                         // Record the placeholder, but DO NOT write anything into the buffer.
    //                         // This keeps the surrounding YAML valid.
    //                         let id = *next_id;
    //                         *next_id += 1;
    //
    //                         let values = if ch.kind() == "variable" {
    //                             if let Some(name) = variable_ident(&ch, src) {
    //                                 env.get(&name)
    //                                     .map(|s| s.iter().cloned().collect())
    //                                     .unwrap_or_default()
    //                             } else {
    //                                 Vec::new()
    //                             }
    //                         } else {
    //                             collect_values(&ch)
    //                         };
    //                         out.push(Placeholder {
    //                             id,
    //                             role: Role::Fragment,
    //                             action_span: ch.byte_range(),
    //                             values,
    //                             is_fragment_output: true,
    //                         });
    //
    //                         // // In define bodies: record the use, but DO NOT write to buf
    //                         // let id = *next_id;
    //                         // *next_id += 1;
    //                         // let values = if ch.kind() == "variable" {
    //                         //     if let Some(name) = variable_ident(&ch, src) {
    //                         //         env.get(&name)
    //                         //             .map(|s| s.iter().cloned().collect())
    //                         //             .unwrap_or_default()
    //                         //     } else {
    //                         //         Vec::new()
    //                         //     }
    //                         // } else {
    //                         //     collect_values(&ch)
    //                         // };
    //                         // out.push(Placeholder {
    //                         //     id,
    //                         //     role: Role::Fragment, // define content has no concrete YAML site
    //                         //     action_span: ch.byte_range(),
    //                         //     values,
    //                         //     is_fragment_output: frag,
    //                         // });
    //
    //                         // IMPORTANT: only write to the buffer when not suppressing define body text
    //                         if !parent_is_define {
    //                             let slot = current_slot_in_buf(buf);
    //                             write_fragment_placeholder(buf, id, slot);
    //                         }
    //                     } else {
    //                         // Normal template files: emit placeholder into YAML
    //                         emit_placeholder_for(ch, src, buf, next_id, out, collect_values, env, frag);
    //                     }
    //                 } else if ch.kind() == "ERROR" {
    //                     // These commonly carry indentation/spacing that YAML needs.
    //                     // Preserve them verbatim.
    //                     // buf.push_str(&src[ch.byte_range()]);
    //                     // skip (often whitespace artifacts)
    //                     continue;
    //                 } else if is_assignment_node(&ch, src) {
    //                     // RECORD assignment uses (no YAML emission)
    //                     let id = *next_id;
    //                     *next_id += 1;
    //                     let values = collect_values(&ch);
    //                     out.push(Placeholder {
    //                         id,
    //                         role: Role::Fragment, // records a use; no concrete YAML location
    //                         action_span: ch.byte_range(),
    //                         values: values.clone(),
    //                         is_fragment_output: false,
    //                     });
    //                     // Try to extract `$var` name and bind collected values
    //                     // child "variable" holds the LHS
    //                     let mut vc = ch.walk();
    //                     for sub in ch.children(&mut vc) {
    //                         if sub.is_named() && sub.kind() == "variable" {
    //                             if let Some(name) = variable_ident(&sub, src) {
    //                                 let mut set = BTreeSet::<String>::new();
    //                                 set.extend(values.into_iter());
    //                                 env.insert(name, set);
    //                             }
    //                             break;
    //                         }
    //                     }
    //                     continue;
    //                 } else {
    //                     // Unknown non-output node at container level — skip to keep YAML valid.
    //                     continue;
    //                 }
    //             }
    //             return;
    //         }
    //
    //         // 3) non-container reached (shouldn’t happen for well-formed trees, but safe fallback)
    //         if is_output_expr_kind(node.kind()) {
    //             let frag = looks_like_fragment_output(&node, src);
    //             emit_placeholder_for(node, src, buf, next_id, out, collect_values, env, frag);
    //         } else {
    //             unreachable!("should not happen?");
    //             // trivia/unknown — pass through
    //             buf.push_str(&src[node.byte_range()]);
    //         }
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
    //         &mut env,
    //     );
    //     out
    // }/ pub fn build_sanitized_with_placeholders(
    //     src: &str,
    //     gtree: &tree_sitter::Tree,
    //     out_placeholders: &mut Vec<Placeholder>,
    //     collect_values: impl Fn(&Node) -> Vec<String> + Copy,
    // ) -> String {
    //     let mut next_id = 0usize;
    //
    //     // Variable environment: var name -> Values set
    //     type Env = BTreeMap<String, BTreeSet<String>>;
    //     let mut env: Env = Env::new();
    //
    //     fn emit_placeholder_for(
    //         node: tree_sitter::Node,
    //         src: &str,
    //         buf: &mut String,
    //         next_id: &mut usize,
    //         out: &mut Vec<Placeholder>,
    //         collect_values: impl Fn(&Node) -> Vec<String> + Copy,
    //         env: &BTreeMap<String, BTreeSet<String>>,
    //         is_fragment_output: bool,
    //     ) {
    //         let id = *next_id;
    //         *next_id += 1;
    //         buf.push('"');
    //         buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
    //         buf.push('"');
    //
    //         let mut values: Vec<String> = if node.kind() == "variable" {
    //             // Pull values from env if we know the variable
    //             if let Some(name) = variable_ident(&node, src) {
    //                 env.get(&name)
    //                     .map(|s| s.iter().cloned().collect())
    //                     .unwrap_or_default()
    //             } else {
    //                 Vec::new()
    //             }
    //         } else {
    //             collect_values(&node)
    //         };
    //         out.push(Placeholder {
    //             id,
    //             role: Role::Unknown, // decided later from YAML AST
    //             action_span: node.byte_range(),
    //             values,
    //             is_fragment_output,
    //         });
    //     }
    //
    //     fn walk(
    //         node: Node,
    //         src: &str,
    //         buf: &mut String,
    //         next_id: &mut usize,
    //         out: &mut Vec<Placeholder>,
    //         collect_values: impl Fn(&Node) -> Vec<String> + Copy,
    //         env: &mut BTreeMap<String, BTreeSet<String>>,
    //     ) {
    //         // 1) pass through raw text
    //         if node.kind() == "text" {
    //             buf.push_str(&src[node.byte_range()]);
    //             return;
    //         }
    //
    //         // 2) containers: descend into their DIRECT children only
    //         if is_container(node.kind()) || is_control_flow(node.kind()) {
    //             let mut c = node.walk();
    //             let parent_is_define = node.kind() == "define_action";
    //             for ch in node.children(&mut c) {
    //                 let parent_is_define = node.kind() == "define_action";
    //
    //                 if !ch.is_named() {
    //                     continue;
    //                 }
    //                 if ch.kind() == "text" {
    //                     // Do NOT emit raw text from define bodies — keeps sanitized YAML valid
    //                     if !parent_is_define {
    //                         buf.push_str(&src[ch.byte_range()]);
    //                     }
    //                     continue;
    //                 }
    //
    //                 // // RECORD guards (no YAML emission)
    //                 if is_control_flow(node.kind()) && is_guard_child(&ch) {
    //                     let id = *next_id;
    //                     *next_id += 1;
    //                     // let values = collect_values(&ch);
    //                     // out.push(Placeholder {
    //                     //     id,
    //                     //     role: Role::Guard,
    //                     //     action_span: ch.byte_range(),
    //                     //     values,
    //                     //     is_fragment_output: false,
    //                     // });
    //                     continue;
    //                 }
    //
    //                 if is_control_flow(ch.kind()) || is_container(ch.kind()) {
    //                     // Nested container (e.g., else_clause); recurse
    //                     walk(ch, src, buf, next_id, out, collect_values, env);
    //                 } else if is_output_expr_kind(ch.kind()) {
    //                     // single placeholder for this direct child expression
    //                     let frag = looks_like_fragment_output(&ch, src);
    //                     if parent_is_define || frag {
    //                         // Record the placeholder, but DO NOT write anything into the buffer.
    //                         // This keeps the surrounding YAML valid.
    //                         let id = *next_id;
    //                         *next_id += 1;
    //
    //                         let values = if ch.kind() == "variable" {
    //                             if let Some(name) = variable_ident(&ch, src) {
    //                                 env.get(&name)
    //                                     .map(|s| s.iter().cloned().collect())
    //                                     .unwrap_or_default()
    //                             } else {
    //                                 Vec::new()
    //                             }
    //                         } else {
    //                             collect_values(&ch)
    //                         };
    //                         out.push(Placeholder {
    //                             id,
    //                             role: Role::Fragment,
    //                             action_span: ch.byte_range(),
    //                             values,
    //                             is_fragment_output: true,
    //                         });
    //
    //                         // // In define bodies: record the use, but DO NOT write to buf
    //                         // let id = *next_id;
    //                         // *next_id += 1;
    //                         // let values = if ch.kind() == "variable" {
    //                         //     if let Some(name) = variable_ident(&ch, src) {
    //                         //         env.get(&name)
    //                         //             .map(|s| s.iter().cloned().collect())
    //                         //             .unwrap_or_default()
    //                         //     } else {
    //                         //         Vec::new()
    //                         //     }
    //                         // } else {
    //                         //     collect_values(&ch)
    //                         // };
    //                         // out.push(Placeholder {
    //                         //     id,
    //                         //     role: Role::Fragment, // define content has no concrete YAML site
    //                         //     action_span: ch.byte_range(),
    //                         //     values,
    //                         //     is_fragment_output: frag,
    //                         // });
    //
    //                         // IMPORTANT: only write to the buffer when not suppressing define body text
    //                         if !parent_is_define {
    //                             let slot = current_slot_in_buf(buf);
    //                             write_fragment_placeholder(buf, id, slot);
    //                         }
    //                     } else {
    //                         // Normal template files: emit placeholder into YAML
    //                         emit_placeholder_for(ch, src, buf, next_id, out, collect_values, env, frag);
    //                     }
    //                 } else if ch.kind() == "ERROR" {
    //                     // These commonly carry indentation/spacing that YAML needs.
    //                     // Preserve them verbatim.
    //                     // buf.push_str(&src[ch.byte_range()]);
    //                     // skip (often whitespace artifacts)
    //                     continue;
    //                 } else if is_assignment_node(&ch, src) {
    //                     // RECORD assignment uses (no YAML emission)
    //                     let id = *next_id;
    //                     *next_id += 1;
    //                     let values = collect_values(&ch);
    //                     out.push(Placeholder {
    //                         id,
    //                         role: Role::Fragment, // records a use; no concrete YAML location
    //                         action_span: ch.byte_range(),
    //                         values: values.clone(),
    //                         is_fragment_output: false,
    //                     });
    //                     // Try to extract `$var` name and bind collected values
    //                     // child "variable" holds the LHS
    //                     let mut vc = ch.walk();
    //                     for sub in ch.children(&mut vc) {
    //                         if sub.is_named() && sub.kind() == "variable" {
    //                             if let Some(name) = variable_ident(&sub, src) {
    //                                 let mut set = BTreeSet::<String>::new();
    //                                 set.extend(values.into_iter());
    //                                 env.insert(name, set);
    //                             }
    //                             break;
    //                         }
    //                     }
    //                     continue;
    //                 } else {
    //                     // Unknown non-output node at container level — skip to keep YAML valid.
    //                     continue;
    //                 }
    //             }
    //             return;
    //         }
    //
    //         // 3) non-container reached (shouldn’t happen for well-formed trees, but safe fallback)
    //         if is_output_expr_kind(node.kind()) {
    //             let frag = looks_like_fragment_output(&node, src);
    //             emit_placeholder_for(node, src, buf, next_id, out, collect_values, env, frag);
    //         } else {
    //             unreachable!("should not happen?");
    //             // trivia/unknown — pass through
    //             buf.push_str(&src[node.byte_range()]);
    //         }
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
    //         &mut env,
    //     );
    //     out
    // }

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
    //     // NEW: stay in mapping if the previous non-empty line is a placeholder mapping entry
    //     // e.g.   "  "__TSG_PLACEHOLDER_3__": 0"
    //     if prev_trim.contains("__TSG_PLACEHOLDER_") && prev_trim.contains("\": 0") {
    //         return Slot::MappingValue;
    //     }
    //
    //     Slot::Plain
    // }

    // fn write_fragment_placeholder(out: &mut String, id: usize, slot: Slot) {
    //     use std::fmt::Write as _;
    //     match slot {
    //         Slot::MappingValue => {
    //             // becomes an entry *inside* the surrounding block mapping,
    //             // can repeat multiple times safely:
    //             let _ = write!(out, "\"__TSG_PLACEHOLDER_{id}__\": 0\n");
    //         }
    //         Slot::SequenceItem => {
    //             // fill the list item in-line, do NOT add a leading '- ' here; it's already present
    //             let _ = write!(out, "\"__TSG_PLACEHOLDER_{id}__\"\n");
    //         }
    //         Slot::Plain => {
    //             // standalone scalar is fine anywhere else
    //             let _ = write!(out, "\"__TSG_PLACEHOLDER_{id}__\"\n");
    //         }
    //     }
    // }

    // Only these node kinds should be replaced by a single placeholder
    fn is_output_expr_kind(kind: &str) -> bool {
        matches!(
            kind,
            "selector_expression" | "function_call" | "chained_pipeline" | "variable" | "dot"
        )
    }

    fn variable_ident(node: &Node, src: &str) -> Option<String> {
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
        // fallback: strip leading '$'
        node.utf8_text(src.as_bytes())
            .ok()
            .map(|t| t.trim().trim_start_matches('$').to_string())
    }

    fn function_name<'a>(node: &tree_sitter::Node<'a>, src: &str) -> Option<String> {
        if node.kind() != "function_call" {
            return None;
        }
        let f = node.child_by_field_name("function")?;
        Some(f.utf8_text(src.as_bytes()).ok()?.to_string())
    }

    fn looks_like_fragment_output(node: &tree_sitter::Node, src: &str) -> bool {
        match node.kind() {
            "function_call" => {
                if let Some(name) = function_name(node, src) {
                    // These usually render mappings/sequences, not scalars
                    // return matches!(name.as_str(), "toYaml");
                    return name == "include" || name == "toYaml";
                }
            }
            "chained_pipeline" => {
                // If any function in the chain is a known fragment producer, treat as fragment
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() && ch.kind() == "function_call" {
                        if let Some(name) = function_name(&ch, src) {
                            if name == "include" || name == "toYaml" {
                                // if matches!(name.as_str(), "toYaml") {
                                return true;
                            }
                        }
                    }
                }
            }
            _ => {}
        };
        false
    }

    fn is_assignment_node(node: &Node, src: &str) -> bool {
        if is_assignment_kind(node.kind()) {
            return true;
        }
        // fallback: grammar differences — detect ':=' in the action text
        let t = &src[node.byte_range()];
        t.contains(":=")
    }

    /// Is `node` a guard child inside a control-flow action? (i.e. appears before the first text body)
    fn is_guard_child(node: &Node) -> bool {
        let Some(parent) = node.parent() else {
            return false;
        };
        if !is_control_flow(parent.kind()) {
            return false;
        }

        // find first named `text` child in the control-flow node
        let mut c = parent.walk();
        let mut first_text_start: Option<usize> = None;
        for ch in parent.children(&mut c) {
            if ch.is_named() && ch.kind() == "text" {
                first_text_start = Some(ch.start_byte());
                break;
            }
        }
        match first_text_start {
            Some(body_start) => node.end_byte() <= body_start,
            None => true, // no text body at all → everything is guard-ish
        }
    }
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
