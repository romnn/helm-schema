//! Schema emission policy.

/// Selects how much analyzed contract evidence is emitted as JSON Schema.
///
/// Profiles change only emission. They do not change chart analysis or the
/// recovered contract. A reduced profile may remove constraints and therefore
/// widen acceptance, but must never introduce a rejection that the full
/// profile does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemaProfile {
    /// Emits every constraint supported by the schema backend.
    #[default]
    Full,
    /// Omits document-level conditional validation while preserving base
    /// path and provider constraints.
    ///
    /// This profile exists for Helm's validator, whose schema compilation
    /// cost grows superlinearly on large conditional documents.
    Lean,
}
