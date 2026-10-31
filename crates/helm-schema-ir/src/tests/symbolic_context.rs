impl super::SymbolicIrContext {
    /// Executes a fresh single-template session and projects its fragment.
    #[must_use]
    pub(crate) fn eval_document_fragment(
        &self,
        src: &str,
    ) -> crate::tests::support::EvaluatedDocument {
        crate::tests::support::eval_document(src, None, &self.analysis_db)
    }
}
