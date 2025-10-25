use color_eyre::eyre::{self, OptionExt};
use helm_schema_template::{parse::parse_gotmpl_document, values::extract_values_paths};
use indoc::indoc;
use test_util::prelude::*;
use vfs::VfsPath;

// #[test]
// fn scans_template_blocks_respecting_strings() -> eyre::Result<()> {
//     Builder::default().build();
//     let s = indoc! {r#"
//         kind: ConfigMap
//         data:
//           tricky: {{ printf "end }}" }} # this should not break scanning
//           okay: {{- toYaml .Values.stuff | nindent 4 -}}
//     "#};
//     let blocks = scan_gotmpl_blocks(s);
//     assert_that!(blocks, len(eq(2)));
//     // verify inner byte slices roughly align
//     let inner1 = &s[blocks[0].inner_start..blocks[0].inner_end];
//     assert_that!(inner1, contains_substring("printf"));
//     let inner2 = &s[blocks[1].inner_start..blocks[1].inner_end];
//     assert_that!(inner2, contains_substring(".Values.stuff"));
//     Ok(())
// }

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
    let values = helm_schema_template::values::extract_values_paths(&parsed.tree, &template);
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
