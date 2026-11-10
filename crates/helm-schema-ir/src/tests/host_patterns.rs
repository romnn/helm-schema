//! Test-only catalog accessors.

use super::FunctionSemantics;

impl FunctionSemantics {
    pub(crate) const fn is_known(self) -> bool {
        self.known
    }
}
