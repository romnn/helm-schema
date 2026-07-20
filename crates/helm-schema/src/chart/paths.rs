pub(crate) fn scope_values_path(path: &str, prefix: &[String]) -> String {
    let path = path.trim();
    if path.is_empty() {
        // An empty path in a chart-local contract denotes that chart's own
        // values root (a whole-tree wrapper scope, a root defaults target),
        // which under parent composition IS the subchart prefix. Leaving it
        // empty would re-root the fact at the PARENT's document root.
        return helm_schema_core::join_value_path(prefix.iter().cloned());
    }

    let path_segments = helm_schema_core::split_value_path(path);
    if path_segments
        .first()
        .is_some_and(|segment| segment == "global")
    {
        return path.to_string();
    }

    if prefix.is_empty() {
        return path.to_string();
    }

    helm_schema_core::join_value_path(prefix.iter().cloned().chain(path_segments))
}
