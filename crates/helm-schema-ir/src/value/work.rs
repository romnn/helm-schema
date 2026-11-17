//! Work allowances for local value and object operations.

pub(super) const OBJECT_WORK: usize = 1_048_576;

pub(super) struct WorkBudget {
    pub(super) remaining: usize,
    exhausted: bool,
}

impl WorkBudget {
    pub(super) fn new(remaining: usize) -> Self {
        Self {
            remaining,
            exhausted: false,
        }
    }
    #[track_caller]
    pub(super) fn charge(&mut self, work: usize) -> bool {
        if let Some(remaining) = self.remaining.checked_sub(work) {
            self.remaining = remaining;
            true
        } else {
            self.exhaust();
            false
        }
    }

    #[track_caller]
    pub(super) fn exhaust(&mut self) {
        if !self.exhausted {
            let site = std::panic::Location::caller();
            tracing::debug_span!(
                target: "helm_schema_ir::counters::heap_store",
                "work_exhausted",
                file = site.file(),
                line = site.line(),
            )
            .in_scope(|| {});
        }
        self.remaining = 0;
        self.exhausted = true;
    }

    pub(super) fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}
