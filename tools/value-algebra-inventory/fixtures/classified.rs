type SemanticAlias<'a> = &'a ScalarValueDispatch;
struct BorrowedWrapper<'a> {
    value: SemanticAlias<'a>,
}
enum BorrowedValue<'a> {
    Known(BorrowedWrapper<'a>),
}
