pub(crate) fn scope_values_path(path: &str, prefix: &[String]) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
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
