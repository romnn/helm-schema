enum PayloadShape {
    Mapping,
    Scalar,
    Sequence,
}
struct Spelling {
    shape: PayloadShape,
}
struct Layout {
    spellings: Vec<Spelling>,
}
enum HintScope {
    Unconditional,
    Guarded,
}
struct HintGrade {
    scope: HintScope,
}
struct CompletedTextUse {
    relation: Option<TextPreimage>,
}
