//! Destination-facing interpretation of a frozen input relationship.

use super::TextBuffer;
use super::relationship::{Decoder, Operation, Preimage, Relationship};
use super::text::TextTerm;
use helm_schema_core::{Guard, Predicate, ValuePattern, ValuesPath};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::rc::Rc;

pub(crate) struct RelationshipUse<'a> {
    pub(crate) condition: Predicate,
    pub(crate) source: Cow<'a, ValuesPath>,
    pub(crate) output_path: Vec<String>,
    pub(crate) omitted_members: BTreeSet<String>,
    pub(crate) kind: crate::ValueKind,
    pub(crate) strip_marker: bool,
    pub(crate) flow_node: bool,
    pub(crate) stringified: bool,
    pub(crate) nil_omitting: bool,
    pub(crate) digest: bool,
    pub(crate) capture: Option<(&'a Rc<TextBuffer>, &'a super::FrozenValue)>,
}
struct Transfer<'a> {
    result: RelationshipUse<'a>,
    decoder: Option<Decoder>,
    serialized: bool,
    trim: bool,
}

impl<'a> Relationship<'a> {
    #[tracing::instrument(skip_all)]
    pub(crate) fn destination(self) -> Option<RelationshipUse<'a>> {
        let Self {
            condition,
            source,
            output_path,
            omitted_members,
            capture,
            preimage,
        } = self;
        let mut transfer = Transfer {
            result: RelationshipUse {
                condition,
                source,
                output_path,
                omitted_members,
                kind: crate::ValueKind::Scalar,
                strip_marker: false,
                flow_node: false,
                stringified: false,
                nil_omitting: false,
                digest: false,
                capture: None,
            },
            decoder: None,
            serialized: false,
            trim: false,
        };
        for step in preimage {
            match step {
                Preimage::TemplateSource(_) => {
                    transfer.result.kind = crate::ValueKind::TemplatedYamlSerialized;
                }
                Preimage::Operation(edge) => transfer.operation(&edge.operation)?,
                Preimage::Text(text) => transfer.text(text.term(), capture)?,
            }
        }
        let Transfer {
            mut result,
            decoder,
            serialized: _,
            trim,
        } = transfer;
        if decoder.is_some() || trim || capture.is_some() && result.capture.is_none() {
            return None;
        }
        if result.strip_marker {
            for key in &mut result.output_path {
                *key = key.replace("<no value>", "");
            }
        }
        Some(result)
    }
}

impl<'a> Transfer<'a> {
    fn operation(&mut self, operation: &Operation) -> Option<()> {
        match operation {
            Operation::Decode(value) if self.decoder.is_none() => self.decoder = Some(*value),
            Operation::Copy | Operation::Omit(_) | Operation::Pick(_) | Operation::Member(_) => {}
            Operation::Stringify => {
                if self.serialized && self.decoder.is_none() {
                    return None;
                }
                self.result.stringified = true;
                self.result.kind = crate::ValueKind::Serialized;
            }
            Operation::Numeric(_) | Operation::Descriptor { kind_only: _ } => {
                self.result.kind = crate::ValueKind::Serialized;
            }
            Operation::Trim => self.trim = true,
            Operation::Decode(_) | Operation::Merge { overwrite: _ } => return None,
        }
        Some(())
    }

    fn serializer(&mut self, value: &super::FrozenValue, json: Option<bool>) -> Option<()> {
        if let Some(decoder) = self.decoder.take() {
            let matching = match decoder {
                Decoder::YamlMap | Decoder::YamlSequence => json.is_none(),
                Decoder::JsonMap | Decoder::JsonSequence | Decoder::JsonValue => json.is_some(),
            };
            if !matching {
                return None;
            }
            self.result.condition = Predicate::all(vec![
                self.result.condition.clone(),
                decoded_shape(
                    value,
                    decoder,
                    &self.result.output_path,
                    &self.result.source,
                )?,
            ]);
        } else {
            if self.serialized {
                return None;
            }
            if self.result.kind != crate::ValueKind::TemplatedYamlSerialized {
                self.result.kind = crate::ValueKind::YamlSerialized;
            }
            self.serialized = true;
            self.result.stringified = false;
            self.result.strip_marker = !json.unwrap_or(false);
            self.result.flow_node = json.is_some();
        }
        self.trim = false;
        Some(())
    }

    fn text(&mut self, text: &'a TextTerm, capture: Option<&'a super::FrozenValue>) -> Option<()> {
        match text {
            TextTerm::Yaml { value, pretty: _ } => self.serializer(value, None)?,
            TextTerm::Json {
                value,
                pretty: _,
                escape_html,
                literal: _,
            } => self.serializer(value, Some(*escape_html))?,
            TextTerm::Capture {
                graph,
                strip_marker: _,
            } => {
                if self.decoder == Some(Decoder::YamlMap) && capture.is_some() {
                    self.result.capture = Some((graph, capture?));
                    self.result.kind = crate::ValueKind::Scalar;
                    self.result.strip_marker = false;
                    self.result.flow_node = false;
                    self.decoder = None;
                    self.serialized = false;
                    self.trim = false;
                } else if self.decoder.is_none() {
                    if self.serialized {
                        return None;
                    }
                    self.result.stringified = true;
                }
            }
            TextTerm::Quote { value: _ } => {
                self.result.kind = crate::ValueKind::Serialized;
                self.result.stringified = true;
                self.result.nil_omitting = true;
            }
            TextTerm::Encoded {
                value: _,
                alphabet: _,
            } => self.result.digest = true,
            TextTerm::Resolved {
                term: _,
                literal: _,
            }
            | TextTerm::Indent {
                value: _,
                width: _,
                newline: _,
            }
            | TextTerm::Tpl {
                source: _,
                result: _,
            } => {}
        }
        Some(())
    }
}
impl RelationshipUse<'_> {
    pub(crate) fn unchanged_condition(&self) -> Predicate {
        if self.kind != crate::ValueKind::TemplatedYamlSerialized && !self.strip_marker {
            return Predicate::True;
        }
        Predicate::Or(vec![
            Predicate::from(Guard::Absent {
                path: self.source.as_ref().clone(),
            }),
            Predicate::from(Guard::MatchesValue {
                path: self.source.as_ref().clone(),
                pattern: if self.kind == crate::ValueKind::TemplatedYamlSerialized {
                    ValuePattern::TemplateFree
                } else {
                    ValuePattern::MarkerFree
                },
            }),
        ])
    }
    pub(crate) fn flow_condition(&self) -> Predicate {
        Predicate::Or(vec![
            Predicate::from(Guard::Absent {
                path: self.source.as_ref().clone(),
            }),
            Predicate::from(Guard::MatchesValue {
                path: self.source.as_ref().clone(),
                pattern: ValuePattern::YamlFlowStable,
            }),
        ])
    }
    pub(crate) fn scalar_condition(&self) -> Predicate {
        Predicate::Or(
            ["object", "array"]
                .into_iter()
                .map(|schema_type| {
                    Predicate::from(Guard::TypeIs {
                        path: self.source.as_ref().clone(),
                        schema_type: schema_type.to_owned(),
                    })
                })
                .collect(),
        )
        .negated()
    }
}

fn decoded_shape(
    value: &super::FrozenValue,
    decoder: Decoder,
    output_path: &[String],
    source: &ValuesPath,
) -> Option<Predicate> {
    let Some(schema_type) = decoder.schema_type() else {
        return Some(Predicate::True);
    };
    if let Some(path) = value.value.identity() {
        return Some(Predicate::from(Guard::TypeIs {
            path: path.clone(),
            schema_type: schema_type.to_owned(),
        }));
    }
    match value.value.node.as_ref() {
        super::ValueNode::Leaf(super::Leaf {
            datum: super::Datum::Map { view: _, target: _ },
            origin: _,
        }) if schema_type == "object" => Some(Predicate::True),
        super::ValueNode::Leaf(super::Leaf {
            datum: super::Datum::Sequence(_) | super::Datum::Strings(_),
            origin: _,
        }) if schema_type == "array" => Some(Predicate::True),
        _ if output_path.is_empty() => Some(Predicate::from(Guard::TypeIs {
            path: source.clone(),
            schema_type: schema_type.to_owned(),
        })),
        _ => None,
    }
}
