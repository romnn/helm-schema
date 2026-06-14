/// Strip YAML-comment lines whose first non-whitespace character is `#`.
///
/// Pure-comment lines never produce real YAML keys at render time; any
/// `{{ ... }}` action embedded in them is documentation by convention. This
/// filter is used before action-level extractors so examples in comments do
/// not contribute schema evidence.
pub(crate) fn strip_yaml_comment_lines(src: &str) -> String {
    src.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}
