//! Helm's chart loader (`pkg/chart/v2/loader`), reduced to the files that
//! decide values: `Chart.yaml`, `requirements.yaml`, `values.yaml`, the
//! lock files it validates, and the dependencies under `charts/`, unpacked
//! or packaged.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde_json::{Map, Value};

use super::ValuesError;
use super::go_value::{Table, table_from_json};
use super::ignore::Rules;
use super::semver::Version;
use super::yaml;

/// Helm's per-file and per-archive decompressed size limits.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct Chart {
    /// `Metadata.Name`; an alias instance carries its alias.
    pub(super) name: String,
    pub(super) version: String,
    pub(super) values: Table,
    /// `Dependencies()`: the loaded charts.
    pub(super) dependencies: Vec<Chart>,
    /// How many leading `dependencies` Helm orders by iterating its
    /// loader's Go map, i.e. in no fixed order: every loaded chart until
    /// `processDependencyEnabled` puts the requirement-matched instances,
    /// in requirement order, after the unmatched ones.
    pub(super) unordered: usize,
    /// `Metadata.Dependencies`; `None` is Go's nil slice.
    pub(super) requirements: Option<Vec<Requirement>>,
}

#[derive(Clone)]
pub(super) struct Requirement {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) alias: String,
    pub(super) condition: String,
    pub(super) tags: Vec<String>,
    pub(super) import_values: Vec<ImportValue>,
    pub(super) enabled: bool,
}

#[derive(Clone)]
pub(super) enum ImportValue {
    /// `- name`: the child's `exports.name` table at the parent's root.
    Exports(String),
    /// `- {child, parent}`: the child's table at the parent's path.
    Table { child: String, parent: String },
}

enum Source {
    Disk(PathBuf),
    Memory(Vec<u8>),
}

struct ChartFile {
    name: String,
    source: Source,
}

impl ChartFile {
    fn read(&self) -> Result<Vec<u8>, ValuesError> {
        match &self.source {
            Source::Disk(path) => std::fs::read(path).map_err(|source| ValuesError::Io {
                path: path.display().to_string(),
                source,
            }),
            Source::Memory(bytes) => Ok(bytes.clone()),
        }
    }
}

/// `loader.LoadDir`: the chart directory filtered by its `.helmignore`.
pub(super) fn load_directory(chart_dir: &Path) -> Result<Chart, ValuesError> {
    let helmignore = chart_dir.join(".helmignore");
    let source = if helmignore.is_file() {
        std::fs::read_to_string(&helmignore).map_err(|source| ValuesError::Io {
            path: helmignore.display().to_string(),
            source,
        })?
    } else {
        String::new()
    };
    let rules = Rules::parse(&source)?;
    let mut files = Vec::new();
    collect_files(chart_dir, "", &rules, &mut files)?;
    load_files(files)
}

fn collect_files(
    dir: &Path,
    prefix: &str,
    rules: &Rules,
    files: &mut Vec<ChartFile>,
) -> Result<(), ValuesError> {
    let io = |source| ValuesError::Io {
        path: dir.display().to_string(),
        source,
    };
    let mut entries = std::fs::read_dir(dir)
        .map_err(io)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    entries.sort();
    for path in entries {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(ValuesError::Unmodelled(format!(
                "non-UTF-8 path {}",
                path.display()
            )));
        };
        let name = format!("{prefix}{file_name}");
        let metadata = std::fs::metadata(&path).map_err(|source| ValuesError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if metadata.is_dir() {
            if !rules.ignores(&name, true) {
                collect_files(&path, &format!("{name}/"), rules, files)?;
            }
            continue;
        }
        if rules.ignores(&name, false) {
            continue;
        }
        if !metadata.is_file() {
            return Err(ValuesError::NotValidated(format!(
                "Helm cannot load irregular file {name}"
            )));
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(ValuesError::NotValidated(format!(
                "Helm refuses {name}: larger than the maximum file size"
            )));
        }
        files.push(ChartFile {
            name,
            source: Source::Disk(path),
        });
    }
    Ok(())
}

/// `loader.LoadArchive`: a packaged chart; `.helmignore` does not apply.
fn load_archive(bytes: &[u8], label: &str) -> Result<Chart, ValuesError> {
    let io = |source| ValuesError::Io {
        path: label.to_string(),
        source,
    };
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut files = Vec::new();
    let mut remaining = MAX_ARCHIVE_BYTES;
    for entry in archive.entries().map_err(io)? {
        let mut entry = entry.map_err(io)?;
        let kind = entry.header().entry_type();
        if kind.is_dir() || kind.is_pax_global_extensions() || kind.is_pax_local_extensions() {
            continue;
        }
        let path = entry
            .path()
            .map_err(io)?
            .to_string_lossy()
            .replace('\\', "/");
        let mut parts = path.split('/');
        if parts.next() == Some("Chart.yaml") {
            return Err(ValuesError::NotValidated(format!(
                "{label}: chart yaml not in base directory"
            )));
        }
        let mut name = Vec::new();
        for part in parts {
            match part {
                "" | "." => {}
                ".." => {
                    return Err(ValuesError::Unmodelled(format!(
                        "{label}: archive member {path}"
                    )));
                }
                part => name.push(part),
            }
        }
        if name.is_empty() {
            return Err(ValuesError::NotValidated(format!(
                "{label}: chart illegally contains content outside the base directory: {path}"
            )));
        }
        let size = entry.size();
        if size > remaining || size > MAX_FILE_BYTES {
            return Err(ValuesError::NotValidated(format!(
                "{label}: {path} is larger than the maximum size"
            )));
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(io)?;
        remaining -= size;
        files.push(ChartFile {
            name: name.join("/"),
            source: Source::Memory(data),
        });
    }
    if files.is_empty() {
        return Err(ValuesError::NotValidated(format!(
            "{label}: no files in chart archive"
        )));
    }
    load_files(files)
}

/// `loader.LoadFiles`.
fn load_files(files: Vec<ChartFile>) -> Result<Chart, ValuesError> {
    let Some(manifest) = files.iter().find(|file| file.name == "Chart.yaml") else {
        return Err(ValuesError::NotValidated(
            "Chart.yaml file is missing".to_string(),
        ));
    };
    let manifest = single_map(&manifest.read()?, "Chart.yaml")?;
    let api_version = string_field(&manifest, "apiVersion")?;
    if !matches!(api_version.as_str(), "" | "v1" | "v2") {
        return Err(ValuesError::Unmodelled(format!(
            "chart apiVersion {api_version:?}"
        )));
    }
    let name = string_field(&manifest, "name")?;
    let version = string_field(&manifest, "version")?;
    let chart_type = string_field(&manifest, "type")?;
    let mut requirements = requirements(manifest.get("dependencies"))?;

    let mut values = Map::new();
    let mut subcharts: BTreeMap<String, Vec<ChartFile>> = BTreeMap::new();
    for file in files {
        match file.name.as_str() {
            "values.yaml" => values = load_values(&file.read()?, "values.yaml")?,
            "requirements.yaml" => {
                let manifest = single_map(&file.read()?, "requirements.yaml")?;
                if manifest.contains_key("dependencies") {
                    requirements = self::requirements(manifest.get("dependencies"))?;
                }
            }
            "Chart.lock" | "requirements.lock" => validate_lock(&file.read()?, &file.name)?,
            _ => {
                let Some(name) = file.name.strip_prefix("charts/") else {
                    continue;
                };
                if Path::new(name).extension().is_some_and(|ext| ext == "prov") {
                    continue;
                }
                let chart = name.split('/').next().unwrap_or_default().to_string();
                subcharts.entry(chart).or_default().push(ChartFile {
                    name: name.to_string(),
                    source: file.source,
                });
            }
        }
    }

    validate_metadata(&name, &version, &chart_type, requirements.as_deref())?;
    let dependencies = load_subcharts(&name, subcharts)?;

    Ok(Chart {
        name,
        version,
        values: table_from_json(values),
        unordered: dependencies.len(),
        dependencies,
        requirements,
    })
}

/// `Metadata.Validate`, for the fields that decide whether and how Helm
/// loads the chart's values.
fn validate_metadata(
    name: &str,
    version: &str,
    chart_type: &str,
    requirements: Option<&[Requirement]>,
) -> Result<(), ValuesError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(ValuesError::NotValidated(format!(
            "invalid chart name {name:?}"
        )));
    }
    if Version::parse(version).is_none() {
        return Err(ValuesError::NotValidated(format!(
            "chart {name}: invalid chart version {version:?}"
        )));
    }
    if !matches!(chart_type, "" | "application" | "library") {
        return Err(ValuesError::NotValidated(format!(
            "chart {name}: chart.metadata.type must be application or library"
        )));
    }
    let mut keys = Vec::new();
    for requirement in requirements.into_iter().flatten() {
        let key = if requirement.alias.is_empty() {
            &requirement.name
        } else {
            &requirement.alias
        };
        if keys.contains(&key) {
            return Err(ValuesError::NotValidated(format!(
                "chart {name}: more than one dependency with name or alias {key:?}"
            )));
        }
        keys.push(key);
    }
    Ok(())
}

/// The `charts/` entries of `LoadFiles`. Helm walks them in random map
/// order; name order is one of the orders Helm may take.
fn load_subcharts(
    name: &str,
    subcharts: BTreeMap<String, Vec<ChartFile>>,
) -> Result<Vec<Chart>, ValuesError> {
    let mut dependencies = Vec::new();
    for (subchart, files) in subcharts {
        if subchart.starts_with(['_', '.']) {
            continue;
        }
        let label = format!("chart {name}: subchart {subchart}");
        if Path::new(&subchart)
            .extension()
            .is_some_and(|ext| ext == "tgz")
        {
            let Some(archive) = files.first().filter(|file| file.name == subchart) else {
                return Err(ValuesError::NotValidated(format!(
                    "error unpacking subchart tar {subchart} in {name}"
                )));
            };
            dependencies.push(load_archive(&archive.read()?, &label)?);
        } else {
            let mut nested = Vec::new();
            for file in files {
                if let Some((_, rest)) = file.name.split_once('/') {
                    nested.push(ChartFile {
                        name: rest.to_string(),
                        source: file.source,
                    });
                }
            }
            dependencies.push(load_files(nested)?);
        }
    }
    Ok(dependencies)
}

/// `loader.LoadValues`: every YAML document of a values file, merged.
fn load_values(bytes: &[u8], file: &str) -> Result<Map<String, Value>, ValuesError> {
    let mut values = Map::new();
    for document in yaml::documents(bytes, file)? {
        match document {
            Value::Null => {}
            Value::Object(document) => merge_maps(&mut values, document),
            _ => {
                return Err(ValuesError::NotValidated(format!(
                    "cannot load {file}: a document is not a map"
                )));
            }
        }
    }
    Ok(values)
}

/// `common.ReadValuesFile`, which `helm lint`'s values rule uses: the
/// FIRST YAML document only.
pub(super) fn read_values_file(path: &Path) -> Result<Map<String, Value>, ValuesError> {
    let bytes = std::fs::read(path).map_err(|source| ValuesError::Io {
        path: path.display().to_string(),
        source,
    })?;
    match yaml::documents(&bytes, "values.yaml")?.into_iter().next() {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(values)) => Ok(values),
        Some(_) => Err(ValuesError::NotValidated(
            "values.yaml does not parse as a map".to_string(),
        )),
    }
}

/// `MergeMaps`: later documents win, tables merge.
pub(super) fn merge_maps(base: &mut Map<String, Value>, overlay: Map<String, Value>) {
    for (key, value) in overlay {
        if let (Value::Object(overlay), Some(Value::Object(existing))) =
            (&value, base.get_mut(&key))
        {
            merge_maps(existing, overlay.clone());
            continue;
        }
        base.insert(key, value);
    }
}

/// A metadata file Helm decodes with `yaml.Unmarshal`: its first document,
/// which must be a map (or empty).
fn single_map(bytes: &[u8], file: &str) -> Result<Map<String, Value>, ValuesError> {
    match yaml::documents(bytes, file)?.into_iter().next() {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(map)) => Ok(map),
        Some(_) => Err(ValuesError::NotValidated(format!("cannot load {file}"))),
    }
}

/// `Chart.lock` and `requirements.lock` must decode into Helm's `Lock`, or
/// the chart does not load.
fn validate_lock(bytes: &[u8], file: &str) -> Result<(), ValuesError> {
    let lock = single_map(bytes, file)?;
    match lock.get("generated") {
        None | Some(Value::Null) => {}
        Some(Value::String(generated)) if is_rfc3339(generated) => {}
        Some(Value::String(generated)) => {
            return Err(ValuesError::Unmodelled(format!(
                "{file}: generated {generated:?} is not an RFC 3339 time this port reads"
            )));
        }
        Some(other) => {
            return Err(ValuesError::NotValidated(format!(
                "cannot load {file}: generated {other} is not a time"
            )));
        }
    }
    match lock.get("digest") {
        None | Some(Value::Null | Value::String(_)) => {}
        Some(other) => {
            return Err(ValuesError::NotValidated(format!(
                "cannot load {file}: digest {other}"
            )));
        }
    }
    requirements(lock.get("dependencies"))?;
    Ok(())
}

/// `YYYY-MM-DDTHH:MM:SS[.fraction](Z|±HH:MM)`, as Go's RFC 3339 parser
/// reads a `time.Time`.
fn is_rfc3339(text: &str) -> bool {
    let bytes = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        bytes
            .get(range)
            .is_some_and(|part| part.iter().all(u8::is_ascii_digit))
    };
    let at = |index: usize, byte: u8| bytes.get(index) == Some(&byte);
    if !(digits(0..4)
        && at(4, b'-')
        && digits(5..7)
        && at(7, b'-')
        && digits(8..10)
        && at(10, b'T')
        && digits(11..13)
        && at(13, b':')
        && digits(14..16)
        && at(16, b':')
        && digits(17..19))
    {
        return false;
    }
    let mut index = 19;
    if at(index, b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    match bytes.get(index) {
        Some(b'Z') => index + 1 == bytes.len(),
        Some(b'+' | b'-') => {
            digits(index + 1..index + 3)
                && at(index + 3, b':')
                && digits(index + 4..index + 6)
                && index + 6 == bytes.len()
        }
        _ => false,
    }
}

/// A string field of a Helm metadata struct; absent or null is Go's "".
fn string_field(object: &Map<String, Value>, key: &str) -> Result<String, ValuesError> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(other) => Err(ValuesError::Unmodelled(format!(
            "non-string chart metadata field {key}: {other}"
        ))),
    }
}

fn requirements(dependencies: Option<&Value>) -> Result<Option<Vec<Requirement>>, ValuesError> {
    let entries = match dependencies {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(entries)) => entries,
        Some(other) => {
            return Err(ValuesError::Unmodelled(format!(
                "chart dependencies {other}"
            )));
        }
    };
    let mut requirements = Vec::new();
    for entry in entries {
        let Value::Object(entry) = entry else {
            return Err(ValuesError::NotValidated(
                "dependencies must not contain empty or null nodes".to_string(),
            ));
        };
        requirements.push(Requirement {
            name: string_field(entry, "name")?,
            version: string_field(entry, "version")?,
            alias: string_field(entry, "alias")?,
            // `Dependency.Validate` maps every whitespace character to a space.
            condition: string_field(entry, "condition")?
                .chars()
                .map(|character| {
                    if character.is_whitespace() {
                        ' '
                    } else {
                        character
                    }
                })
                .collect(),
            tags: tags(entry.get("tags"))?,
            import_values: import_values(entry.get("import-values"))?,
            enabled: false,
        });
    }
    Ok(Some(requirements))
}

fn tags(tags: Option<&Value>) -> Result<Vec<String>, ValuesError> {
    let values = match tags {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(values)) => values,
        Some(other) => {
            return Err(ValuesError::Unmodelled(format!("dependency tags {other}")));
        }
    };
    let mut tags = Vec::new();
    for tag in values {
        let Value::String(tag) = tag else {
            return Err(ValuesError::Unmodelled(format!("dependency tag {tag}")));
        };
        tags.push(tag.clone());
    }
    Ok(tags)
}

fn import_values(imports: Option<&Value>) -> Result<Vec<ImportValue>, ValuesError> {
    let values = match imports {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(values)) => values,
        Some(other) => {
            return Err(ValuesError::Unmodelled(format!("import-values {other}")));
        }
    };
    let mut imports = Vec::new();
    for value in values {
        match value {
            Value::String(name) => imports.push(ImportValue::Exports(name.clone())),
            Value::Object(table) => {
                let (Some(Value::String(child)), Some(Value::String(parent))) =
                    (table.get("child"), table.get("parent"))
                else {
                    return Err(ValuesError::Unmodelled(format!(
                        "import-values entry {value}"
                    )));
                };
                imports.push(ImportValue::Table {
                    child: child.clone(),
                    parent: parent.clone(),
                });
            }
            other => {
                return Err(ValuesError::Unmodelled(format!(
                    "import-values entry {other}"
                )));
            }
        }
    }
    Ok(imports)
}
