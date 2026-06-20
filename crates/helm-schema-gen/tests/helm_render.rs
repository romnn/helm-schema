mod common;

#[test]
fn helm_templates_render_successfully() {
    for case in common::cases::HELM_RENDER_CASES {
        common::assert_helm_render_case(case);
    }
}
