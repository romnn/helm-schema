use indoc::indoc;

fn find_first<'a>(root: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == kind {
            return Some(n);
        }
        let mut c = n.walk();
        let children: Vec<_> = n.children(&mut c).collect();
        for ch in children.into_iter().rev() {
            stack.push(ch);
        }
    }
    None
}

fn has_any<'a>(root: tree_sitter::Node<'a>, kind: &str) -> bool {
    find_first(root, kind).is_some()
}

fn find_mapping_pair_with_plain_key<'a>(
    root: tree_sitter::Node<'a>,
    src: &str,
    key_text: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "block_mapping_pair" {
            if let Some(key) = n.child_by_field_name("key") {
                if key.kind() == "plain_scalar" {
                    if let Ok(t) = key.utf8_text(src.as_bytes()) {
                        if t.trim() == key_text {
                            return Some(n);
                        }
                    }
                }
            }
        }
        let mut c = n.walk();
        let children: Vec<_> = n.children(&mut c).collect();
        for ch in children.into_iter().rev() {
            stack.push(ch);
        }
    }
    None
}

#[test]
fn smoke_parses_complex_inline_helm_yaml_and_ast_shape_is_stable() {
    let language = tree_sitter::Language::new(helm_schema_template_grammar::yaml::language());

    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: {{ printf "%s-%s" .Release.Name "example" }}
        spec:
          initContainers:
            {{- include "common.tplvalues.render" (dict "value" .Values.initContainers "context" $) | nindent 12 }}
            - name: init
              image: busybox
              command:
                - sh
                - -c
                - |
                  echo "hello"
          containers:
            - name: main
              image: nginx
              env:
                {{- if .Values.extraEnv }}
                {{- toYaml .Values.extraEnv | nindent 16 }}
                {{- end }}
              ports:
                - containerPort: 80
    "#};

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse(src, None).unwrap();

    let root = tree.root_node();
    assert!(!root.has_error(), "root has_error; sexp={}", root.to_sexp());

    assert!(has_any(root, "document"), "missing document; sexp={}", root.to_sexp());
    assert!(has_any(root, "block_mapping"), "missing block_mapping; sexp={}", root.to_sexp());

    let spec_pair = find_mapping_pair_with_plain_key(root, src, "spec")
        .unwrap_or_else(|| panic!("missing spec mapping pair; sexp={}", root.to_sexp()));
    let init_pair = find_mapping_pair_with_plain_key(root, src, "initContainers")
        .unwrap_or_else(|| panic!("missing initContainers mapping pair; sexp={}", root.to_sexp()));

    let spec_value = spec_pair
        .child_by_field_name("value")
        .and_then(|n| n.named_child(0))
        .unwrap_or_else(|| panic!("spec has no value node; sexp={}", spec_pair.to_sexp()));

    assert!(
        has_any(spec_value, "block_mapping"),
        "spec value is not (or does not contain) a block_mapping; sexp={}",
        spec_value.to_sexp()
    );

    let init_value = init_pair
        .child_by_field_name("value")
        .and_then(|n| n.named_child(0))
        .unwrap_or_else(|| panic!("initContainers has no value node; sexp={}", init_pair.to_sexp()));

    // This is the key regression check: the initContainers value should parse as a sequence
    // and still include the helm template line inside that structure.
    assert!(
        has_any(init_value, "block_sequence"),
        "initContainers value does not contain a block_sequence; sexp={}",
        init_value.to_sexp()
    );

    assert!(
        has_any(init_value, "helm_template"),
        "initContainers value does not contain helm_template; sexp={}",
        init_value.to_sexp()
    );

    // Sanity: ensure we still parsed at least one explicit list item ('- name: init')
    let bs = find_first(init_value, "block_sequence")
        .unwrap_or_else(|| panic!("missing block_sequence under initContainers; sexp={}", init_value.to_sexp()));
    assert!(
        bs.named_child_count() >= 1,
        "block_sequence under initContainers has no items; sexp={}",
        bs.to_sexp()
    );
}
