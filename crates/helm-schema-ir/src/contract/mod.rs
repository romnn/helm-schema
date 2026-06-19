mod document;
mod fact;
mod finalized;
mod graph;
mod projection;
mod use_claim;
mod use_semantics;

pub use document::{
    ContractDocument, ContractDocumentGuard, ContractDocumentProvenance, ContractDocumentSpan,
    ContractDocumentUse,
};
pub(crate) use fact::ContractTypeHint;
pub use finalized::FinalizedContract;
pub use graph::ContractIr;
pub use projection::ContractProjection;
pub use use_claim::ContractUse;
