use std::path::{Path, PathBuf};

use helm_schema::chart_source::RootChartSource;
use helm_schema::generation::{
    ConditionalAnchors, EmissionPolicy, EmissionPolicyDelta, EmissionSelection, SchemaProfile,
};
use helm_schema::{CliError, EngineResult};
use serde::{Deserialize, Serialize};

use crate::cli::{EmissionArgs, SchemaProfile as CliSchemaProfile};

const CONFIG_FILE_NAME: &str = "helm-schema.yaml";
const CONFIG_VERSION: u64 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(rename = "version")]
    _version: u64,
    profile: Option<ConfigProfile>,
    #[serde(default)]
    emission: ConfigEmission,
}

#[derive(Deserialize)]
struct ConfigHeader {
    version: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ConfigProfile {
    Full,
    Lean,
}

impl From<ConfigProfile> for SchemaProfile {
    fn from(profile: ConfigProfile) -> Self {
        match profile {
            ConfigProfile::Full => Self::Full,
            ConfigProfile::Lean => Self::Lean,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ConfigEmission {
    root_anchored_conditionals: Option<ConfigToggle>,
    local_conditionals: Option<ConfigToggle>,
    terminal_clauses: Option<ConfigToggle>,
    kind_partitions: Option<ConfigToggle>,
}

impl ConfigEmission {
    fn delta(&self) -> EmissionPolicyDelta {
        EmissionPolicyDelta::new(
            self.root_anchored_conditionals.map(Into::into),
            self.local_conditionals.map(Into::into),
            self.terminal_clauses.map(Into::into),
            self.kind_partitions.map(Into::into),
        )
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ConfigToggle {
    On,
    Off,
}

impl From<ConfigToggle> for bool {
    fn from(toggle: ConfigToggle) -> Self {
        matches!(toggle, ConfigToggle::On)
    }
}

struct LoadedConfig {
    path: String,
    config: ConfigFile,
    explicit: bool,
}

/// Resolved CLI composition-root policy and its visible provenance.
pub(crate) struct EffectiveConfig {
    pub(crate) selection: EmissionSelection,
    policy: EmissionPolicy,
    pub(crate) file_weakening: Vec<&'static str>,
    pub(crate) file_weakening_is_explicit: bool,
    printable: PrintableEffectiveConfig,
}

impl EffectiveConfig {
    pub(crate) fn to_yaml(&self) -> EngineResult<String> {
        Ok(serde_yaml::to_string(&self.printable)?)
    }
}

#[derive(Serialize)]
struct PrintableEffectiveConfig {
    version: u64,
    profile: PrintableField,
    emission: PrintableEmission,
}

#[derive(Serialize)]
struct PrintableEmission {
    #[serde(rename = "root-anchored-conditionals")]
    root_anchored_conditionals: PrintableField,
    #[serde(rename = "local-conditionals")]
    local_conditionals: PrintableField,
    #[serde(rename = "terminal-clauses")]
    terminal_clauses: PrintableField,
    #[serde(rename = "kind-partitions")]
    kind_partitions: PrintableField,
}

#[derive(Serialize)]
struct PrintableField {
    value: String,
    source: String,
}

#[derive(Clone)]
enum ValueSource {
    BuiltIn,
    Profile(SchemaProfile),
    File(String),
    Cli,
}

impl ValueSource {
    fn label(&self) -> String {
        match self {
            Self::BuiltIn => "built-in".to_string(),
            Self::Profile(profile) => format!("profile:{}", profile.as_str()),
            Self::File(path) => path.clone(),
            Self::Cli => "CLI".to_string(),
        }
    }
}

struct EffectiveKnob {
    value: bool,
    source: ValueSource,
}

impl EffectiveKnob {
    fn printable(self) -> PrintableField {
        PrintableField {
            value: if self.value { "on" } else { "off" }.to_string(),
            source: self.source.label(),
        }
    }
}

pub(crate) fn resolve(
    root_source: &RootChartSource,
    chart_path: &Path,
    explicit_config: Option<&Path>,
    no_config: bool,
    cli_profile: Option<CliSchemaProfile>,
    cli_emission: EmissionArgs,
) -> EngineResult<EffectiveConfig> {
    let loaded = if no_config {
        None
    } else {
        load_config(root_source, chart_path, explicit_config)?
    };
    let effective = resolve_loaded(loaded.as_ref(), cli_profile, cli_emission)?;
    let without_file = resolve_loaded(None, cli_profile, cli_emission)?;
    let file_weakening = weakened_knobs(without_file.policy, effective.policy);
    let file_weakening_is_explicit = loaded.as_ref().is_some_and(|loaded| loaded.explicit);

    Ok(EffectiveConfig {
        file_weakening,
        file_weakening_is_explicit,
        ..effective
    })
}

fn load_config(
    root_source: &RootChartSource,
    chart_path: &Path,
    explicit_config: Option<&Path>,
) -> EngineResult<Option<LoadedConfig>> {
    let (path, source, explicit) = if let Some(explicit_config) = explicit_config {
        let path = absolute_config_path(explicit_config)?;
        let source = std::fs::read_to_string(&path).map_err(|source| CliError::ConfigRead {
            path: path.clone(),
            source,
        })?;
        (path.display().to_string(), source, true)
    } else {
        let config_path = root_source.chart_dir().join(CONFIG_FILE_NAME)?;
        if !config_path.is_file()? {
            return Ok(None);
        }
        let source = config_path.read_to_string()?;
        (
            format!("{}/{}", chart_path.display(), CONFIG_FILE_NAME),
            source,
            false,
        )
    };

    let header: ConfigHeader =
        serde_yaml::from_str(&source).map_err(|source| CliError::InvalidConfig {
            path: path.clone(),
            source,
        })?;
    if header.version != CONFIG_VERSION {
        return Err(CliError::UnsupportedConfigVersion {
            path: PathBuf::from(&path),
            found: header.version,
            supported_min: CONFIG_VERSION,
            supported_max: CONFIG_VERSION,
        });
    }
    let config: ConfigFile =
        serde_yaml::from_str(&source).map_err(|source| CliError::InvalidConfig {
            path: path.clone(),
            source,
        })?;
    Ok(Some(LoadedConfig {
        path,
        config,
        explicit,
    }))
}

fn absolute_config_path(path: &Path) -> EngineResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn resolve_loaded(
    loaded: Option<&LoadedConfig>,
    cli_profile: Option<CliSchemaProfile>,
    cli_emission: EmissionArgs,
) -> EngineResult<EffectiveConfig> {
    let file_profile = loaded
        .and_then(|loaded| loaded.config.profile)
        .map(Into::into);
    let (profile, profile_source) = if let Some(cli_profile) = cli_profile {
        (SchemaProfile::from(cli_profile), ValueSource::Cli)
    } else if let (Some(profile), Some(loaded)) = (file_profile, loaded) {
        (profile, ValueSource::File(loaded.path.clone()))
    } else {
        (SchemaProfile::Full, ValueSource::BuiltIn)
    };

    let preset = profile.resolved_policy().policy();
    let mut root = EffectiveKnob {
        value: preset.root_anchored_conditionals(),
        source: ValueSource::Profile(profile),
    };
    let mut local = EffectiveKnob {
        value: preset.local_conditionals(),
        source: ValueSource::Profile(profile),
    };
    let mut terminal = EffectiveKnob {
        value: preset.terminal_clauses(),
        source: ValueSource::Profile(profile),
    };
    let mut kind = EffectiveKnob {
        value: preset.kind_partitions(),
        source: ValueSource::Profile(profile),
    };

    let file_delta = if cli_profile.is_none() {
        loaded.map(|loaded| (loaded.config.emission.delta(), loaded.path.clone()))
    } else {
        None
    };
    if let Some((delta, path)) = file_delta {
        apply_delta(
            delta,
            &ValueSource::File(path),
            &mut root,
            &mut local,
            &mut terminal,
            &mut kind,
        );
    }
    apply_delta(
        cli_emission.delta(),
        &ValueSource::Cli,
        &mut root,
        &mut local,
        &mut terminal,
        &mut kind,
    );

    let delta = EmissionPolicyDelta::new(
        explicit_override(&root),
        explicit_override(&local),
        explicit_override(&terminal),
        explicit_override(&kind),
    );
    let selection = EmissionSelection::Preset { profile, delta };
    let policy = EmissionPolicy::new(
        ConditionalAnchors::new(root.value, local.value),
        terminal.value,
        kind.value,
    )?;
    let printable = PrintableEffectiveConfig {
        version: CONFIG_VERSION,
        profile: PrintableField {
            value: profile.as_str().to_string(),
            source: profile_source.label(),
        },
        emission: PrintableEmission {
            root_anchored_conditionals: root.printable(),
            local_conditionals: local.printable(),
            terminal_clauses: terminal.printable(),
            kind_partitions: kind.printable(),
        },
    };

    Ok(EffectiveConfig {
        selection,
        policy,
        file_weakening: Vec::new(),
        file_weakening_is_explicit: false,
        printable,
    })
}

fn explicit_override(knob: &EffectiveKnob) -> Option<bool> {
    matches!(knob.source, ValueSource::File(_) | ValueSource::Cli).then_some(knob.value)
}

fn apply_delta(
    delta: EmissionPolicyDelta,
    source: &ValueSource,
    root: &mut EffectiveKnob,
    local: &mut EffectiveKnob,
    terminal: &mut EffectiveKnob,
    kind: &mut EffectiveKnob,
) {
    for (value, knob) in [
        (delta.root_anchored_conditionals(), root),
        (delta.local_conditionals(), local),
        (delta.terminal_clauses(), terminal),
        (delta.kind_partitions(), kind),
    ] {
        if let Some(value) = value {
            knob.value = value;
            knob.source = source.clone();
        }
    }
}

fn weakened_knobs(before: EmissionPolicy, after: EmissionPolicy) -> Vec<&'static str> {
    [
        (
            "root-anchored-conditionals",
            before.root_anchored_conditionals(),
            after.root_anchored_conditionals(),
        ),
        (
            "local-conditionals",
            before.local_conditionals(),
            after.local_conditionals(),
        ),
        (
            "terminal-clauses",
            before.terminal_clauses(),
            after.terminal_clauses(),
        ),
        (
            "kind-partitions",
            before.kind_partitions(),
            after.kind_partitions(),
        ),
    ]
    .into_iter()
    .filter_map(|(name, before, after)| (before && !after).then_some(name))
    .collect()
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
