use clap::{Args, ValueEnum};
use helm_schema::generation::EmissionPolicyDelta;

/// On/off spelling shared by emission-policy CLI overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PolicyToggle {
    /// Enable the selected emission class.
    On,
    /// Disable the selected emission class.
    Off,
}

impl From<PolicyToggle> for bool {
    fn from(toggle: PolicyToggle) -> Self {
        matches!(toggle, PolicyToggle::On)
    }
}

/// Optional W-class emission overrides applied after profile and config policy.
#[derive(Args, Debug, Clone, Copy, Default)]
pub struct EmissionArgs {
    /// Override root-anchored ordinary conditional emission (W-class).
    #[arg(long, value_enum, value_name = "STATE")]
    pub root_anchored_conditionals: Option<PolicyToggle>,

    /// Override locally anchored ordinary conditional emission (W-class).
    #[arg(long, value_enum, value_name = "STATE")]
    pub local_conditionals: Option<PolicyToggle>,

    /// Override unconditional and guarded terminal-clause emission (W-class).
    #[arg(long, value_enum, value_name = "STATE")]
    pub terminal_clauses: Option<PolicyToggle>,

    /// Override kind partitions; at least one matching anchor lane must be on.
    #[arg(long, value_enum, value_name = "STATE")]
    pub kind_partitions: Option<PolicyToggle>,
}

impl EmissionArgs {
    pub(crate) fn delta(self) -> EmissionPolicyDelta {
        EmissionPolicyDelta::new(
            self.root_anchored_conditionals.map(Into::into),
            self.local_conditionals.map(Into::into),
            self.terminal_clauses.map(Into::into),
            self.kind_partitions.map(Into::into),
        )
    }
}
