//! Source line index. Lines are the tokens of the layout parser; the index
//! also backs byte→line lookups for the query layer.

/// Byte offsets of every line start, including a phantom final line when the
/// source ends with a newline (a byte query at `source.len()` must resolve to
/// an empty line there, matching `line_bounds` semantics).
pub(crate) struct LineIndex {
    starts: Vec<usize>,
    source_len: usize,
}

impl LineIndex {
    pub(crate) fn new(source: &str) -> Self {
        let mut starts = vec![0usize];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        Self {
            starts,
            source_len: source.len(),
        }
    }

    pub(crate) fn count(&self) -> usize {
        self.starts.len()
    }

    /// Line span `[start, end)` excluding the trailing newline.
    pub(crate) fn span(&self, line: usize) -> (usize, usize) {
        let start = self.starts[line];
        let end = self
            .starts
            .get(line + 1)
            .map_or(self.source_len, |next| next - 1);
        (start, end.max(start))
    }
}
