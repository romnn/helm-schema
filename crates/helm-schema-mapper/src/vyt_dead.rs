// fn snapshot(&self) -> Vec<String> {
//     self.active.clone()
// }

// if is_control_flow(node.kind()) {
//     // Collect guard(s) from header area (everything before first text)
//     self.collect_guards_from(node);
//     // Now walk into children (body)
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             self.walk(ch);
//         }
//     }
//     // Pop the guards we pushed in collect_guards_from()
//     // (We pushed possibly multiple during selection parsing; pop same count.)
//     // For the probe, we just clear back to previous size by counting ‘;’ separator we used.
//     while let Some(last) = self.guards.active.last() {
//         if last.ends_with("\u{0}") {
//             // sentinel we inserted below
//             self.guards.pop(); // remove sentinel
//             break;
//         }
//         self.guards.pop();
//     }
//     return;
// }

// "selector_expression"
// | "field"
// | "dot"
// | "variable"
// | "function_call"
// | "chained_pipeline" => {
//     self.handle_output(node);
//     // still traverse; nested stuff inside may carry guards etc.
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             self.walk(ch);
//         }
//     }
// }

// fn collect_guards_from(&mut self, node: Node) {
//     let mut pushed_any = false;
//
//     match node.kind() {
//         "if_action" | "with_action" | "range_action" => {
//             let mut w = node.walk();
//             for ch in node.children(&mut w) {
//                 if !ch.is_named() {
//                     continue;
//                 }
//                 // NEW: ignore trim/spacing artifacts
//                 if ch.kind() == "ERROR" || ch.kind() == "text" {
//                     continue;
//                 }
//
//                 // Treat the first non-text/non-error child as the header
//                 let keys = collect_values_anywhere(ch, self.source);
//                 for k in keys {
//                     if !k.is_empty() {
//                         self.guards.push(k);
//                         pushed_any = true;
//                     }
//                 }
//                 break; // only the header
//             }
//         }
//         _ => {}
//     }
//
//     if pushed_any {
//         self.guards.push("\u{0}".to_string()); // sentinel
//     }
// }

// fn collect_guards_from(&mut self, node: Node) {
//     let mut pushed_any = false;
//
//     match node.kind() {
//         // For if/with/range, the first named child is the condition/iterable.
//         "if_action" | "with_action" | "range_action" => {
//             let mut w = node.walk();
//             for ch in node.children(&mut w) {
//                 if !ch.is_named() {
//                     continue;
//                 }
//                 // Take ONLY the first named child as the header/condition.
//                 let keys = collect_values_anywhere(ch, self.source);
//                 for k in keys {
//                     if !k.is_empty() {
//                         self.guards.push(k);
//                         pushed_any = true;
//                     }
//                 }
//                 break; // do not include body selectors
//             }
//         }
//         // else/define: no guards to capture
//         _ => {}
//     }
//
//     if pushed_any {
//         // push a sentinel so we can pop these guards after the body
//         self.guards.push("\u{0}".to_string());
//     }
// }

// fn collect_guards_from(&mut self, node: Node) {
//     let body_start = first_text_start(node);
//     let mut pushed_any = false;
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if !ch.is_named() {
//             continue;
//         }
//         if let Some(b) = body_start {
//             if ch.start_byte() >= b {
//                 break;
//             }
//         }
//         // scan this header subtree for .Values.* selectors
//         let keys = collect_values_anywhere(ch, self.source);
//         for k in keys {
//             if !k.is_empty() {
//                 self.guards.push(k);
//                 pushed_any = true;
//             }
//         }
//     }
//     if pushed_any {
//         // push a sentinel so we know where to stop popping later
//         self.guards.push("\u{0}".to_string());
//     }
// }

// fn collect_values_anywhere(node: Node, src: &str) -> BTreeSet<String> {
//     let mut out = BTreeSet::new();
//     let mut stack = vec![node];
//
//     // Pre-compile once per call; cheap enough for tests. If you want, hoist to lazy_static.
//     // Matches things like:
//     //   .Values.foo
//     //   $.Values.foo.bar
//     //   Values.foo   (very rare in tpl, but cheap to support)
//     let re = Regex::new(
//         r"(?x)
//         (?:\.\s*|\$\s*)?Values  # optional dot or dollar, then 'Values'
//         (?:\.[A-Za-z_][A-Za-z0-9_]*)+  # one or more .ident segments
//     ",
//     )
//     .unwrap();
//
//     while let Some(n) = stack.pop() {
//         // 1) Preferred: structured parse when the node is a selector_expression
//         if n.kind() == "selector_expression" {
//             if let Some(segs) =
//                 parse_selector_expression(&n, src).or_else(|| parse_selector_chain(n, src))
//             {
//                 if !segs.is_empty() {
//                     if segs[0] == "Values" && segs.len() > 1 {
//                         out.insert(segs[1..].join("."));
//                     } else if segs[0] == "." && segs.len() > 2 && segs[1] == "Values" {
//                         out.insert(segs[2..].join("."));
//                     } else if segs[0] == "$" && segs.len() > 2 && segs[1] == "Values" {
//                         out.insert(segs[2..].join("."));
//                     }
//                 }
//             }
//         }
//
//         // 2) Fallback: text scan for .Values.* anywhere under this node
//         if let Ok(txt) = n.utf8_text(src.as_bytes()) {
//             for m in re.find_iter(txt) {
//                 // example hits: ".Values.labels", "$.Values.ingress", "Values.name"
//                 let hit = m.as_str();
//                 // Normalize to segments, drop leading "." or "$" and the "Values" head
//                 // Then keep the remainder joined by dots.
//                 // Split conservatively: periods separate identifiers, ignore whitespace after . or $ which regex already handled.
//                 let mut parts: Vec<&str> = hit.split('.').collect();
//                 // parts may look like ["", "Values", "labels"] or ["$", "Values", "ingress", ...] or ["Values", "name"]
//                 while !parts.is_empty()
//                     && (parts[0].trim().is_empty() || parts[0] == "$" || parts[0] == "")
//                 {
//                     parts.remove(0);
//                 }
//                 if !parts.is_empty() && parts[0] == "Values" {
//                     parts.remove(0);
//                 }
//                 if !parts.is_empty() {
//                     out.insert(parts.join("."));
//                 }
//             }
//         }
//
//         // DFS
//         let mut c = n.walk();
//         for ch in n.children(&mut c) {
//             if ch.is_named() {
//                 stack.push(ch);
//             }
//         }
//     }
//
//     out
// }

// /// Collect `.Values.*` value paths reachable anywhere under `node`.
// /// Uses your robust chain parsers; normalizes to "a.b.c" without the "Values." prefix.
// fn collect_values_anywhere(node: Node, src: &str) -> BTreeSet<String> {
//     let mut out = BTreeSet::new();
//     let mut stack = vec![node];
//     while let Some(n) = stack.pop() {
//         match n.kind() {
//             "selector_expression" => {
//                 if let Some(segs) =
//                     parse_selector_expression(&n, src).or_else(|| parse_selector_chain(n, src))
//                 {
//                     if !segs.is_empty() {
//                         if segs[0] == "Values" && segs.len() > 1 {
//                             out.insert(segs[1..].join("."));
//                         } else if segs[0] == "."
//                             && segs.len() > 1
//                             && segs[1] == "Values"
//                             && segs.len() > 2
//                         {
//                             out.insert(segs[2..].join("."));
//                         } else if segs[0] == "$"
//                             && segs.len() > 1
//                             && segs[1] == "Values"
//                             && segs.len() > 2
//                         {
//                             out.insert(segs[2..].join("."));
//                         }
//                     }
//                 }
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
//     out
// }

// fn collect_values_anywhere(node: Node, src: &str) -> BTreeSet<String> {
//     let mut out = BTreeSet::new();
//     let mut stack = vec![node];
//     while let Some(n) = stack.pop() {
//         match n.kind() {
//             "selector_expression" => {
//                 // 1) Structural parse first
//                 dbg!(parse_selector_expression(&n, src));
//                 dbg!(parse_selector_chain(n, src));
//                 if let Some(segs) = parse_selector_chain(n, src)
//                 // parse_selector_expression(&n, src).or_else(|| parse_selector_chain(n, src))
//                 {
//                     if !segs.is_empty() {
//                         if segs[0] == "Values" && segs.len() > 1 {
//                             out.insert(segs[1..].join("."));
//                         } else if segs[0] == "." && segs.len() > 2 && segs[1] == "Values" {
//                             out.insert(segs[2..].join("."));
//                         } else if segs[0] == "$" && segs.len() > 2 && segs[1] == "Values" {
//                             out.insert(segs[2..].join("."));
//                         }
//                     }
//                 }
//
//                 // 2) NEW: textual fallback for nested chains like ".Values.labels.app"
//                 if out.is_empty() {
//                     if let Ok(text) = n.utf8_text(src.as_bytes()) {
//                         let t = text.trim();
//                         // Normalize well-known roots
//                         for prefix in &[".Values.", "$.Values.", "Values."] {
//                             if let Some(rest) = t.strip_prefix(prefix) {
//                                 // e.g. "labels.app"
//                                 if !rest.is_empty()
//                                     && !rest.starts_with(|c: char| c == ' ' || c == '}')
//                                 {
//                                     out.insert(rest.to_string());
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }
//             _ => {}
//         }
//
//         let mut c = n.walk();
//         for ch in n.children(&mut c) {
//             if ch.is_named() {
//                 stack.push(ch);
//             }
//         }
//     }
//     out
// }

// if is_control_flow_kind(node.kind()) {
//     self.collect_guards_from(node);
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             self.walk(ch);
//         }
//     }
//     // Pop guards until sentinel (if we pushed any)
//     while let Some(last) = self.guards.active.last() {
//         if last.ends_with('\u{0}') {
//             self.guards.pop(); // pop sentinel
//             break;
//         }
//         self.guards.pop();
//     }
//     return;
// }

// fn pop(&mut self) {
//     let _ = self.active.pop();
// }
//
// fn snapshot(&self) -> Vec<String> {
//     self.active
//         .iter()
//         .filter(|s| s.as_str() != "\u{0}")
//         .cloned()
//         .collect()
// }

// /// Rebind `.` for a `with` action to the header's .Values.* chain.
// fn open_with_scope_if_any(&mut self, node: Node) -> bool {
//     if node.kind() != "with_action" {
//         return false;
//     }
//
//     // Find the header node (first named, non-text, non-error child)
//     let mut w = node.walk();
//     let mut header: Option<Node> = None;
//     for ch in node.children(&mut w) {
//         if !ch.is_named() {
//             continue;
//         }
//         if ch.kind() == "text" || ch.kind() == "ERROR" {
//             continue;
//         }
//         header = Some(ch);
//         break;
//     }
//     let Some(h) = header else {
//         return false;
//     };
//
//     // Extract the .Values.* chain used in the header
//     let vals: Vec<String> = collect_values_anywhere(h, self.source)
//         .into_iter()
//         .collect();
//     if vals.is_empty() {
//         return false;
//     }
//     // For a with-header we expect exactly one chain; take the first
//     let base = vals[0].clone();
//
//     // Push a scope that only rebinds '.'
//     self.bindings.push_scope(
//         Default::default(),
//         Some(Binding {
//             base,
//             role: BindingRole::Item, // role isn't used for with, but Item is fine
//         }),
//     );
//     true
// }

// if is_control_flow_kind(node.kind()) {
//     // Save/extend guards from header
//     let sp = self.guards.savepoint();
//     self.collect_guards_from(node);
//
//     // Open scopes depending on control-flow kind
//     let opened_range_scope = self.open_range_scope_if_any(node);
//     let opened_with_scope = self.open_with_scope_if_any(node);
//
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             self.walk(ch);
//         }
//     }
//
//     // Close scopes (with-range order doesn't matter here)
//     if opened_with_scope {
//         self.bindings.pop_scope();
//     }
//     if opened_range_scope {
//         self.bindings.pop_scope();
//     }
//
//     // Restore guards
//     self.guards.restore(sp);
//     return;
// }

// if is_control_flow_kind(node.kind()) {
//     // Save current guards, collect new ones from the header
//     let sp = self.guards.savepoint();
//     self.collect_guards_from(node);
//
//     // Open a range scope if applicable
//     let opened_range_scope = self.open_range_scope_if_any(node);
//
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             self.walk(ch);
//         }
//     }
//
//     // Close range scope (if opened) and restore guards
//     if opened_range_scope {
//         self.bindings.pop_scope();
//     }
//     self.guards.restore(sp);
//     return;
// }

// if is_control_flow_kind(node.kind()) {
//     // Collect guards as before
//     self.collect_guards_from(node);
//
//     // NEW: open a range scope if applicable
//     let opened_range_scope = self.open_range_scope_if_any(node);
//
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             self.walk(ch);
//         }
//     }
//
//     // Close range scope (if opened)
//     if opened_range_scope {
//         self.bindings.pop_scope();
//     }
//
//     // Pop guard sentinel as before
//     while let Some(last) = self.guards.active.last() {
//         if last.ends_with('\u{0}') {
//             self.guards.pop();
//             break;
//         }
//         self.guards.pop();
//     }
//     return;
// }

// if is_control_flow_kind(node.kind()) {
//     let sp = self.guards.savepoint();
//     self.collect_guards_from(node);
//
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             self.walk(ch);
//         }
//     }
//
//     self.guards.restore(sp);
//     return;
// }

// // NEW: only the full selectors and the producers:
// "selector_expression" | "function_call" | "chained_pipeline" => {
//     self.handle_output(node);
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             self.walk(ch);
//         }
//     }
// }

// fn collect_guards_from(&mut self, node: Node) {
//     let mut pushed_any = false;
//
//     match node.kind() {
//         "if_action" | "with_action" | "range_action" => {
//             let mut w = node.walk();
//
//             // Find the header: first named, non-text, non-error child.
//             let mut header: Option<Node> = None;
//             for ch in node.children(&mut w) {
//                 if !ch.is_named() {
//                     continue;
//                 }
//                 if ch.kind() == "text" || ch.kind() == "ERROR" {
//                     continue;
//                 }
//                 header = Some(ch);
//                 break;
//             }
//
//             if let Some(h) = header {
//                 // Prefer collecting from argument_list/parenthesized_pipeline children;
//                 // fall back to the header node if we don't find any.
//                 let mut any_args = false;
//                 let mut hw = h.walk();
//                 for sub in h.children(&mut hw) {
//                     if !sub.is_named() {
//                         continue;
//                     }
//                     if sub.kind() == "argument_list" || sub.kind() == "parenthesized_pipeline" {
//                         let keys = collect_values_anywhere(sub, self.source);
//                         for k in keys {
//                             if !k.is_empty() {
//                                 self.guards.push(k);
//                                 pushed_any = true;
//                                 any_args = true;
//                             }
//                         }
//                     }
//                 }
//
//                 if !any_args {
//                     let keys = collect_values_anywhere(h, self.source);
//                     for k in keys {
//                         if !k.is_empty() {
//                             self.guards.push(k);
//                             pushed_any = true;
//                         }
//                     }
//                 }
//             }
//         }
//         _ => {}
//     }
//
//     if pushed_any {
//         // sentinel to pop after body
//         self.guards.push("\u{0}".to_string());
//     }
// }

// // Decide if this node is a fragment producer
// let is_fragment = pipeline_contains_any(node, self.source, &["toYaml", "fromYaml", "tpl"]);
// let sources = collect_values_anywhere(node, self.source);
// if sources.is_empty() {
//     return;
// }
// let path = self.shape.current_path();
//
// let kind = if is_fragment {
//     VYKind::Fragment
// } else {
//     VYKind::Scalar
// };
// let guards = self.guards.snapshot();
//
// for s in sources {
//     self.uses.push(VYUse {
//         source_expr: s,
//         path: path.clone(),
//         kind,
//         guards: guards.clone(),
//     });
// }

// // dict? → build alias map; else try to set dot binding
// if function_name_of(&n, &self.source).as_deref() == Some("dict") {
//     // scan key/value pairs inside the dict's argument_list
//     let mut w = n.walk();
//     for ch in n.children(&mut w) {
//         if ch.is_named() && ch.kind() == "argument_list" {
//             let mut aw = ch.walk();
//             let args: Vec<Node> = ch.children(&mut aw).filter(|x| x.is_named()).collect();
//             let mut i = 0;
//             while i + 1 < args.len() {
//                 let key_node = &args[i];
//                 let val_node = &args[i + 1];
//                 let mut key: Option<String> = None;
//                 if let Ok(t) = key_node.utf8_text(self.source.as_bytes()) {
//                     let s = t.trim().trim_matches('"').trim_matches('`').to_string();
//                     if !s.is_empty() {
//                         key = Some(s);
//                     }
//                 }
//                 if let Some(k) = key {
//                     if let Some(p) = self.value_path_from_expr(*val_node) {
//                         alias_out.insert(k, p);
//                     }
//                 }
//                 i += 2;
//             }
//         }
//     }
// } else {
//     if let Some(p) = self.value_path_from_expr(n) {
//         *dot_out = Some(Binding {
//             base: p,
//             role: BindingRole::Item,
//         });
//     }
// }

// // Call handle_output for variables too, so {{ $k }} / {{ $v }} are captured.
// "selector_expression" | "function_call" | "chained_pipeline" | "dot" | "variable" => {
//     self.handle_output(node);
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             self.walk(ch);
//         }
//     }
// }

// fn open_range_scope_if_any(&mut self, node: Node) -> bool {
//     if node.kind() != "range_action" {
//         return false;
//     }
//
//     // Find the header node (first named, non-text, non-error child)
//     let mut w = node.walk();
//     let mut header: Option<Node> = None;
//     for ch in node.children(&mut w) {
//         if !ch.is_named() {
//             continue;
//         }
//         if ch.kind() == "text" || ch.kind() == "ERROR" {
//             continue;
//         }
//         header = Some(ch);
//         break;
//     }
//     let Some(h) = header else {
//         return false;
//     };
//
//     let Ok(h_text) = h.utf8_text(self.source.as_bytes()) else {
//         return false;
//     };
//
//     // Expect something like: "$k, $v := .Values.env"  OR "$v := .Values.env"
//     // Very small regex for this test’s shape:
//     let re = regex::Regex::new(
//         r#"(?x)
//             ^
//             (?P<vars>\$[A-Za-z_]\w*(?:\s*,\s*\$[A-Za-z_]\w*)?)
//             \s*:=\s*
//             (?P<expr>.+?)\s*$
//         "#,
//     )
//     .unwrap();
//
//     let Some(caps) = re.captures(h_text.trim()) else {
//         return false;
//     };
//     let vars_raw = caps.name("vars").unwrap().as_str();
//     let expr = caps.name("expr").unwrap().as_str();
//
//     // Extract base after .Values.*  -> take the first segment as the base ("env" here)
//     let base = first_values_base(expr).unwrap_or_else(|| "Values".to_string());
//     let base_first = base.split('.').next().unwrap_or(&base).to_string();
//
//     let mut map = HashMap::new();
//     // Parse $k and $v
//     let mut var_iter = vars_raw.split(',').map(|s| s.trim().to_string());
//     if let Some(v1) = var_iter.next() {
//         // 1 var form binds value
//         map.insert(
//             v1,
//             Binding {
//                 base: base_first.clone(),
//                 role: BindingRole::MapValue,
//             },
//         );
//     }
//     if let Some(v2) = var_iter.next() {
//         // 2 var form: first is key, second is value
//         // fix the first we inserted above:
//         if let Some((first_name, _)) = map.iter().next().map(|(k, v)| (k.clone(), v.clone())) {
//             // replace first as key
//             map.insert(
//                 first_name.clone(),
//                 Binding {
//                     base: base_first.clone(),
//                     role: BindingRole::MapKey,
//                 },
//             );
//         }
//         map.insert(
//             v2,
//             Binding {
//                 base: base_first.clone(),
//                 role: BindingRole::MapValue,
//             },
//         );
//     }
//
//     // In Go templates, '.' inside range is rebound to the iterated element (value)
//     let dot = Some(Binding {
//         base: base_first,
//         role: BindingRole::MapValue,
//     });
//
//     self.bindings.push_scope(map, dot);
//     true
// }

// /// Feed raw text; update container stack. VERY small subset (good for smoke tests).
// fn ingest(&mut self, text: &str) {
//     for raw in text.split_inclusive('\n') {
//         let line = raw.trim_end_matches('\n');
//         let indent = line.chars().take_while(|&c| c == ' ').count();
//         let after = &line[indent..];
//
//         // Dedent
//         while let Some((top_indent, _, _)) = self.stack.last().cloned() {
//             if top_indent > indent {
//                 self.stack.pop();
//             } else {
//                 break;
//             }
//         }
//
//         let trimmed = after.trim();
//         if trimmed.is_empty() || trimmed.starts_with("{{") {
//             // ignore blank/template lines for shape
//             continue;
//         }
//
//         if after.starts_with("- ") {
//             // At this indent we are definitely inside a sequence.
//             match self.stack.last_mut() {
//                 Some((top_indent, Container::Sequence, pending)) if *top_indent == indent => {
//                     *pending = None; // a new item; keep as sequence
//                 }
//                 Some((top_indent, Container::Mapping, _)) if *top_indent == indent => {
//                     // mapping at this level flips to sequence? Treat as child: push new seq level
//                     self.stack.push((indent, Container::Sequence, None));
//                 }
//                 Some((top_indent, _, _)) if *top_indent < indent => {
//                     // sequence nested under parent
//                     self.stack.push((indent, Container::Sequence, None));
//                 }
//                 None => {
//                     self.stack.push((indent, Container::Sequence, None));
//                 }
//                 _ => {}
//             }
//             // If the line is like "- key: value", recognize an immediate child mapping
//             if let Some(colon) = after.find(':') {
//                 // "- key: ..." => the key belongs to the MAPPING below this item
//                 let key = after["- ".len()..colon].trim().to_string();
//                 // Enter child mapping one indent deeper (conceptually)
//                 let child_indent = indent + 2;
//                 // ensure a mapping level exists for children
//                 self.stack
//                     .push((child_indent, Container::Mapping, Some(key)));
//             }
//             continue;
//         }
//
//         // Mapping key?
//         if after.ends_with(':') || after.contains(':') {
//             // Heuristic: treat "key:" or "key: value" as mapping at this indent.
//             let key = after.split(':').next().unwrap_or("").trim().to_string();
//             match self.stack.last_mut() {
//                 Some((top_indent, Container::Mapping, pending)) if *top_indent == indent => {
//                     *pending = Some(key);
//                 }
//                 Some((top_indent, _, _)) if *top_indent < indent => {
//                     self.stack.push((indent, Container::Mapping, Some(key)));
//                 }
//                 None => {
//                     self.stack.push((indent, Container::Mapping, Some(key)));
//                 }
//                 _ => {
//                     // if we were a sequence item at same indent, open child mapping
//                     self.stack.push((indent, Container::Mapping, Some(key)));
//                 }
//             }
//             continue;
//         }
//
//         // Any other content at this indent: stabilize current container but don’t change keys.
//         if self.stack.is_empty() {
//             // default to mapping root
//             self.stack.push((indent, Container::Mapping, None));
//         }
//     }
// }
//// fn collect_values_anywhere(node: Node, src: &str) -> BTreeSet<String> {
//     let mut out = BTreeSet::new();
//     let mut stack = vec![node];
//
//     while let Some(n) = stack.pop() {
//         if n.kind() == "selector_expression" {
//             // Try structural parse first (chain is robust and preserves . / $ roots).
//             let mut added_for_this_node = false;
//
//             if let Some(segs) = parse_selector_chain(n, src) {
//                 let segs: Vec<_> = segs.iter().map(|s| s.as_str()).collect();
//                 let rest = match segs.as_slice() {
//                     // .Values.foo.bar
//                     [".", "Values", rest @ ..] if !rest.is_empty() => Some(rest),
//                     // $.Values.foo.bar
//                     ["$", "Values", rest @ ..] if !rest.is_empty() => Some(rest),
//                     // Values.foo.bar
//                     ["Values", rest @ ..] if !rest.is_empty() => Some(rest),
//                     _ => None,
//                 };
//
//                 if let Some(rest) = rest {
//                     out.insert(rest.join("."));
//                     added_for_this_node = true;
//                 }
//             }
//
//             // // Per-node textual fallback (for any odd selector shapes the chain parser doesn’t cover)
//             // if !added_for_this_node {
//             //     if let Ok(text) = n.utf8_text(src.as_bytes()) {
//             //         let t = text.trim();
//             //         for prefix in &[".Values.", "$.Values.", "Values."] {
//             //             if let Some(rest) = t.strip_prefix(prefix) {
//             //                 let rest = rest.trim();
//             //                 if !rest.is_empty() && !rest.starts_with(|c: char| c == ' ' || c == '}')
//             //                 {
//             //                     out.insert(rest.to_string());
//             //                     break;
//             //                 }
//             //             }
//             //         }
//             //     }
//             // }
//         }
//
//         // DFS
//         let mut w = n.walk();
//         for ch in n.children(&mut w) {
//             if ch.is_named() {
//                 stack.push(ch);
//             }
//         }
//     }
//
//     out
// }

// dbg!(node.kind());
// match node.kind() {
//     "if_action" | "with_action" | "range_action" => {
//         // Find header: first named, non-text, non-error child
//         let mut w = node.walk();
//         let header = node
//             .children(&mut w)
//             .find(|ch| ch.is_named() && ch.kind() != "text" && ch.kind() != "ERROR");
//         dbg!(&header);
//
//         if let Some(h) = header {
//             // Collect from the whole header subtree (not just argument_list)
//             let keys = collect_values_anywhere(h, self.source);
//             for k in keys {
//                 if !k.is_empty() {
//                     self.guards.push(k);
//                 }
//             }
//         }
//     }
//     _ => {}
// }
