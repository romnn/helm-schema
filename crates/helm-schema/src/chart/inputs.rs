use std::collections::BTreeMap;

use helm_schema_goyaml::{Document, MetadataOverlay, merge_values, values_map};
use serde_yaml::{Mapping, Value};

use super::files::{Entry, LoadedFile, strip_bom, yaml_utf8_error};
use crate::error::{CliError, EngineResult};

#[derive(Debug)]
pub(super) struct Inputs {
    pub(super) metadata: Value,
    defaults: Result<Value, ValuesError>,
    pub(super) lint: Option<Value>,
}

#[derive(Debug)]
struct ValuesError {
    path: String,
    failure: ValuesFailure,
}

#[derive(Debug)]
enum ValuesFailure {
    Utf8,
    Frame(helm_schema_syntax::HelmYamlError),
    Decode(helm_schema_goyaml::Error),
}

impl From<helm_schema_goyaml::Error> for ValuesFailure {
    fn from(error: helm_schema_goyaml::Error) -> Self {
        Self::Decode(error)
    }
}

impl Inputs {
    pub(super) fn load(
        entries: &[Entry],
        last: &mut BTreeMap<Vec<u8>, usize>,
        lint: Option<&[u8]>,
    ) -> EngineResult<Self> {
        let mut inputs = Self {
            metadata: Value::Mapping(Mapping::new()),
            defaults: Ok(Value::Null),
            lint: None,
        };
        let mut source = None;
        let mut overlay = MetadataOverlay::default();
        let mut physical_lint = lint.map(strip_bom);
        for Entry { file, source: _ } in entries
            .iter()
            .filter(|entry| entry.file.name == b"Chart.yaml")
        {
            let doc = read(file)?;
            overlay
                .apply(&mut inputs.metadata, &doc.value)
                .map_err(|error| invalid(&file.path, error.to_string()))?;
            if let Some(metadata) = inputs.metadata.as_mapping_mut()
                && metadata
                    .get("apiVersion")
                    .is_none_or(|value| value.is_null() || value.as_str() == Some(""))
            {
                metadata.insert(Value::from("apiVersion"), Value::from("v1"));
            }
            source = Some(&file.path);
        }
        for (index, Entry { file, source: _ }) in entries.iter().enumerate() {
            match file.name.as_slice() {
                b"requirements.yaml" => {
                    let doc = read(file)?;
                    overlay
                        .apply(&mut inputs.metadata, &doc.value)
                        .map_err(|error| invalid(&file.path, error.to_string()))?;
                    source = Some(&file.path);
                }
                b"values.yaml" => {
                    inputs.defaults = inputs.defaults.and_then(|_| {
                        read_values(file, &mut physical_lint, &mut inputs.lint).map_err(|failure| {
                            ValuesError {
                                path: file.path.clone(),
                                failure,
                            }
                        })
                    });
                }
                b"requirements.lock" => {}
                _ => continue,
            }
            if matches!(
                file.name.as_slice(),
                b"requirements.yaml" | b"requirements.lock"
            ) && inputs.metadata.get("apiVersion").and_then(Value::as_str) == Some("v1")
            {
                last.insert(file.name.clone(), index);
            }
        }
        if let Some(lint) = physical_lint {
            inputs.lint = helm_schema_goyaml::unmarshal(lint)
                .and_then(|value| values_map(&value))
                .ok();
        }
        if let Some(source) = source {
            validate(&mut inputs.metadata, source)?;
        }
        Ok(inputs)
    }

    pub(super) fn defaults(&self) -> EngineResult<&Value> {
        self.defaults
            .as_ref()
            .map_err(|ValuesError { path, failure }| match failure {
                ValuesFailure::Utf8 => yaml_utf8_error(path),
                ValuesFailure::Frame(error) => invalid(path, error.to_string()),
                ValuesFailure::Decode(error) => invalid(path, error.to_string()),
            })
    }
}

fn read_values(
    file: &LoadedFile,
    physical_lint: &mut Option<&[u8]>,
    lint: &mut Option<Value>,
) -> Result<Value, ValuesFailure> {
    let text = std::str::from_utf8(&file.bytes).map_err(|_| ValuesFailure::Utf8)?;
    let mut defaults = Value::Mapping(Mapping::new());
    for (index, frame) in helm_schema_syntax::transport_documents(text)
        .into_iter()
        .enumerate()
    {
        let frame = frame.map_err(ValuesFailure::Frame)?;
        let projected = values_map(&helm_schema_goyaml::unmarshal(frame.as_bytes())?)?;
        // Lint reads the physical root file directly; only identical decoder input can share.
        if index == 0 && *physical_lint == Some(strip_bom(frame.as_bytes())) {
            *physical_lint = None;
            *lint = Some(projected.clone());
        }
        merge_values(&mut defaults, projected);
    }
    Ok(defaults)
}

fn read(file: &LoadedFile) -> EngineResult<Document> {
    helm_schema_goyaml::read_document(file.yaml_text()?.as_bytes())
        .map_err(|error| invalid(&file.path, error.to_string()))
}

fn invalid(path: &str, message: String) -> CliError {
    CliError::ChartDecode {
        path: path.to_owned(),
        message,
    }
}

fn validate(metadata: &mut Value, path: &str) -> EngineResult<()> {
    let Some(map) = metadata.as_mapping_mut() else {
        return Ok(());
    };
    for field in [
        "name",
        "home",
        "description",
        "icon",
        "condition",
        "tags",
        "appVersion",
        "kubeVersion",
    ] {
        sanitize(map.get_mut(field));
    }
    let error = |message: &str| CliError::ChartDecode {
        path: path.to_owned(),
        message: message.to_owned(),
    };
    for field in ["name", "apiVersion", "version"] {
        if map
            .get(field)
            .is_none_or(|value| value.is_null() || value.as_str() == Some(""))
        {
            return Err(error(&format!("chart metadata {field} is empty")));
        }
    }
    if let Some(name) = map.get("name").and_then(Value::as_str)
        && (matches!(name, "." | "..")
            || name.contains('/')
            || cfg!(windows) && name.contains('\\'))
    {
        return Err(error("invalid chart name"));
    }
    if let Some(version) = map.get("version").and_then(Value::as_str)
        && !valid_version(version)
    {
        return Err(error("invalid chart version"));
    }
    if let Some(kind) = map.get("type").and_then(Value::as_str)
        && !matches!(kind, "" | "application" | "library")
    {
        return Err(error("invalid chart type"));
    }
    for field in ["sources", "keywords", "maintainers", "dependencies"] {
        if let Some(items) = map.get_mut(field).and_then(Value::as_sequence_mut) {
            for item in items {
                if let Some(fields) = item.as_mapping_mut() {
                    for key in ["name", "version", "repository", "condition", "email", "url"] {
                        sanitize(fields.get_mut(key));
                    }
                    if let Some(tags) = fields.get_mut("tags").and_then(Value::as_sequence_mut) {
                        for tag in tags {
                            sanitize(Some(tag));
                        }
                    }
                    if let Some(alias) = fields.get("alias").and_then(Value::as_str)
                        && !alias
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                    {
                        return Err(error("invalid dependency alias"));
                    }
                } else if item.is_null() && matches!(field, "maintainers" | "dependencies") {
                    return Err(error("null chart metadata element"));
                } else {
                    sanitize(Some(item));
                }
            }
        }
    }
    Ok(())
}

fn sanitize(value: Option<&mut Value>) {
    if let Some(value) = value
        && let Some(text) = value.as_str()
    {
        let sanitized: String = text
            .chars()
            .filter_map(|ch| {
                if ch.is_whitespace() {
                    Some(' ')
                } else {
                    (!ch.is_control()).then_some(ch)
                }
            })
            .collect();
        if sanitized != text {
            *value = Value::String(sanitized);
        }
    }
}

fn valid_version(version: &str) -> bool {
    if version.len() > 256 {
        return false;
    }
    let version = version.strip_prefix('v').unwrap_or(version);
    let (core, tail) = version.split_at(version.find(['-', '+']).unwrap_or(version.len()));
    let mut parts = core.split('.').map(str::parse::<u64>);
    let (Some(Ok(major)), Some(Ok(minor)), Some(Ok(patch)), None) = (
        parts.next(),
        parts.next().or(Some(Ok(0))),
        parts.next().or(Some(Ok(0))),
        parts.next(),
    ) else {
        return false;
    };
    semver::Version::parse(&format!("{major}.{minor}.{patch}{tail}")).is_ok()
}

#[cfg(test)]
#[path = "tests/inputs.rs"]
mod tests;
