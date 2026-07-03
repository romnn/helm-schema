//! Templated-YAML frontend: one parser for the Helm "YAML with holes"
//! document language.
//!
//! A Helm template is two interleaved languages: Go template actions with a
//! real grammar (parsed by tree-sitter), and YAML layout recovered from the
//! text between actions. This crate owns the second half. It tokenizes a
//! template source into lines and action spans, then runs a layout parser
//! over the indent structure, producing a [`TemplatedDocument`] CST in which
//! YAML structure (mapping entries, sequence items, scalars, block scalars),
//! template control regions (`if`/`with`/`range`/`define`/`block` with their
//! branches), output holes, and comments are all first-class nodes carrying
//! byte spans.
//!
//! # Dependency direction
//!
//! `helm-schema-syntax` depends only on the tree-sitter grammar crate and is
//! the single owner of the raw Go-template *tree* parse
//! ([`parse_go_template`]). `helm-schema-ast` layers the typed expression AST
//! (`TemplateExpr`) on top and re-exports this crate's YAML lexical helpers;
//! it depends on this crate, never the reverse. Expression-level facts
//! (`toYaml`-ness, `nindent` widths) intentionally stay out of the CST: the
//! layout is decided by document shape alone, and expression semantics are
//! applied by consumers that already own the expression parser.
//!
//! # Layout semantics and the line-model contract
//!
//! Control actions in real charts frequently do not nest cleanly with YAML
//! structure (a mapping entry opened inside an `if` branch can adopt children
//! after `{{ end }}`). Container structure is therefore derived purely from
//! the visible YAML lines by indent discipline — action-only lines, comments,
//! and blanks are transparent to layout — and control regions are attached
//! into the tree as first-class overlay nodes. Regions whose branch bodies
//! align with whole nodes stay structured; regions that provably violate the
//! well-nested assumption are flagged (or degraded to [`Node::Opaque`] when
//! they open mid-scalar) instead of guessed at.
//!
//! open-slot semantics that `helm-schema-ast`'s attribution previously
//! recovered with an O(n²) per-query line replay; here the parse is a single
//! pass and each query is an O(depth) chain walk.

mod actions;
mod cst;
mod dump;
mod lines;
mod parse;
mod yaml_scan;

pub use actions::parse_go_template;
pub use cst::{
    BlockScalar, CommentLine, ControlBranch, ControlKind, ControlRegion, MappingEntry, Node,
    OpaqueKind, OpaqueNode, OutputAction, ScalarLine, ScalarPart, ScalarParts, SequenceItem, Span,
    TemplatedDocument,
};
pub use yaml_scan::{parse_yaml_key, structural_mapping_colon, unquote_yaml_scalar};
