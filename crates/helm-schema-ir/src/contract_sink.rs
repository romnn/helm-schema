use crate::Guard;

/// Append each guard not already present, preserving existing order.
pub(crate) fn merge_guards(target: &mut Vec<Guard>, extra_guards: &[Guard]) {
    for guard in extra_guards {
        if !target.contains(guard) {
            target.push(guard.clone());
        }
    }
}
