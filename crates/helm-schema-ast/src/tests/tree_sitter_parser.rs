use super::action_continues_pending_yaml_value;

#[test]
fn open_mapping_key_continues_with_structural_fragment_indent() {
    let pending = "metadata:\n  labels:\n";
    assert!(action_continues_pending_yaml_value(pending, 4));
    assert!(!action_continues_pending_yaml_value(pending, 2));
}

#[test]
fn open_mapping_key_continues_past_comment_line() {
    let pending = "metadata:\n  labels:\n  # chart adds labels here\n";
    assert!(action_continues_pending_yaml_value(pending, 4));
}
