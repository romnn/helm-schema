use std::fmt::Write as _;
use std::path::PathBuf;

use vfs::VfsPath;
use yaml_rust::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust::scanner::{Marker, TScalarStyle, TokenType};

use super::ChartContext;
use crate::error::{CliError, EngineResult};

const LEGACY_BOOLEAN_ALIASES: &[&str] = &[
    "y", "Y", "yes", "Yes", "YES", "n", "N", "no", "No", "NO", "on", "On", "ON", "off", "Off",
    "OFF",
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct AliasKey {
    path: String,
    line: usize,
    column: usize,
    spelling: String,
}

enum Container {
    Mapping { expecting_key: bool },
    Sequence,
}

struct AliasCollector<'a> {
    path: &'a str,
    containers: Vec<Container>,
    keys: Vec<AliasKey>,
}

impl AliasCollector<'_> {
    fn mapping_expects_key(&self) -> bool {
        matches!(
            self.containers.last(),
            Some(Container::Mapping {
                expecting_key: true
            })
        )
    }

    fn finish_node(&mut self) {
        if let Some(Container::Mapping { expecting_key }) = self.containers.last_mut() {
            *expecting_key = !*expecting_key;
        }
    }
}

impl MarkedEventReceiver for AliasCollector<'_> {
    fn on_event(&mut self, event: Event, marker: Marker) {
        match event {
            Event::Scalar(spelling, style, _, tag) => {
                if self.mapping_expects_key()
                    && style == TScalarStyle::Plain
                    && !is_explicit_string_tag(tag.as_ref())
                    && LEGACY_BOOLEAN_ALIASES.contains(&spelling.as_str())
                {
                    self.keys.push(AliasKey {
                        path: self.path.to_string(),
                        line: marker.line(),
                        column: marker.col() + 1,
                        spelling,
                    });
                }
                self.finish_node();
            }
            Event::Alias(_) => self.finish_node(),
            Event::MappingStart(_) => self.containers.push(Container::Mapping {
                expecting_key: true,
            }),
            Event::SequenceStart(_) => self.containers.push(Container::Sequence),
            Event::MappingEnd | Event::SequenceEnd => {
                self.containers.pop();
                self.finish_node();
            }
            Event::Nothing
            | Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart
            | Event::DocumentEnd => {}
        }
    }
}

fn is_explicit_string_tag(tag: Option<&TokenType>) -> bool {
    matches!(
        tag,
        Some(TokenType::Tag(handle, suffix))
            if (handle == "!!" && suffix == "str")
                || (handle.is_empty() && suffix == "tag:yaml.org,2002:str")
    )
}

pub(crate) fn reject_legacy_boolean_alias_keys(
    charts: &[ChartContext],
    values_files: &[PathBuf],
) -> EngineResult<()> {
    let mut keys = Vec::new();

    for chart in charts {
        let path = chart.chart_dir.join("values.yaml")?;
        if path.is_file()? {
            collect_vfs_keys(&path, &mut keys)?;
        }
    }
    for path in values_files {
        let source = std::fs::read_to_string(path)?;
        collect_keys(path.to_string_lossy().as_ref(), &source, &mut keys)?;
    }

    if keys.is_empty() {
        return Ok(());
    }

    keys.sort();
    keys.dedup();
    let mut details = String::new();
    for key in keys {
        let _ = writeln!(
            details,
            "  {}:{}:{}: unquoted key `{}`",
            key.path, key.line, key.column, key.spelling
        );
    }
    let _ = details.pop();
    Err(CliError::YamlBooleanAliasKeys { details })
}

fn collect_vfs_keys(path: &VfsPath, keys: &mut Vec<AliasKey>) -> EngineResult<()> {
    collect_keys(path.as_str(), &path.read_to_string()?, keys)
}

fn collect_keys(path: &str, source: &str, keys: &mut Vec<AliasKey>) -> EngineResult<()> {
    let mut collector = AliasCollector {
        path,
        containers: Vec::new(),
        keys: Vec::new(),
    };
    Parser::new(source.chars())
        .load(&mut collector, true)
        .map_err(|source| CliError::YamlBooleanAliasScan {
            path: path.to_string(),
            message: source.to_string(),
        })?;
    keys.append(&mut collector.keys);
    Ok(())
}
