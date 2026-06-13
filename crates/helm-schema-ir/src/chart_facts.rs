use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartFacts {
    pub path_facts: BTreeMap<String, PathFact>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathFact {
    pub has_render_use: bool,
    pub all_render_uses_self_guarded: bool,
    pub has_fragment_render: bool,
    pub descendant_accessed: bool,
    pub has_self_range_guard_render_use: bool,
}
