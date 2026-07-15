use std::collections::BTreeMap;

/// How `range` iterates one values path.
///
/// A default (all-false) mode means the path carries no range facts; absent
/// map entries and default modes are interchangeable, so queries never need
/// to distinguish "no entry" from "entry with no flags".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RangeMode {
    /// Iterated DIRECTLY (`range .Values.x`): only such paths have member
    /// identities and an iterable input domain.
    pub(crate) direct: bool,
    /// The iterated values came through JSON decoding.
    pub(crate) json_decoded: bool,
    /// Iterated with key and value variables (`range $k, $v := …`):
    /// integers iterate single-variable ranges only ("can't use 2 to
    /// iterate over more than one variable").
    pub(crate) destructured: bool,
}

/// The per-path range facts of one scope (an interpreter run, a helper
/// summary, a fail capture, a contract graph): one map instead of three
/// parallel path sets, so merging, remapping, and copying cannot leave one
/// flavor behind.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RangeModes {
    modes: BTreeMap<String, RangeMode>,
}

impl RangeModes {
    /// The mode recorded for a path; all-false when nothing was recorded.
    pub(crate) fn mode(&self, path: &str) -> RangeMode {
        self.modes.get(path).copied().unwrap_or_default()
    }

    pub(crate) fn mark_direct(&mut self, path: &str) {
        self.entry(path, |mode| mode.direct = true);
    }

    pub(crate) fn mark_json_decoded(&mut self, path: &str) {
        self.entry(path, |mode| mode.json_decoded = true);
    }

    pub(crate) fn mark_destructured(&mut self, path: &str) {
        self.entry(path, |mode| mode.destructured = true);
    }

    /// Union the flags of both scopes per path.
    pub(crate) fn merge(&mut self, other: &RangeModes) {
        for (path, mode) in &other.modes {
            let merged = self.modes.entry(path.clone()).or_default();
            merged.direct |= mode.direct;
            merged.json_decoded |= mode.json_decoded;
            merged.destructured |= mode.destructured;
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, RangeMode)> {
        self.modes.iter().map(|(path, mode)| (path.as_str(), *mode))
    }

    /// Rewrite every path, unioning modes that collapse onto one target.
    pub(crate) fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        let mut mapped = RangeModes::default();
        for (path, mode) in &self.modes {
            let target = map(path);
            let merged = mapped.modes.entry(target).or_default();
            merged.direct |= mode.direct;
            merged.json_decoded |= mode.json_decoded;
            merged.destructured |= mode.destructured;
        }
        *self = mapped;
    }

    fn entry(&mut self, path: &str, set: impl FnOnce(&mut RangeMode)) {
        if path.trim().is_empty() {
            return;
        }
        set(self.modes.entry(path.to_string()).or_default());
    }
}
