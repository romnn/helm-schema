//! Destination-facing interpretation of a frozen input relationship.

use super::TextBuffer;
use super::relationship::{Decoder, Preimage, Relationship, TextFormat, ValueOperation};
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
    pub(crate) text_preimage: Option<helm_schema_core::TextPreimage>,
    pub(crate) capture: Option<(&'a Rc<TextBuffer>, &'a super::FrozenValue)>,
}
struct Transfer<'a> {
    result: RelationshipUse<'a>,
    decoder: Option<Decoder>,
    serialized: bool,
    trim: bool,
    capture_bound: bool,
    text_opaque: bool,
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
                text_preimage: None,
            },
            decoder: None,
            serialized: false,
            trim: false,
            capture_bound: false,
            text_opaque: false,
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
            capture_bound,
            text_opaque: _,
        } = transfer;
        if decoder.is_some()
            || trim
            || capture.is_some() && result.capture.is_none() && !capture_bound
        {
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
    fn operation(&mut self, operation: &ValueOperation) -> Option<()> {
        match operation {
            ValueOperation::Decode(value) if self.decoder.is_none() => {
                self.opaque_text();
                self.decoder = Some(*value);
            }
            ValueOperation::Copy
            | ValueOperation::Omit(_)
            | ValueOperation::Pick(_)
            | ValueOperation::Member(_) => {}
            ValueOperation::Format(TextFormat::Value | TextFormat::String) => {
                if self.serialized && self.decoder.is_none() {
                    return None;
                }
                self.result.stringified = true;
                self.result.kind = crate::ValueKind::Serialized;
            }
            ValueOperation::Numeric(_) | ValueOperation::Descriptor { kind_only: _ } => {
                self.opaque_text();
                self.result.kind = crate::ValueKind::Serialized;
            }
            ValueOperation::Text(transform) => {
                self.result.stringified = true;
                // A transform without a character-wise preimage (trunc, trimPrefix, trimSuffix,
                // multi-character replace) still places text derived from the source; only the
                // spelling relation is lost, so the row stays with opaque text.
                if transform
                    .preimage()
                    .and_then(|inner| self.compose_text(inner))
                    .is_none()
                {
                    self.opaque_text();
                    self.result.kind = crate::ValueKind::Serialized;
                }
            }
            ValueOperation::Trim => self.trim = true,
            ValueOperation::Capability {
                guard: _,
                decision: _,
            }
            | ValueOperation::Decode(_)
            | ValueOperation::Merge { overwrite: _ } => {
                return None;
            }
        }
        Some(())
    }

    fn compose_text(&mut self, inner: helm_schema_core::TextPreimage) -> Option<()> {
        if self.text_opaque {
            return Some(());
        }
        self.result.text_preimage = Some(match self.result.text_preimage.take() {
            Some(outer) => inner.then(&outer)?,
            None => inner,
        });
        Some(())
    }

    fn opaque_text(&mut self) {
        self.result.text_preimage = None;
        self.text_opaque = true;
    }

    fn serializer(&mut self, value: &super::FrozenValue, json: Option<bool>) -> Option<()> {
        self.opaque_text();
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
                    if !self.capture_bound
                        && let Some(value) = capture
                        && let Some(relation) = capture_preimage(graph, value, &mut 128)
                    {
                        self.compose_text(relation)?;
                        self.capture_bound = true;
                    }
                    if self.serialized {
                        return None;
                    }
                    self.result.stringified = true;
                }
            }
            TextTerm::Quote { value } => {
                self.opaque_text();
                self.result.kind = crate::ValueKind::Serialized;
                self.result.stringified = true;
                self.result.nil_omitting =
                    super::text::quote_nil_condition(value) != Some(Predicate::False);
            }
            TextTerm::Encoded {
                value: _,
                alphabet: _,
            } => {
                self.opaque_text();
                self.result.digest = true;
            }
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

fn capture_preimage(
    graph: &TextBuffer,
    source: &super::FrozenValue,
    remaining: &mut usize,
) -> Option<helm_schema_core::TextPreimage> {
    let mut relation = helm_schema_core::TextPreimage::default();
    let mut found = false;
    for node in &graph.parts {
        *remaining = remaining.checked_sub(1)?;
        let part = match node.content() {
            super::TextPart::Bytes(bytes) => Some(bytes.clone()),
            super::TextPart::Frozen(value) => {
                if std::ptr::eq(value, source) {
                    if found {
                        return None;
                    }
                    found = true;
                    continue;
                }
                if let super::ValueNode::Leaf(super::Leaf {
                    datum: super::Datum::Text(text),
                    origin: _,
                }) = value.value.node.as_ref()
                    && let TextTerm::Capture {
                        graph,
                        strip_marker: false,
                    } = text.term()
                    && let Some(inner) = capture_preimage(graph, source, remaining)
                {
                    if found {
                        return None;
                    }
                    relation.before.push_str(&inner.before);
                    relation.after.push_str(&inner.after);
                    found = true;
                    continue;
                }
                value.exact_text()
            }
            super::TextPart::Unknown(_)
            | super::TextPart::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }?;
        if relation.before.len() + relation.after.len() + part.len() > 4096 {
            return None;
        }
        if found {
            relation.after.push_str(&part);
        } else {
            relation.before.push_str(&part);
        }
    }
    found.then_some(relation)
}
