struct IrAnalysisDb {
    receiver_calls: RefCell<HashMap<String, Option<ReceiverCall>>>,
}
struct ReceiverCall {
    receiver: TemplateExpr,
    name: String,
}
struct Namespace {
    ordinal: u64,
}
struct BorrowedContext<'source> {
    database: &'source IrAnalysisDb,
    namespace: &'source Namespace,
    unrelated: u64,
}
impl Executor {
    fn helper(&self, state: State) {
        self.execute_helper_branch(state);
    }
    fn inspect(&self, state: State) {
        self.inspect_helper_branch(state);
    }
}
