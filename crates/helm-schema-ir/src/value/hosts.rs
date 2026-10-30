use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use base64::Engine as _;
use helm_schema_core::GuardValue;

use super::{
    AliasSet, Callee, Datum, Field, Invocation, Leaf, MapType, ObjectData, Origin, Remainder,
    ScalarType, State, TypeCoverage, UnknownValue, Value, ValueNode,
};
use crate::function_semantics::{Method, MethodReceiver};

pub(super) fn method(receiver: &Value, name: &str) -> Method {
    let mut pending = vec![receiver];
    let mut resolved = None;
    for _ in 0..128 {
        let Some(value) = pending.pop() else {
            return resolved.unwrap_or_else(|| Method::Foreign(name.to_owned()));
        };
        let family = match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map { view, target: _ }
                    | Datum::Unknown(UnknownValue {
                        types: TypeCoverage::Map(view),
                        aliases: _,
                    }),
                origin: _,
            }) => match view {
                MapType::Values => MethodReceiver::Values,
                MapType::Files => MethodReceiver::Files,
                MapType::ApiVersions => MethodReceiver::ApiVersions,
                MapType::KubeVersion => MethodReceiver::KubeVersion,
                MapType::Interface | MapType::String | MapType::Capabilities => {
                    MethodReceiver::Other
                }
            },
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Text(_)
                    | Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Sequence(_)
                    | Datum::Strings(_)
                    | Datum::Slot {
                        backing: _,
                        index: _,
                    }
                    | Datum::Unknown(UnknownValue {
                        types:
                            TypeCoverage::Unresolved
                            | TypeCoverage::InputNonMap
                            | TypeCoverage::Scalar(_)
                            | TypeCoverage::Sequence,
                        aliases: _,
                    }),
                origin: _,
            }) => MethodReceiver::Other,
            ValueNode::Select {
                decision: _,
                yes,
                no,
            } => {
                pending.extend([yes, no]);
                continue;
            }
        };
        let next = Method::resolve(family, name);
        if resolved.as_ref().is_some_and(|previous| previous != &next) {
            return Method::Foreign(name.to_owned());
        }
        resolved = Some(next);
    }
    Method::Foreign(name.to_owned())
}

pub(super) fn transfer(invocation: &Invocation, state: &mut State) -> Value {
    let Callee::Method {
        receiver,
        descriptor,
    } = &invocation.callee
    else {
        return Value::unknown();
    };
    let inputs = match descriptor {
        Method::ValuesAsMap | Method::ValuesTable | Method::ValuesYaml | Method::Foreign(_) => {
            state.heap.freeze(receiver).inputs()
        }
        Method::FilesGet
        | Method::FilesGetBytes
        | Method::FilesGlob
        | Method::FilesLines
        | Method::FilesAsConfig
        | Method::FilesAsSecrets
        | Method::ApiVersionsHas
        | Method::KubeVersionGitVersion
        | Method::KubeVersionString => BTreeSet::new(),
    };
    match descriptor {
        Method::ValuesAsMap => {
            let target = state.map_target(receiver);
            let nonempty = target.as_ref().is_some_and(|target| {
                match state.heap.version.object(target).map(|object| &object.data) {
                    Some(ObjectData::Map {
                        fields,
                        remainder: _,
                    }) => fields
                        .values()
                        .any(|field| matches!(state.consumed_field(field), Field::Present(_))),
                    Some(
                        ObjectData::Strings(_)
                        | ObjectData::Opaque { reachable: _ }
                        | ObjectData::Widened { inputs: _ },
                    )
                    | None => false,
                }
            });
            let converted = match target {
                Some(target) => Value::with_influences(
                    Value::leaf(Datum::Map {
                        view: MapType::Interface,
                        target: Some(target),
                    }),
                    inputs.clone(),
                ),
                None => unknown_map(MapType::Interface, inputs.clone()),
            };
            match if nonempty {
                Some(true)
            } else {
                state.truth(receiver)
            } {
                Some(true) => converted,
                Some(false) => Value::with_influences(state.heap.map(Remainder::Closed), inputs),
                None => {
                    let decision = state.test(receiver);
                    let empty = Value::with_influences(state.heap.map(Remainder::Closed), inputs);
                    Value::select(decision, converted, empty)
                }
            }
        }
        Method::ValuesTable => unknown_map(MapType::Values, inputs),
        Method::ValuesYaml => {
            Value::with_influences(Value::scalar(ScalarType::String, None), inputs)
        }
        Method::FilesGet
        | Method::FilesGetBytes
        | Method::FilesGlob
        | Method::FilesLines
        | Method::FilesAsConfig
        | Method::FilesAsSecrets => files_transfer(invocation, state),
        Method::ApiVersionsHas => readonly_unknown(TypeCoverage::Scalar(ScalarType::Bool)),
        Method::KubeVersionGitVersion => {
            let version = state.get_field(receiver, "Version");
            if state.heap.freeze(&version).go_type() == super::kinds::GoType::String {
                version
            } else {
                readonly_unknown(TypeCoverage::Scalar(ScalarType::String))
            }
        }
        Method::KubeVersionString => readonly_unknown(TypeCoverage::Scalar(ScalarType::String)),
        Method::Foreign(_) => {
            let mut reachable: Vec<_> = invocation
                .operands
                .iter()
                .map(|operand| operand.value.clone())
                .collect();
            reachable.push(receiver.clone());
            let inputs = super::operations::inputs(&reachable, state);
            super::operations::widen_operands(&reachable, state);
            Value::influenced(inputs)
        }
    }
}

fn unknown_map(
    view: MapType,
    inputs: std::collections::BTreeSet<helm_schema_core::ValuesPath>,
) -> Value {
    Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Unknown(UnknownValue {
                types: TypeCoverage::Map(view),
                aliases: AliasSet::RenderGraph,
            }),
            origin: Origin {
                identity: None,
                influences: inputs,
            },
        })),
    }
}

fn readonly_unknown(types: TypeCoverage) -> Value {
    Value::leaf(Datum::Unknown(UnknownValue {
        types,
        aliases: AliasSet::Known(BTreeSet::new()),
    }))
}

fn typed_map(mut value: Value, view: MapType) -> Value {
    match Rc::make_mut(&mut value.node) {
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    view: existing,
                    target: _,
                },
            origin: _,
        }) => *existing = view,
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => return readonly_unknown(TypeCoverage::Map(view)),
    }
    value
}

#[tracing::instrument(skip_all)]
pub(super) fn file_catalogs(
    db: &crate::analysis_db::IrAnalysisDb,
    state: &mut State,
) -> BTreeMap<String, Value> {
    let mut catalogs = BTreeMap::new();
    for (chart, files) in db.chart_files() {
        let value = state.heap.map(Remainder::Closed);
        if let Some(target) = state.map_target(&value) {
            for (path, bytes) in files {
                state.heap.write(
                    &target,
                    path,
                    Field::Present(Value::byte_sequence(bytes.clone(), None)),
                );
            }
        }
        catalogs.insert(chart.clone(), typed_map(value, MapType::Files));
    }
    catalogs
        .entry(String::new())
        .or_insert_with(|| typed_map(state.heap.map(Remainder::Closed), MapType::Files));
    catalogs
}

pub(super) fn capabilities(state: &mut State) -> Value {
    let version = state.heap.map(Remainder::Closed);
    if let Some(target) = state.map_target(&version) {
        for field in ["Version", "Major", "Minor"] {
            state.heap.write(
                &target,
                field,
                Field::Present(readonly_unknown(TypeCoverage::Scalar(ScalarType::String))),
            );
        }
    }
    let capabilities = state.heap.map(Remainder::Closed);
    if let Some(target) = state.map_target(&capabilities) {
        state.heap.write(
            &target,
            "APIVersions",
            Field::Present(readonly_unknown(TypeCoverage::Map(MapType::ApiVersions))),
        );
        state.heap.write(
            &target,
            "KubeVersion",
            Field::Present(typed_map(version, MapType::KubeVersion)),
        );
        state.heap.write(
            &target,
            "HelmVersion",
            Field::Present(readonly_unknown(TypeCoverage::Unresolved)),
        );
    }
    typed_map(capabilities, MapType::Capabilities)
}

fn catalog(receiver: &Value, state: &mut State) -> Option<BTreeMap<String, Value>> {
    let target = state.map_target(receiver)?;
    let ObjectData::Map {
        fields,
        remainder: Remainder::Closed,
    } = &state.heap.version.object(&target)?.data
    else {
        return None;
    };
    let mut files = BTreeMap::new();
    for (name, field) in fields {
        match state.consumed_field(field) {
            Field::Present(value) => {
                files.insert(name.clone(), value);
            }
            Field::Absent => {}
            Field::Select {
                decision: _,
                yes: _,
                no: _,
            }
            | Field::Unknown => return None,
        }
    }
    Some(files)
}

pub(super) fn file_field(receiver: &Value, key: &str, state: &mut State) -> Option<Value> {
    if !matches!(
        receiver.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                view: MapType::Files,
                target: _
            } | Datum::Unknown(UnknownValue {
                types: TypeCoverage::Map(MapType::Files),
                aliases: _
            }),
            origin: _,
        })
    ) {
        return None;
    }
    Some(match catalog(receiver, state) {
        Some(files) => files
            .get(key)
            .cloned()
            .unwrap_or_else(Value::nil_byte_sequence),
        None => readonly_unknown(TypeCoverage::Sequence),
    })
}

fn bytes(value: &Value) -> Option<Vec<u8>> {
    let ValueNode::Leaf(Leaf {
        datum: Datum::Sequence(sequence),
        origin: _,
    }) = value.node.as_ref()
    else {
        return None;
    };
    Some(sequence_bytes(sequence)?.to_vec())
}

pub(super) fn sequence_bytes(sequence: &super::InterfaceSequence) -> Option<&[u8]> {
    sequence
        .storage
        .as_ref()?
        .bytes()?
        .get(sequence.start..sequence.start.checked_add(sequence.length)?)
}

#[tracing::instrument(skip_all)]
fn files_transfer(invocation: &Invocation, state: &mut State) -> Value {
    let Callee::Method {
        receiver,
        descriptor,
    } = &invocation.callee
    else {
        return readonly_unknown(TypeCoverage::Unresolved);
    };
    let result_type = match descriptor {
        Method::FilesGetBytes | Method::FilesLines => TypeCoverage::Sequence,
        Method::FilesGlob => TypeCoverage::Map(MapType::Files),
        Method::FilesGet | Method::FilesAsConfig | Method::FilesAsSecrets => {
            TypeCoverage::Scalar(ScalarType::String)
        }
        Method::ValuesAsMap
        | Method::ValuesTable
        | Method::ValuesYaml
        | Method::ApiVersionsHas
        | Method::KubeVersionGitVersion
        | Method::KubeVersionString
        | Method::Foreign(_) => return readonly_unknown(TypeCoverage::Unresolved),
    };
    let Some(files) = catalog(receiver, state) else {
        return readonly_unknown(result_type);
    };
    if matches!(descriptor, Method::FilesAsConfig | Method::FilesAsSecrets) {
        return flatten_files(files, descriptor == &Method::FilesAsSecrets, state);
    }
    let Some(GuardValue::String(name)) = invocation
        .operands
        .first()
        .and_then(|operand| operand.value.constant())
    else {
        return readonly_unknown(result_type);
    };
    if descriptor == &Method::FilesGlob {
        return glob_files(files, name, state);
    }
    let file = files.get(name);
    if descriptor == &Method::FilesGetBytes {
        return file
            .cloned()
            .unwrap_or_else(|| Value::byte_sequence(Vec::new(), Some(0)));
    }
    let Some(content) = file.map_or_else(|| Some(Vec::new()), bytes) else {
        return readonly_unknown(result_type);
    };
    let Ok(text) = std::str::from_utf8(&content) else {
        return readonly_unknown(result_type);
    };
    if descriptor == &Method::FilesGet {
        return Value::string(text);
    }
    if file.is_none() {
        return string_slice(Vec::new(), state);
    }
    if text.is_empty() {
        return readonly_unknown(TypeCoverage::Sequence);
    }
    string_slice(
        text.strip_suffix('\n')
            .unwrap_or(text)
            .split('\n')
            .map(Value::string)
            .collect(),
        state,
    )
}

fn string_slice(items: Vec<Value>, state: &mut State) -> Value {
    let length = items.len();
    let backing = state
        .heap
        .allocate(ObjectData::Strings(items), super::Multiplicity::Singleton);
    Value::strings(backing, length)
}

fn glob_files(files: BTreeMap<String, Value>, pattern: &str, state: &mut State) -> Value {
    // Unsupported dialect features retain unknown coverage instead of guessing a file set.
    let recursive = pattern == "**"
        || pattern
            .strip_suffix("/**")
            .is_some_and(|prefix| !prefix.contains("**"));
    if pattern.contains(['{', '}', '[', ']', '\\']) || pattern.contains("**") && !recursive {
        return readonly_unknown(TypeCoverage::Map(MapType::Files));
    }
    let Ok(pattern) = glob::Pattern::new(pattern) else {
        return readonly_unknown(TypeCoverage::Map(MapType::Files));
    };
    let value = state.heap.map(Remainder::Closed);
    if let Some(target) = state.map_target(&value) {
        for (name, content) in files {
            if pattern.matches_with(
                &name,
                glob::MatchOptions {
                    require_literal_separator: true,
                    require_literal_leading_dot: false,
                    case_sensitive: true,
                },
            ) {
                state.heap.write(&target, &name, Field::Present(content));
            }
        }
    }
    typed_map(value, MapType::Files)
}

fn flatten_files(files: BTreeMap<String, Value>, secrets: bool, state: &mut State) -> Value {
    let mut contents = BTreeMap::new();
    for (name, value) in files {
        let Some(bytes) = bytes(&value) else {
            return readonly_unknown(TypeCoverage::Scalar(ScalarType::String));
        };
        let text = if secrets {
            base64::engine::general_purpose::STANDARD.encode(bytes)
        } else {
            let Ok(text) = String::from_utf8(bytes) else {
                return readonly_unknown(TypeCoverage::Scalar(ScalarType::String));
            };
            text
        };
        let name = name.rsplit('/').next().unwrap_or(&name).to_owned();
        if contents
            .get(&name)
            .is_some_and(|existing| existing != &text)
        {
            // Helm iterates a Go map, so colliding basenames have no single known winner.
            return readonly_unknown(TypeCoverage::Scalar(ScalarType::String));
        }
        contents.insert(name, text);
    }
    let value = state.heap.map(Remainder::Closed);
    if let Some(target) = state.map_target(&value) {
        for (name, text) in contents {
            state
                .heap
                .write(&target, &name, Field::Present(Value::string(&text)));
        }
    }
    Value::frozen_text(
        super::text::TextTerm::Yaml {
            value: state.heap.freeze(&value),
            pretty: false,
        },
        BTreeSet::new(),
    )
}

pub(super) fn as_files(value: Value) -> Value {
    typed_map(value, MapType::Files)
}

pub(super) fn go_type(view: MapType) -> super::kinds::GoType {
    match view {
        MapType::Interface | MapType::String | MapType::Values | MapType::Files => {
            super::kinds::GoType::Map
        }
        MapType::ApiVersions => super::kinds::GoType::Slice,
        MapType::Capabilities | MapType::KubeVersion => super::kinds::GoType::Unknown,
    }
}
