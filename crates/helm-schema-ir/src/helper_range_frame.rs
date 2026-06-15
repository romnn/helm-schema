#[derive(Clone)]
pub(crate) struct RangeFrame<I> {
    definitely_nonempty: bool,
    iterations: Option<Vec<I>>,
}

impl<I> RangeFrame<I> {
    pub(crate) fn unknown() -> Self {
        Self {
            definitely_nonempty: false,
            iterations: None,
        }
    }

    pub(crate) fn new(definitely_nonempty: bool, iterations: Option<Vec<I>>) -> Self {
        Self {
            definitely_nonempty,
            iterations,
        }
    }

    pub(crate) fn is_definitely_nonempty(&self) -> bool {
        self.definitely_nonempty
    }

    pub(crate) fn iteration_count(&self) -> usize {
        self.iterations.as_ref().map(Vec::len).unwrap_or(1)
    }

    pub(crate) fn iteration(&self, index: usize) -> Option<I>
    where
        I: Clone,
    {
        self.iterations
            .as_ref()
            .and_then(|iterations| iterations.get(index))
            .cloned()
    }

    pub(crate) fn has_exact_iterations(&self) -> bool {
        self.iterations.is_some()
    }
}
