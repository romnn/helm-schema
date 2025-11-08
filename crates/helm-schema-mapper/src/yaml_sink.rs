use crate::YamlPath;
use std::collections::{BTreeMap, BTreeSet};

/// What kind of slot we’re *currently* in from YAML’s perspective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YSlot {
    Plain,    // top-level/plain position
    MapKey,   // about to write a key
    MapValue, // about to write a value under a map
    SeqItem,  // about to write a list item
}

pub trait YSink {
    /// Feed literal YAML text (from the template) into the sink to update context and append to output.
    fn push_text(&mut self, text: &str);

    /// Current slot computed from actual YAML context.
    fn slot(&self) -> YSlot;

    /// Emit a fragment placeholder and return its id + logical YAML path (if already resolvable).
    fn emit_fragment(&mut self, values: &BTreeSet<String>) -> usize;

    /// Emit a scalar placeholder at the current location.
    fn emit_scalar(&mut self, values: &BTreeSet<String>) -> usize;

    /// Finalize and extract the finished buffer + placeholder → YamlPath mapping.
    fn finish(self) -> (String, BTreeMap<usize, Option<YamlPath>>);
}
