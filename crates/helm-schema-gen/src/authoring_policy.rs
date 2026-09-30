//! Caller-selected authoring policy.
//!
//! A generated schema carries two kinds of constraints. Recovered constraints
//! follow from what the chart's templates, helpers, and resource sinks do
//! with a value. Authoring assertions follow from how the chart is written:
//! a closed values root reports keys no template reads, and a declared
//! default's type reports values of another type. Helm can render values that
//! an authoring assertion rejects. These settings select which assertions a
//! schema makes; they never change a recovered constraint.

use serde::Serialize;

/// The authoring assertions a generated schema makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AuthoringPolicy {
    /// Closure of every chart values root.
    pub root: RootPolicy,
    /// Whether chart-declared defaults assert their types.
    pub declared_types: DeclaredTypes,
}

/// Closure of the chart values root and every dependency instance root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RootPolicy {
    /// Reject root keys that neither a template nor a declared default names.
    #[default]
    Closed,
    /// Admit unknown root keys. Nested structural and provider closures are
    /// unaffected.
    Open,
}

/// The role of a chart-declared default's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeclaredTypes {
    /// A declared default asserts its scalar type, container kind, and item
    /// shapes where no other evidence governs the path.
    #[default]
    Assert,
    /// A declared default documents its property names and value, but
    /// asserts nothing on its own. Template and provider constraints remain.
    Annotate,
}
