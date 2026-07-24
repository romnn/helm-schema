use indoc::indoc;
use super::action_continues_pending_yaml_value;

#[test]
fn open_mapping_key_continues_with_structural_fragment_indent() {
    let pending = indoc! {"
        metadata:
          labels:
    "};
    assert!(action_continues_pending_yaml_value(pending, 4));
    assert!(!action_continues_pending_yaml_value(pending, 2));
}

#[test]
fn open_mapping_key_continues_past_comment_line() {
    let pending = indoc! {"
        metadata:
          labels:
          # chart adds labels here
    "};
    assert!(action_continues_pending_yaml_value(pending, 4));
}
