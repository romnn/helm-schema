use clap::ValueEnum;

/// Schema emission profiles accepted by the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum SchemaProfile {
    /// Emit every recovered constraint.
    #[default]
    Full,
    /// Omit document-level conditional validation for faster Helm schema
    /// compilation while preserving base and provider constraints.
    Lean,
}

impl From<SchemaProfile> for helm_schema::generation::SchemaProfile {
    fn from(profile: SchemaProfile) -> Self {
        match profile {
            SchemaProfile::Full => Self::Full,
            SchemaProfile::Lean => Self::Lean,
        }
    }
}
