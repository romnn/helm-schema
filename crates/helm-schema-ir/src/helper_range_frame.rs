#[derive(Clone)]
pub(crate) struct RangeFrame<I> {
    pub(crate) definitely_nonempty: bool,
    pub(crate) iterations: Option<Vec<I>>,
}
