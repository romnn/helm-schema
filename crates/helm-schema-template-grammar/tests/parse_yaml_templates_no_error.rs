use std::ffi::OsStr;
use std::fmt::Write;
use std::path::{Path, PathBuf};

fn is_template_yaml(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };

    let is_yaml = ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml");
    if !is_yaml {
        return false;
    }

    path.components()
        .any(|c| c.as_os_str() == OsStr::new("templates"))
}

fn indent_len(line: &str) -> usize {
    line.as_bytes().iter().take_while(|b| **b == b' ').count()
}

fn fill_empty_mapping_values_with_placeholder(text: &str, placeholder: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(text.len());

    for (idx, line) in lines.iter().enumerate() {
        let (content, has_nl) = line
            .strip_suffix('\n')
            .map_or((*line, false), |c| (c, true));

        let content = content.strip_suffix('\r').unwrap_or(content);
        let trimmed = content.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(content);
            if has_nl {
                out.push('\n');
            }
            continue;
        }

        // Only target lines like `key:` / `- key:` with no value.
        // Skip block scalars (`key: |`, `key: >`) and explicit null-ish values.
        let Some(colon_idx) = content.find(':') else {
            out.push_str(content);
            if has_nl {
                out.push('\n');
            }
            continue;
        };

        let (before, after) = content.split_at(colon_idx + 1);
        if !after.trim().is_empty() {
            out.push_str(content);
            if has_nl {
                out.push('\n');
            }
            continue;
        }

        let before_trimmed = before[..before.len() - 1].trim();
        if before_trimmed.is_empty() {
            out.push_str(content);
            if has_nl {
                out.push('\n');
            }
            continue;
        }

        // Decide if this is the start of a block by looking ahead.
        // YAML allows *indentless sequences* as mapping values:
        //
        //   items:
        //   - key: value
        //
        // The `-` can appear at the same indent as the key.
        let cur_indent = indent_len(content);
        let mut next_nonempty_indent: Option<usize> = None;
        let mut next_nonempty_trimmed: Option<&str> = None;
        for next in lines.iter().skip(idx + 1) {
            let next_content = next.strip_suffix('\n').unwrap_or(next);
            let next_content = next_content.strip_suffix('\r').unwrap_or(next_content);
            let t = next_content.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            next_nonempty_indent = Some(indent_len(next_content));
            next_nonempty_trimmed = Some(t);
            break;
        }

        let starts_block = next_nonempty_indent.is_some_and(|i| i > cur_indent)
            || (next_nonempty_indent == Some(cur_indent)
                && next_nonempty_trimmed.is_some_and(|t| t.starts_with('-')));
        if starts_block {
            out.push_str(before);
        } else {
            out.push_str(before);
            out.push(' ');
            out.push_str(placeholder);
        }

        if has_nl {
            out.push('\n');
        }
    }

    out
}

fn dedent_suspiciously_indented_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_nonempty_indent: Option<usize> = None;
    let mut last_nonempty_had_scalar_value = false;

    for line in text.split_inclusive('\n') {
        let (content, has_nl) = line.strip_suffix('\n').map_or((line, false), |c| (c, true));
        let content = content.strip_suffix('\r').unwrap_or(content);

        let trimmed = content.trim();
        if trimmed.is_empty() {
            out.push_str(content);
            if has_nl {
                out.push('\n');
            }
            continue;
        }

        let indent = indent_len(content);
        let looks_like_key = trimmed
            .split_once(':')
            .is_some_and(|(k, _)| !k.trim().is_empty());

        let mut used_indent = indent;
        if let Some(prev) = last_nonempty_indent {
            let suspiciously_deeper = indent.saturating_sub(prev) > 16;
            if suspiciously_deeper && last_nonempty_had_scalar_value && looks_like_key {
                used_indent = prev;
            }
        }

        if used_indent == indent {
            out.push_str(content);
        } else {
            out.push_str(&" ".repeat(used_indent));
            out.push_str(trimmed);
        }
        if has_nl {
            out.push('\n');
        }

        last_nonempty_indent = Some(used_indent);
        last_nonempty_had_scalar_value = trimmed
            .split_once(':')
            .is_some_and(|(_, v)| !v.trim().is_empty());
    }

    out
}

fn strip_whitespace_only_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_block_scalar = false;
    let mut block_scalar_header_indent: usize = 0;

    for line in text.split_inclusive('\n') {
        let (content, has_nl) = line.strip_suffix('\n').map_or((line, false), |c| (c, true));

        let content = content.strip_suffix('\r').unwrap_or(content);

        if in_block_scalar
            && !content.trim().is_empty()
            && indent_len(content) <= block_scalar_header_indent
        {
            in_block_scalar = false;
        }

        if !in_block_scalar {
            let trimmed_end = content.trim_end();
            if let Some(last_token) = trimmed_end.split_whitespace().last() {
                let first = last_token.as_bytes().first().copied();
                if first == Some(b'|') || first == Some(b'>') {
                    in_block_scalar = true;
                    block_scalar_header_indent = indent_len(content);
                }
            }
        }

        if content.trim().is_empty() {
            if in_block_scalar {
                if has_nl {
                    out.push('\n');
                }
                continue;
            }

            if has_nl {
                out.push('\n');
            }
            continue;
        }

        out.push_str(content);
        if has_nl {
            out.push('\n');
        }
    }

    out
}

fn sanitize_yaml_for_parse_from_gotmpl_tree(gotmpl_tree: &tree_sitter::Tree, src: &str) -> String {
    let root = gotmpl_tree.root_node();
    let mut keep_ranges: Vec<std::ops::Range<usize>> = Vec::new();

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "text" || node.kind() == "yaml_no_injection_text" {
            keep_ranges.push(node.byte_range());
        }

        let mut c = node.walk();
        let children: Vec<_> = node.children(&mut c).collect();
        for ch in children.into_iter().rev() {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }

    keep_ranges.sort_by_key(|r| r.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for r in keep_ranges {
        match merged.last_mut() {
            None => merged.push(r),
            Some(last) => {
                if r.start <= last.end {
                    last.end = last.end.max(r.end);
                } else {
                    merged.push(r);
                }
            }
        }
    }

    let bytes = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut ri = 0usize;
    let mut keep_mask = vec![false; bytes.len()];
    for i in 0..bytes.len() {
        while ri < merged.len() && i >= merged[ri].end {
            ri += 1;
        }
        let in_keep = ri < merged.len() && i >= merged[ri].start && i < merged[ri].end;
        keep_mask[i] = in_keep;
        let b = bytes[i];
        if in_keep || b == b'\n' {
            out.push(b);
        } else {
            out.push(b' ');
        }
    }

    // Replace template action runs with a placeholder token when they occur inline with YAML.
    // If we keep them as spaces, they can produce invalid plain scalars like `rediss://     :`.
    let mut i = 0usize;
    let mut line_start = 0usize;
    while i < out.len() {
        if out[i] == b'\n' {
            i += 1;
            line_start = i;
            continue;
        }

        let is_hole = i < keep_mask.len() && !keep_mask[i] && out[i] != b'\n';
        if !is_hole {
            i += 1;
            continue;
        }

        let start = i;
        while i < out.len() && out[i] != b'\n' && i < keep_mask.len() && !keep_mask[i] {
            i += 1;
        }
        let end = i;

        let mut line_end = end;
        while line_end < out.len() && out[line_end] != b'\n' {
            line_end += 1;
        }

        let prefix = &out[line_start..start];
        let prefix_has_non_ws = prefix.iter().any(|b| !b.is_ascii_whitespace());

        // If the hole starts immediately after a mapping `:` (e.g. `key: {{ ... }}`), do not
        // replace it with `0`s. We want `fill_empty_mapping_values_with_placeholder` to decide
        // whether this starts an indented block (next line more indented) or needs a scalar.
        let prefix_ends_with_mapping_colon = prefix
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .is_some_and(|idx| prefix[idx] == b':' && !prefix[..idx].contains(&b':'));

        let mut j = end;
        while j < line_end && out[j].is_ascii_whitespace() {
            j += 1;
        }
        let suffix_starts_with_colon = j < line_end && out[j] == b':';

        if suffix_starts_with_colon || (prefix_has_non_ws && !prefix_ends_with_mapping_colon) {
            out[start..end].fill(b'0');
        }
    }

    String::from_utf8(out).expect("sanitized YAML must be valid utf-8")
}

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

        if is_template_yaml(&path) {
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

fn find_most_specific_error_or_missing_node(
    tree: &tree_sitter::Tree,
) -> Option<tree_sitter::Node<'_>> {
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
                    if n_len < cur_len { Some(n) } else { Some(cur) }
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

fn contains_kind(root: tree_sitter::Node<'_>, kind: &str) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == kind {
            return true;
        }
        let mut c = n.walk();
        let children: Vec<_> = n.children(&mut c).collect();
        for ch in children.into_iter().rev() {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }
    false
}

#[test]
#[allow(clippy::too_many_lines)]
fn parses_all_testdata_yaml_templates_best_effort() {
    let yaml_language = tree_sitter::Language::new(helm_schema_template_grammar::yaml::language());
    let gotmpl_language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());

    let only = std::env::var("HELM_SCHEMA_TEST_ONLY").ok();
    let dump_sanitized = std::env::var("HELM_SCHEMA_DUMP_SANITIZED").ok();

    let root = test_util::workspace_testdata().join("charts");

    let mut files = Vec::new();
    collect_yaml_files(&root, &mut files);
    files.sort();
    let total_files = files.len();

    assert!(
        !files.is_empty(),
        "no yaml files found under {}",
        root.display()
    );

    let only_path = only.as_ref().and_then(|only| {
        let only = PathBuf::from(only);
        let is_path_like =
            only.is_absolute() || only.to_string_lossy().contains(std::path::MAIN_SEPARATOR);
        if !is_path_like {
            return None;
        }

        if only.is_absolute() {
            return std::fs::canonicalize(&only).ok().or(Some(only));
        }

        let direct = std::fs::canonicalize(&only).ok();
        if direct.is_some() {
            return direct;
        }

        let under_charts = root.join(&only);
        std::fs::canonicalize(&under_charts).ok()
    });

    let mut parsed_files = 0usize;

    for file in &files {
        if let Some(ref only) = only {
            if let Some(ref only_path) = only_path {
                let file_canon = std::fs::canonicalize(file).unwrap_or_else(|_| (*file).clone());
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
        parser.set_language(&yaml_language).unwrap();
        let src = std::fs::read_to_string(file).unwrap();

        let sanitized = if src.contains("{{") {
            let mut gotmpl_parser = tree_sitter::Parser::new();
            gotmpl_parser.set_language(&gotmpl_language).unwrap();
            let gotmpl_tree = gotmpl_parser.parse(&src, None).unwrap();

            let sanitized = sanitize_yaml_for_parse_from_gotmpl_tree(&gotmpl_tree, &src);
            let sanitized = strip_whitespace_only_lines(&sanitized);
            let sanitized = dedent_suspiciously_indented_keys(&sanitized);
            fill_empty_mapping_values_with_placeholder(&sanitized, "0")
        } else {
            src.clone()
        };

        // Some templates are essentially "all actions" and produce no YAML text on their own.
        // In that case, the sanitized YAML is empty and we accept that there is no YAML document.
        if src.contains("{{") && sanitized.trim().is_empty() {
            continue;
        }

        if dump_sanitized.is_some() {
            let file_name = file
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("template.yaml");
            let dump_path =
                PathBuf::from("/tmp").join(format!("helm-schema-sanitized-{file_name}"));
            let _ = std::fs::write(&dump_path, &sanitized);
        }

        let tree = parser.parse(&sanitized, None).unwrap();

        let root = tree.root_node();
        let has_doc = contains_kind(root, "document");
        let ok = has_doc;

        if !ok {
            let err = find_most_specific_error_or_missing_node(&tree);
            let mut extra = String::new();
            if let Some(node) = err {
                let (line, col) = line_col(&sanitized, node.start_byte());
                let sp = node.start_position();
                let ep = node.end_position();

                let start = node.start_byte().saturating_sub(250);
                let end = (node.start_byte() + 250).min(sanitized.len());
                let snippet = sanitized[start..end].replace('\n', "\\n");
                extra = format!(
                    "\nmost_specific_error_or_missing at {}:{} kind={} is_error={} is_missing={} range={:?} point={:?}..{:?}\nnode_sexp={}\n",
                    line,
                    col,
                    node.kind(),
                    node.is_error(),
                    node.is_missing(),
                    node.byte_range(),
                    sp,
                    ep,
                    node.to_sexp(),
                );
                let _ = write!(extra, "snippet_around_error_start=\n{snippet}\n");
            }

            panic!(
                "tree-sitter YAML best-effort parse missing required structure in {}\nroot_has_error={} root_sexp={}{}",
                file.display(),
                root.has_error(),
                root.to_sexp(),
                extra,
            );
        }
    }

    if only.is_none() {
        assert_eq!(
            parsed_files, total_files,
            "expected to parse all discovered charts/**/templates YAML files (found={total_files}, parsed={parsed_files})"
        );
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
