use std::collections::BTreeSet;
use std::fmt;
use std::rc::Rc;

use helm_schema_core::ValuesPath;

use super::{Datum, FrozenValue, Leaf, TextBuffer, Value, ValueNode};

#[derive(Clone, Debug)]
pub(super) struct FrozenText(pub(super) Rc<TextTerm>);

pub(super) enum TextTerm {
    Capture {
        graph: Rc<TextBuffer>,
        strip_marker: bool,
    },
    Quote {
        value: FrozenValue,
    },
    Yaml {
        value: FrozenValue,
        pretty: bool,
    },
    Indent {
        value: FrozenValue,
        width: usize,
        newline: bool,
    },
}

impl fmt::Debug for TextTerm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capture {
                graph,
                strip_marker,
            } => formatter
                .debug_struct("Capture")
                .field("terms", &graph.parts.len())
                .field("strip_marker", strip_marker)
                .finish_non_exhaustive(),
            Self::Quote { value } => formatter
                .debug_struct("Quote")
                .field("snapshot", &value.heap.id.ordinal)
                .finish_non_exhaustive(),
            Self::Yaml { value, pretty } => formatter
                .debug_struct("Yaml")
                .field("snapshot", &value.heap.id.ordinal)
                .field("pretty", pretty)
                .finish_non_exhaustive(),
            Self::Indent {
                value,
                width,
                newline,
            } => formatter
                .debug_struct("Indent")
                .field("snapshot", &value.heap.id.ordinal)
                .field("width", width)
                .field("newline", newline)
                .finish_non_exhaustive(),
        }
    }
}

impl PartialEq for FrozenText {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for FrozenText {}

impl Value {
    pub(super) fn frozen_text(term: TextTerm, inputs: BTreeSet<ValuesPath>) -> Self {
        Self::leaf(Datum::Text(FrozenText(Rc::new(term)))).with_influences(inputs)
    }
}

pub(super) fn capture(graph: &TextBuffer, strip_marker: bool) -> Value {
    if let Some(text) = graph.exact_text() {
        return Value::string(&if strip_marker {
            text.replace("<no value>", "")
        } else {
            text
        });
    }
    let inputs = graph.inputs();
    // A widened node carries dependencies but no text structure to retain.
    if opaque(graph) {
        return Value::scalar(super::ScalarType::String, None).with_influences(inputs);
    }
    Value::frozen_text(
        TextTerm::Capture {
            graph: Rc::new(graph.clone()),
            strip_marker,
        },
        inputs,
    )
}

fn opaque(graph: &TextBuffer) -> bool {
    let [node] = graph.parts.as_slice() else {
        return false;
    };
    match node.content() {
        super::TextPart::Unknown(_) => true,
        super::TextPart::Frozen(value) => {
            value.identity().is_none()
                && matches!(
                    value.value.node.as_ref(),
                    ValueNode::Leaf(Leaf {
                        datum: Datum::Unknown(super::UnknownValue {
                            types: super::TypeCoverage::Unresolved,
                            aliases: _,
                        }),
                        origin: _,
                    })
                )
        }
        super::TextPart::Bytes(_)
        | super::TextPart::Select {
            decision: _,
            yes: _,
            no: _,
        } => false,
    }
}

enum Unresolved {
    Choice(Rc<super::FrozenTest>),
    Unknown,
    Exhausted,
}

type Outcome<'a> = dyn Fn(&super::FrozenTest) -> Option<bool> + 'a;

impl FrozenText {
    pub(super) fn projected(&self, original: &Value, outcome: &Outcome<'_>) -> Value {
        // Exhaustion publishes no partial frontier that later queries could refine again.
        self.project(original, outcome, &mut 4096)
            .unwrap_or_else(|| original.clone())
    }

    fn project(
        &self,
        original: &Value,
        outcome: &Outcome<'_>,
        remaining: &mut usize,
    ) -> Option<Value> {
        match resolve(&self.0, outcome, remaining) {
            Ok(text) => Some(Value::string(&text)),
            Err(Unresolved::Unknown) => Some(original.clone()),
            Err(Unresolved::Exhausted) => None,
            Err(Unresolved::Choice(decision)) => {
                let mut branch = |taken| {
                    self.project(
                        original,
                        &|test| {
                            if test.id == decision.id {
                                Some(taken)
                            } else {
                                outcome(test)
                            }
                        },
                        remaining,
                    )
                };
                let yes = branch(true)?;
                let no = branch(false)?;
                Some(choice(&decision, yes, no))
            }
        }
    }
}

fn resolve(
    term: &TextTerm,
    outcome: &Outcome<'_>,
    remaining: &mut usize,
) -> Result<String, Unresolved> {
    *remaining = remaining.checked_sub(1).ok_or(Unresolved::Exhausted)?;
    match term {
        TextTerm::Capture {
            graph,
            strip_marker,
        } => {
            let text = buffer_text(graph, outcome, remaining)?;
            Ok(if *strip_marker {
                text.replace("<no value>", "")
            } else {
                text
            })
        }
        TextTerm::Indent {
            value,
            width,
            newline,
        } => Ok(indent(
            &value_text(value, outcome, remaining)?,
            *width,
            *newline,
        )),
        TextTerm::Quote { value: _ }
        | TextTerm::Yaml {
            value: _,
            pretty: _,
        } => Err(Unresolved::Unknown),
    }
}

fn selected<'a, T>(
    decision: &Rc<super::FrozenTest>,
    yes: &'a T,
    no: &'a T,
    outcome: &Outcome<'_>,
) -> Result<&'a T, Unresolved> {
    outcome(decision)
        .map(|taken| if taken { yes } else { no })
        .ok_or_else(|| Unresolved::Choice(Rc::clone(decision)))
}

fn value_text(
    value: &FrozenValue,
    outcome: &Outcome<'_>,
    remaining: &mut usize,
) -> Result<String, Unresolved> {
    *remaining = remaining.checked_sub(1).ok_or(Unresolved::Exhausted)?;
    match value.value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => value_text(
            &FrozenValue {
                heap: Rc::clone(&value.heap),
                value: selected(decision, yes, no, outcome)?.clone(),
            },
            outcome,
            remaining,
        ),
        ValueNode::Leaf(Leaf {
            datum: Datum::Text(text),
            origin: _,
        }) => resolve(&text.0, outcome, remaining),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
                    go_type: _,
                    content: _,
                }
                | Datum::Map { view: _, target: _ }
                | Datum::Sequence(_)
                | Datum::Strings(_)
                | Datum::Slot {
                    backing: _,
                    index: _,
                }
                | Datum::Unknown(_),
            origin: _,
        }) => value.exact_text().ok_or(Unresolved::Unknown),
    }
}

fn buffer_text(
    buffer: &TextBuffer,
    outcome: &Outcome<'_>,
    remaining: &mut usize,
) -> Result<String, Unresolved> {
    let mut text = String::new();
    for node in &buffer.parts {
        *remaining = remaining.checked_sub(1).ok_or(Unresolved::Exhausted)?;
        let part = match node.content() {
            super::TextPart::Bytes(bytes) => bytes.clone(),
            super::TextPart::Frozen(value) => value_text(value, outcome, remaining)?,
            super::TextPart::Unknown(_) => return Err(Unresolved::Unknown),
            super::TextPart::Select { decision, yes, no } => {
                buffer_text(selected(decision, yes, no, outcome)?, outcome, remaining)?
            }
        };
        text.push_str(&part);
        if text.len() > 64 * 1024 {
            return Err(Unresolved::Unknown);
        }
    }
    Ok(text)
}

// Unknown leaves retain the original graph; specializing it again here would recurse.
fn choice(decision: &Rc<super::FrozenTest>, yes: Value, no: Value) -> Value {
    if yes == no {
        return yes;
    }
    Value {
        node: Rc::new(ValueNode::Select {
            decision: Rc::clone(decision),
            yes,
            no,
        }),
    }
}

pub(super) fn indent(text: &str, width: usize, newline: bool) -> String {
    let prefix = " ".repeat(width);
    let mut output = text
        .split('\n')
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    if newline {
        output.insert(0, '\n');
    }
    output
}

#[cfg(test)]
#[path = "../tests/runtime_text_terms.rs"]
mod tests;
