use std::cell::RefCell;
use std::collections::HashMap;

use helm_schema_ast::{TemplateExpr, parse_action_expressions};

thread_local! {
    static TEMPLATE_EXPR_CACHE: RefCell<HashMap<String, Vec<TemplateExpr>>> =
        RefCell::new(HashMap::new());
}

pub(crate) fn clear_template_expr_cache() {
    TEMPLATE_EXPR_CACHE.with(|cache| cache.borrow_mut().clear());
}

pub(crate) fn parse_expr_text(text: &str) -> Vec<TemplateExpr> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let normalized = if trimmed.starts_with("{{") {
        trimmed.to_string()
    } else {
        format!("{{{{ {trimmed} }}}}")
    };

    if let Some(cached) = TEMPLATE_EXPR_CACHE.with(|cache| cache.borrow().get(&normalized).cloned())
    {
        return cached;
    }

    let parsed = parse_action_expressions(&normalized);
    TEMPLATE_EXPR_CACHE.with(|cache| {
        cache.borrow_mut().insert(normalized, parsed.clone());
    });
    parsed
}
