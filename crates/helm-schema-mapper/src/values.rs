use std::collections::BTreeSet;
use tree_sitter::{Node, Tree, TreeCursor};

/// A `.Values` path referenced from templates, normalized to dot-path form:
///   - `.Values.foo.bar`       -> "foo.bar"
///   - `index .Values "a" "b"` -> "a.b"
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValuePath(pub String);

pub fn push_path(paths: &mut BTreeSet<ValuePath>, segs: &[String]) {
    if segs.is_empty() {
        return;
    }
    // full path
    paths.insert(ValuePath(segs.join(".")));
}

// A selector is terminal when it is NOT the left/operand child of a parent selector_expression
pub fn is_terminal_selector(node: &Node) -> bool {
    if node.kind() != "selector_expression" {
        return false;
    }
    if let Some(parent) = node.parent() {
        if parent.kind() == "selector_expression" {
            if let Some(op) = parent.child_by_field_name("operand") {
                // same node id ⇒ this node is the left side of a longer chain
                if op.id() == node.id() {
                    return false;
                }
            }
        }
    }
    true
}

pub fn parse_index_call(node: &Node, src: &str) -> Option<Vec<String>> {
    debug_assert_eq!(node.kind(), "function_call");

    // 1) Get (identifier, argument_list): try fields, else positional fallback.
    let (ident, args) = match (
        node.child_by_field_name("function"),
        node.child_by_field_name("arguments"),
    ) {
        (Some(f), Some(a)) => (f, a),
        _ => {
            let mut cursor = node.walk();
            let mut it = node.named_children(&mut cursor);
            let f = it.next()?; // identifier
            let a = it.next()?; // argument_list
            (f, a)
        }
    };

    if ident.kind() != "identifier" || ident.utf8_text(src.as_bytes()).ok()? != "index" {
        return None;
    }
    if args.kind() != "argument_list" {
        return None;
    }

    // 2) Collect all named children of the argument_list.
    let mut kids = Vec::new();
    let mut aw = args.walk();
    for ch in args.named_children(&mut aw) {
        kids.push(ch);
    }
    if kids.is_empty() {
        return None;
    }

    // 3) Head must be .Values or a selector rooted at .Values
    let mut segs = match kids[0].kind() {
        "field" => {
            let name = kids[0].child_by_field_name("name")?;
            (name.utf8_text(src.as_bytes()).ok()? == "Values").then(|| Vec::<String>::new())
        }
        "selector_expression" => parse_selector_expression(&kids[0], src),
        _ => None,
    }?;

    // 4) Remaining args become path segments (support raw + interpreted strings, and idents).
    for ch in kids.into_iter().skip(1) {
        match ch.kind() {
            "interpreted_string_literal" | "raw_string_literal" => {
                let raw = ch.utf8_text(src.as_bytes()).ok()?;
                let seg = raw
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim_matches('`')
                    .to_string();
                if !seg.is_empty() {
                    segs.push(seg);
                }
            }
            "identifier" | "field_identifier" => {
                segs.push(ch.utf8_text(src.as_bytes()).ok()?.to_string());
            }
            _ => {}
        }
    }

    if segs.is_empty() { None } else { Some(segs) }
}

pub fn extract_values_paths(tree: &Tree, src: &str) -> BTreeSet<ValuePath> {
    let mut paths = BTreeSet::<ValuePath>::new();
    let root = tree.root_node();

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "selector_expression" && is_terminal_selector(&node) {
            if let Some(segs) = parse_selector_expression(&node, src) {
                push_path(&mut paths, &segs);
            }
        }

        if node.kind() == "function_call" {
            if let Some(segs) = parse_index_call(&node, src) {
                push_path(&mut paths, &segs);
            }
        }

        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }
    paths
}

pub fn normalize_segments(raw: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(raw.len() + 4);
    for s in raw {
        if s == "." || s == "$" || s == "Values" {
            out.push(s.clone());
            continue;
        }
        // $cfg → ["$", "cfg"]
        if s.starts_with('$') && s.len() > 1 {
            out.push("$".to_string());
            out.push(s[1..].to_string());
            continue;
        }
        // .config → [".", "config"]
        if s.starts_with('.') {
            out.push(".".to_string());
            let trimmed = s.trim_start_matches('.');
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
            continue;
        }
        out.push(s.clone());
    }
    out
}

// only used in collect_define_info and collect_values_in_subtree
pub fn parse_selector_expression(node: &Node, src: &str) -> Option<Vec<String>> {
    // Get the robust chain first
    let mut segs = parse_selector_chain(*node, src)?;
    segs = normalize_segments(&segs);

    // Accept .Values.*, $.Values.*, or bare Values.* (some grammars tokenize like that)
    // Return ONLY the tail after "Values"
    if segs.len() >= 3 && (segs[0] == "." || segs[0] == "$") && segs[1] == "Values" {
        return Some(segs[2..].to_vec());
    }
    if segs.len() >= 2 && segs[0] == "Values" {
        return Some(segs[1..].to_vec());
    }

    None

    // return parse_selector_chain(node.clone(), src);

    // // Expect something like:
    // // (selector_expression (selector_expression
    // //   (field (identifier "Values"))
    // //   (field_identifier "ingress"))
    // //   (field_identifier "enabled"))
    // //
    // // Walk leftward to ensure base is .Values or $.Values
    // let mut segs = Vec::<String>::new();
    // let mut cur = *node;
    // loop {
    //     match cur.kind() {
    //         "selector_expression" => {
    //             let left = cur.child_by_field_name("operand")?;
    //             let right = cur.child_by_field_name("field")?; // field_identifier
    //             if right.kind() == "field_identifier" {
    //                 segs.push(right.utf8_text(src.as_bytes()).ok()?.to_string());
    //             }
    //             cur = left;
    //         }
    //         "field" => {
    //             // ".Values" -> (field (identifier "Values"))
    //             let id = cur.child_by_field_name("name")?;
    //             if id.kind() == "identifier" && id.utf8_text(src.as_bytes()).ok()? == "Values" {
    //                 segs.reverse(); // collected from right to left
    //                 let segs = normalize_segments(&segs);
    //                 return Some(segs);
    //             } else {
    //                 return None;
    //             }
    //         }
    //         "variable" | "dot" => {
    //             // not a .Values chain
    //             return None;
    //         }
    //         _ => return None,
    //     }
    // }
}

pub(crate) fn parse_selector_chain(node: Node, src: &str) -> Option<Vec<String>> {
    fn rec(n: Node, src: &str, out: &mut Vec<String>) {
        match n.kind() {
            "selector_expression" => {
                let left = n
                    .child_by_field_name("operand")
                    .or_else(|| n.child(0))
                    .unwrap();
                let right = n
                    .child_by_field_name("field")
                    .or_else(|| n.child(1))
                    .unwrap();
                rec(left, src, out);
                let id = right
                    .utf8_text(src.as_bytes())
                    .ok()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !id.is_empty() {
                    out.push(id);
                }
            }

            // NEW: handle (...) around an operand like ($.Chart)
            "parenthesized_pipeline" => {
                if let Some(inner) = n.named_child(0) {
                    rec(inner, src, out);
                }
            }

            // Nice to have: if someone calls this on a pipeline, descend to its head
            "chained_pipeline" => {
                if let Some(head) = n.named_child(0) {
                    rec(head, src, out);
                }
            }

            "dot" => out.push(".".into()),

            // Make variable handling consistent: push "$" or "$name" as-is.
            "variable" => {
                if let Ok(t) = n.utf8_text(src.as_bytes()) {
                    let t = t.trim();
                    if t.starts_with('$') {
                        out.push(t.to_string()); // "$" or "$name"
                    }
                }
            }

            "identifier" | "field" | "field_identifier" => {
                let id = n
                    .utf8_text(src.as_bytes())
                    .ok()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !id.is_empty() {
                    out.push(id);
                }
            }
            _ => {}
        }
    }

    let mut segs = Vec::new();
    rec(node, src, &mut segs);

    let segs = normalize_segments(&segs);
    if segs.is_empty() { None } else { Some(segs) }

    // fn rec(n: Node, src: &str, out: &mut Vec<String>) {
    //     match n.kind() {
    //         "selector_expression" => {
    //             let left = n
    //                 .child_by_field_name("operand")
    //                 .or_else(|| n.child(0))
    //                 .unwrap();
    //             let right = n
    //                 .child_by_field_name("field")
    //                 .or_else(|| n.child(1))
    //                 .unwrap();
    //             rec(left, src, out);
    //             let id = right
    //                 .utf8_text(src.as_bytes())
    //                 .ok()
    //                 .unwrap_or("")
    //                 .trim()
    //                 .to_string();
    //             if !id.is_empty() {
    //                 out.push(id);
    //             }
    //         }
    //         "dot" => out.push(".".into()),
    //         "variable" => {
    //             let t = n.utf8_text(src.as_bytes()).ok().unwrap_or("").trim();
    //             if t == "$" {
    //                 out.push("$".into());
    //             }
    //         }
    //         "identifier" | "field" | "field_identifier" => {
    //             let id = n
    //                 .utf8_text(src.as_bytes())
    //                 .ok()
    //                 .unwrap_or("")
    //                 .trim()
    //                 .to_string();
    //             if !id.is_empty() {
    //                 out.push(id);
    //             }
    //         }
    //         _ => {}
    //     }
    // }
    // let mut segs = Vec::new();
    // rec(node, src, &mut segs);
    // let normalized_segs = normalize_segments(&segs);
    // dbg!(&normalized_segs);
    // if normalized_segs.is_empty() {
    //     None
    // } else {
    //     Some(normalized_segs)
    // }
}

#[cfg(test)]
mod tests {
    use super::extract_values_paths;
    use color_eyre::eyre::{self, OptionExt};
    use helm_schema_template::parse::parse_gotmpl_document;
    use indoc::indoc;
    use test_util::prelude::*;
    use vfs::VfsPath;

    fn collect(expr: &str) -> eyre::Result<std::collections::BTreeSet<String>> {
        let full = format!("{{{{ {expr} }}}}");
        let parsed = parse_gotmpl_document(&full).ok_or_eyre("failed to parse")?;
        let values = extract_values_paths(&parsed.tree, &full)
            .into_iter()
            .map(|p| p.0)
            .collect();
        Ok(values)
    }

    #[test]
    fn selector_chain_basic() -> eyre::Result<()> {
        Builder::default().build();
        let got = collect(".Values.foo.bar.baz")?;
        assert_that!(got, contains(eq("foo.bar.baz")));
        Ok(())
    }

    #[test]
    fn index_with_strings() -> eyre::Result<()> {
        Builder::default().build();
        let got = collect(r#"index .Values "ingress" "pathType""#)?;
        assert_that!(got, contains(eq("ingress.pathType")));
        Ok(())
    }

    #[test]
    fn nested_in_wrappers() -> eyre::Result<()> {
        Builder::default().build();
        let got = collect(r#"tpl .Values.a.b . | default (index .Values "x" "y")"#)?;
        assert_that!(got, contains(eq("a.b")));
        assert_that!(got, contains(eq("x.y")));
        Ok(())
    }

    #[test]
    fn ignore_non_values() -> eyre::Result<()> {
        Builder::default().build();
        let got = collect(".Chart.Name")?;
        assert_that!(got, is_empty());
        Ok(())
    }

    #[test]
    fn hyphen_braces_supported() -> eyre::Result<()> {
        Builder::default().build();
        let full = indoc! {r#"
        {{- if .Values.a }}
        {{- index .Values "b" "c" -}}
        {{- end -}}
    "#};
        let parsed = parse_gotmpl_document(full).ok_or_eyre("failed to parse")?;
        let all = extract_values_paths(&parsed.tree, full);
        let have: Vec<_> = all.into_iter().map(|p| p.0).collect();
        assert!(have.contains(&"a".to_string()));
        assert!(have.contains(&"b.c".to_string()));
        Ok(())
    }

    #[test]
    fn index_with_backticks() -> eyre::Result<()> {
        Builder::default().build();
        // {{ index .Values `ingress` `hostname` }} => ingress.hostname
        let src = "{{ index .Values `ingress` `hostname` }}";
        let p = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
        let got = extract_values_paths(&p.tree, &p.source);
        let have: Vec<_> = got.into_iter().map(|v| v.0).collect();
        assert_eq!(have, vec!["ingress.hostname"]);
        Ok(())
    }

    #[test]
    fn selector_chain_no_head_no_leaf() -> eyre::Result<()> {
        Builder::default().build();
        // {{ .Values.ingress.enabled }} => only "ingress.enabled"
        let p =
            parse_gotmpl_document("{{ .Values.ingress.enabled }}").ok_or_eyre("failed to parse")?;
        let got = extract_values_paths(&p.tree, &p.source);
        let have: Vec<_> = got.into_iter().map(|v| v.0).collect();
        assert_eq!(have, vec!["ingress.enabled"]);
        Ok(())
    }

    #[test]
    fn index_with_selector_head() -> eyre::Result<()> {
        Builder::default().build();
        // {{ index .Values "a" "b" "c" }} => a.b.c
        let p = parse_gotmpl_document(r#"{{ index .Values "a" "b" "c" }}"#)
            .ok_or_eyre("failed to parse")?;
        let got = extract_values_paths(&p.tree, &p.source);
        let have: Vec<_> = got.into_iter().map(|v| v.0).collect();
        assert_eq!(have, vec!["a.b.c"]);
        Ok(())
    }

    #[test]
    fn parses_go_template_and_extracts_values_paths() -> eyre::Result<()> {
        Builder::default().build();
        let srcs = vec![
            r#"{{ .Values.ingress.enabled }}"#,
            r#"{{ index .Values "ingress" "pathType" }}"#,
            r#"{{ toYaml .Values.ingress.extraRules | nindent 2 }}"#,
            r#"{{ .Values.commonAnnotations }}"#,
        ];

        // println!("{}", &helm_schema_template_grammar::go_template::NODE_TYPES);

        let mut all = std::collections::BTreeSet::new();
        for src in srcs {
            let parsed = parse_gotmpl_document(src).ok_or_eyre("failed to parse")?;
            let ast = helm_schema_template::fmt::SExpr::parse_tree(&parsed.tree.root_node(), src);
            println!("{}", ast.to_string_pretty());

            let paths = extract_values_paths(&parsed.tree, src);
            dbg!(&paths);
            for p in paths {
                all.insert(p.0);
            }
        }

        // Should see these normalized paths
        assert!(all.contains("ingress.enabled"));
        assert!(all.contains("ingress.pathType"));
        assert!(all.contains("ingress.extraRules"));
        assert!(all.contains("commonAnnotations"));
        Ok(())
    }

    #[test]
    fn extracts_values_from_selectors_and_index_and_pipelines() -> eyre::Result<()> {
        let cases = [
            (
                r#"{{ .Values.ingress.enabled }}"#,
                vec![
                    // "ingress",
                    "ingress.enabled",
                ],
            ),
            (
                r#"{{ index .Values "ingress" "pathType" }}"#,
                vec!["ingress.pathType"],
            ),
            (
                r#"{{ toYaml .Values.ingress.extraRules | nindent 2 }}"#,
                vec![
                    // "ingress",
                    "ingress.extraRules",
                ],
            ),
            (
                r#"{{ index .Values `ingress` `hostname` }}"#,
                vec!["ingress.hostname"],
            ),
            // nested selector chain
            (
                r#"{{ .Values.database.primary.user }}"#,
                vec![
                    // "database",
                    // "database.primary",
                    "database.primary.user",
                ],
            ),
            // ensure we don't match non-.Values
            (r#"{{ .Release.Name }}"#, vec![]),
            // complex pipeline with both
            (
                r#"{{ default (index .Values "featureFlags" "enable_v2") (.Values.featureFlags.enable_v1) }}"#,
                vec![
                    // "featureFlags",
                    "featureFlags.enable_v1",
                    "featureFlags.enable_v2",
                ],
            ),
        ];

        for (expr, want) in cases {
            let parsed = parse_gotmpl_document(expr).ok_or_eyre("failed to parse")?;
            let ast = helm_schema_template::fmt::SExpr::parse_tree(&parsed.tree.root_node(), expr);
            println!("{}", ast.to_string_pretty());

            let paths = extract_values_paths(&parsed.tree, &parsed.source);

            sim_assert_eq!(have: paths.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), want: want);
            // for expect in want {
            //     assert!(got.iter().any(|v| v.0 == expect), "missing {}", expect);
            // }
        }
        Ok(())
    }

    #[test]
    fn end_to_end_ingress_sample_smoke() -> eyre::Result<()> {
        Builder::default().build();
        let root = VfsPath::new(vfs::MemoryFS::new());
        let template = indoc! {r#"
        {{- if .Values.ingress.enabled }}
        apiVersion: {{ include "common.capabilities.ingress.apiVersion" . }}
        kind: Ingress
        metadata:
          name: {{ include "common.names.fullname" . }}
          namespace: {{ include "common.names.namespace" . | quote }}
          labels:
            app.kubernetes.io/component: minio
          {{- if or .Values.ingress.annotations .Values.commonAnnotations }}
          annotations: {{- include "common.tplvalues.render" (dict "value" .Values.ingress.annotations "context" .) | nindent 4 }}
          {{- end }}
        spec:
          {{- if .Values.ingress.ingressClassName }}
          ingressClassName: {{ .Values.ingress.ingressClassName | quote }}
          {{- end }}
          rules:
            {{- if .Values.ingress.hostname }}
            - host: {{ tpl .Values.ingress.hostname . }}
              http:
                paths:
                  - path: {{ .Values.ingress.path }}
                    pathType: {{ .Values.ingress.pathType }}
            {{- end }}
        {{- end }}
    "#};
        write(&root.join("templates/ing.yaml")?, template)?;

        // Parse gotmpl document and get ranges
        let parsed = helm_schema_template::parse::parse_gotmpl_document(&template)
            .ok_or_eyre("failed to parse go template")?;
        let ast = helm_schema_template::fmt::SExpr::parse_tree(&parsed.tree.root_node(), &template);
        println!("{}", ast.to_string_pretty());

        let template_ranges = helm_schema_template::parse::template_node_byte_ranges(&parsed);
        dbg!(&template_ranges);

        // Sanitize to YAML and parse YAML
        let sanitized = helm_schema_template::yaml_parse::sanitize_yaml_from_gotmpl_text_nodes(
            &parsed.tree,
            &template,
            // &template_ranges,
        );
        println!("{sanitized}");
        let y = helm_schema_template::yaml_parse::parse_yaml_sanitized(&sanitized)
            .ok_or_eyre("failed to parse yaml")?;
        let ast = helm_schema_template::fmt::SExpr::parse_tree(&y.tree.root_node(), &sanitized);
        println!("{}", ast.to_string_pretty());
        assert!(y.tree.root_node().child_count() > 0);

        // Extract .Values paths from gotmpl tree
        let values = extract_values_paths(&parsed.tree, &template);
        let mut all = std::collections::BTreeSet::new();
        for v in values {
            all.insert(v.0);
        }

        assert!(all.contains("ingress.enabled"));
        assert!(all.contains("ingress.annotations"));
        assert!(all.contains("commonAnnotations"));
        assert!(all.contains("ingress.ingressClassName"));
        assert!(all.contains("ingress.hostname"));
        assert!(all.contains("ingress.path"));
        assert!(all.contains("ingress.pathType"));
        Ok(())

        // // scan -> sanitize -> parse yaml -> parse each gotmpl expr -> collect values
        // let content = root.join("templates/ing.yaml")?.read_to_string()?;
        // let blocks = scan_gotmpl_blocks(&content);
        // dbg!(&blocks);
        //
        // // yaml parse should succeed after sanitization
        // let sanitized = sanitize_yaml_for_parse(&content, &blocks);
        // let y = parse_yaml_sanitized(&sanitized).expect("yaml parse");
        // assert!(y.tree.root_node().child_count() > 0);
        //
        // // go-template values extraction across all blocks
        // let mut all = std::collections::BTreeSet::new();
        // for b in &blocks {
        //     // let expr = &content[b.inner_start..b.inner_end];
        //     let expr = ["{{ ", &content[b.inner_start..b.inner_end], " }}"].concat();
        //     println!("{expr}");
        //
        //     let parsed = parse_gotmpl_expr(&expr).ok_or_eyre("failed to parse")?;
        //     let ast = helm_schema_template::fmt::SExpr::parse_tree(&parsed.tree.root_node(), &expr);
        //     println!("{}", ast.to_string_pretty());
        //
        //     let values = extract_values_paths(&parsed.tree, &expr);
        //     for value in values {
        //         all.insert(value.0);
        //     }
        // }
        //
        // // spot-check expected keys
        // assert!(all.contains("ingress.enabled"));
        // assert!(all.contains("ingress.annotations"));
        // assert!(all.contains("commonAnnotations"));
        // assert!(all.contains("ingress.ingressClassName"));
        // assert!(all.contains("ingress.hostname"));
        // assert!(all.contains("ingress.path"));
        // assert!(all.contains("pathType")); // from `index` or direct? here direct .Values.ingress.pathType
        // assert!(all.contains("ingress.pathType"));
        // Ok(())
    }
}
