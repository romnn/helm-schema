use std::path::{Path, PathBuf};

fn collect_yaml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_yaml_files(&path, out);
            continue;
        }

        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };

        if ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml") {
            out.push(path);
        }
    }
}

fn line_col(text: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;

    for (i, ch) in text.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }

    (line, col)
}

fn find_most_specific_error_or_missing_node(tree: &tree_sitter::Tree) -> Option<tree_sitter::Node<'_>> {
    let root = tree.root_node();

    let mut stack = vec![root];
    let mut best: Option<tree_sitter::Node<'_>> = None;

    while let Some(n) = stack.pop() {
        if n.is_error() || n.is_missing() {
            best = match best {
                None => Some(n),
                Some(cur) => {
                    let cur_len = cur.byte_range().len();
                    let n_len = n.byte_range().len();
                    if n_len < cur_len {
                        Some(n)
                    } else {
                        Some(cur)
                    }
                }
            };
        }

        let mut c = n.walk();
        let children: Vec<_> = n.children(&mut c).collect();
        for ch in children.into_iter().rev() {
            stack.push(ch);
        }
    }

    best
}

#[test]
fn parses_all_testdata_yaml_templates_with_zero_error_nodes() {
    let language = tree_sitter::Language::new(helm_schema_template_grammar::yaml::language());

    let only = std::env::var("HELM_SCHEMA_TEST_ONLY").ok();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("helm-schema-mapper")
        .join("testdata")
        .join("charts");

    let mut files = Vec::new();
    collect_yaml_files(&root, &mut files);
    files.sort();

    assert!(!files.is_empty(), "no yaml files found under {}", root.display());

    let only_path = only.as_ref().and_then(|only| {
        let only = PathBuf::from(only);
        let is_path_like = only.is_absolute() || only.to_string_lossy().contains(std::path::MAIN_SEPARATOR);
        if !is_path_like {
            return None;
        }

        std::fs::canonicalize(&only).ok().or(Some(only))
    });

    let mut parsed_files = 0usize;

    for file in files {
        if let Some(ref only) = only {
            if let Some(ref only_path) = only_path {
                let file_canon = std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
                if file_canon != *only_path {
                    continue;
                }
            } else {
                let p = file.to_string_lossy();
                if !p.contains(only) {
                    continue;
                }
            }
        }

        parsed_files += 1;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap();
        let src = std::fs::read_to_string(&file).unwrap();
        let tree = parser.parse(&src, None).unwrap();

        if let Some(node) = find_most_specific_error_or_missing_node(&tree) {
            let (line, col) = line_col(&src, node.start_byte());
            let sp = node.start_position();
            let ep = node.end_position();
            panic!(
                "tree-sitter YAML parse error/missing node in {} at {}:{}\nkind={} is_error={} is_missing={} range={:?} point={:?}..{:?}\nnode_sexp={}\nroot_sexp={}\n",
                file.display(),
                line,
                col,
                node.kind(),
                node.is_error(),
                node.is_missing(),
                node.byte_range(),
                sp,
                ep,
                node.to_sexp(),
                tree.root_node().to_sexp(),
            );
        }

        if tree.root_node().has_error() {
            panic!(
                "tree has_error() but no explicit ERROR/MISSING node was found in {}\nroot_sexp={}\n",
                file.display(),
                tree.root_node().to_sexp()
            );
        }
    }

    if let Some(ref only) = only {
        assert!(
            parsed_files > 0,
            "HELM_SCHEMA_TEST_ONLY={:?} matched zero yaml files under {}",
            only,
            root.display()
        );
    }
}
