//! `encoding/json` assignment of converted chart inputs, retaining uncertainty at its source.

use std::collections::HashMap;

use serde_yaml::{
    Mapping, Value as Yaml,
    value::{Tag, TaggedValue},
};

use crate::Error;
use crate::decode::{
    Candidate, Json, Member, Shape, Value, convert, go_json_string, go_signed_float,
    go_unsigned_float, json_error,
};

/// A chart value whose Go interpretation cannot be projected.
fn unknown() -> Yaml {
    tagged("!unknown", Yaml::Null)
}

fn tagged(tag: &str, value: Yaml) -> Yaml {
    Yaml::Tagged(Box::new(TaggedValue {
        tag: Tag::new(tag),
        value,
    }))
}

/// Whether a dependency identity or control keeps the direct YAML spelling Helm's
/// pre-loader reader recognized.
#[must_use]
pub fn is_authored(value: &Yaml) -> bool {
    matches!(value, Yaml::Tagged(tagged) if tagged.tag == "!authored")
}

/// Unmarshals one values document into Helm's `map[string]interface{}` target
/// (`pkg/chart/v2/loader/load.go:212-248`).
///
/// # Errors
///
/// Returns invalid JSON keys, nonfinite values, or a non-map document.
pub fn values_map(value: &Value) -> Result<Yaml, Error> {
    let json = convert(value, Shape::Values)?;
    json.marshal()?;
    let mut values = Yaml::Mapping(Mapping::new());
    if !matches!(json, Json::Null) {
        MetadataOverlay::default().assign(
            &mut values,
            &json,
            Shape::Values,
            &mut Vec::new(),
            false,
        )?;
    }
    Ok(values)
}

/// Retains previously initialized Go slice slots across metadata overlays.
#[derive(Default)]
pub struct MetadataOverlay {
    spare: HashMap<Vec<Yaml>, Vec<Yaml>>,
}

impl MetadataOverlay {
    /// Unmarshals one metadata document into the existing Go-shaped struct.
    ///
    /// # Errors
    ///
    /// Returns invalid JSON keys, nonfinite values, or incompatible field types.
    pub fn apply(&mut self, destination: &mut Yaml, incoming: &Value) -> Result<(), Error> {
        let json = convert(incoming, Shape::Chart)?;
        json.marshal()?;
        self.assign(destination, &json, Shape::Chart, &mut Vec::new(), true)
    }

    /// Decodes into an existing value as `encoding/json` does (`decode.go:380-1080`).
    ///
    /// `direct` holds while every key on the path to a dependency identity or control
    /// was spelled exactly and directly.
    fn assign(
        &mut self,
        destination: &mut Yaml,
        incoming: &Json,
        shape: Shape,
        path: &mut Vec<Yaml>,
        direct: bool,
    ) -> Result<(), Error> {
        match (incoming, shape) {
            (Json::Null, Shape::Text | Shape::Bool | Shape::Chart) => {}
            (Json::Null, _) => {
                self.forget(path);
                *destination = Yaml::Null;
            }
            (Json::String(text), Shape::Text) if direct => {
                *destination = tagged("!authored", Yaml::from(text.as_str()));
            }
            (Json::String(text) | Json::Coerced(text), Shape::Text | Shape::Any) => {
                *destination = Yaml::from(text.as_str());
            }
            (Json::Bool(value), Shape::Bool | Shape::Any) => *destination = Yaml::Bool(*value),
            (Json::Int(value), Shape::Any) => *destination = number(go_signed_float(*value)),
            (Json::Uint(value), Shape::Any) => *destination = number(go_unsigned_float(*value)),
            (Json::Float(bits), Shape::Any) => *destination = number(f64::from_bits(*bits)),
            (Json::Array(items), Shape::Any | Shape::List) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    let mut value = Yaml::Null;
                    self.assign(&mut value, item, Shape::Any, path, false)?;
                    values.push(value);
                }
                *destination = Yaml::Sequence(values);
            }
            (Json::Array(items), Shape::Texts | Shape::Dependencies | Shape::Maintainers) => {
                self.slice(destination, items, shape, path, direct)?;
            }
            (Json::Object(members), shape) if shape.is_object() => {
                if shape == Shape::Any || !destination.is_mapping() {
                    *destination = Yaml::Mapping(Mapping::new());
                }
                for member in members {
                    self.member(destination, member, shape, path, direct)?;
                }
            }
            _ => return Err(json_error(format!("cannot unmarshal JSON into {shape:?}"))),
        }
        Ok(())
    }

    /// Reuses initialized slots past the current length (`encoding/json/decode.go:540-590`).
    fn slice(
        &mut self,
        destination: &mut Yaml,
        items: &[Json],
        shape: Shape,
        path: &mut Vec<Yaml>,
        direct: bool,
    ) -> Result<(), Error> {
        if items.is_empty() {
            self.forget(path);
            *destination = Yaml::Sequence(Vec::new());
            return Ok(());
        }
        let mut slots = match std::mem::take(destination) {
            Yaml::Sequence(items) => items,
            _ => Vec::new(),
        };
        if let Some(spare) = self.spare.remove(path) {
            slots.extend(spare);
        }
        let count = items.len();
        slots.resize_with(slots.len().max(count), || Yaml::Null);
        for (index, (slot, item)) in slots.iter_mut().zip(items).enumerate() {
            path.push(Yaml::from(index));
            self.assign(slot, item, shape.element(), path, direct)?;
            path.pop();
            if shape == Shape::Texts && slot.is_null() {
                *slot = Yaml::from("");
            }
        }
        if slots.len() > count {
            self.spare.insert(path.clone(), slots.split_off(count));
        }
        *destination = Yaml::Sequence(slots);
        Ok(())
    }

    /// Assigns one object member in marshaled order. Struct fields decode in place, map
    /// elements decode into a fresh value, and unknown struct fields are ignored.
    fn member(
        &mut self,
        destination: &mut Yaml,
        Member { raw, candidates }: &Member,
        shape: Shape,
        path: &mut Vec<Yaml>,
        direct: bool,
    ) -> Result<(), Error> {
        let key = go_json_string(raw);
        let (name, field, fresh) = match shape {
            Shape::TextMap => (key.as_str(), Shape::Text, Some(Yaml::from(""))),
            _ if shape.fields().is_empty() => (key.as_str(), Shape::Any, Some(Yaml::Null)),
            _ => match shape.field(&key) {
                Some((name, field)) => (name, field, None),
                None => return Ok(()),
            },
        };
        let Some(object) = destination.as_mapping_mut() else {
            return Err(json_error("missing JSON object destination"));
        };
        let slot = object.entry(Yaml::from(name)).or_insert(Yaml::Null);
        if let Some(fresh) = fresh {
            *slot = fresh;
        }
        path.push(Yaml::from(name));
        let result = if let [
            Candidate {
                value,
                direct: spelled,
            },
        ] = candidates.as_slice()
        {
            let direct = direct && *spelled && key == name && provenance(shape, name);
            self.assign(slot, value, field, path, direct)
        } else {
            winners(slot, candidates, field)
        };
        path.pop();
        result
    }

    /// Drops spare slots of a slice that is replaced or released.
    fn forget(&mut self, path: &[Yaml]) {
        if !self.spare.is_empty() {
            self.spare.retain(|cached, _| !cached.starts_with(path));
        }
    }
}

/// Dependency identities and controls carry their spelling for the legacy hint.
fn provenance(shape: Shape, field: &str) -> bool {
    matches!(
        (shape, field),
        (Shape::Chart, "dependencies")
            | (Shape::Dependency, "name" | "alias" | "condition" | "tags")
    )
}

/// Decodes each possible winner of a converted key on its own and keeps the facts every
/// successful outcome shares. The document fails only when every winner fails.
fn winners(slot: &mut Yaml, candidates: &[Candidate], field: Shape) -> Result<(), Error> {
    let mut joined = None;
    let mut failure = None;
    for Candidate { value, direct: _ } in candidates {
        let mut outcome = slot.clone();
        let decoded = value.marshal().and_then(|()| {
            MetadataOverlay::default().assign(&mut outcome, value, field, &mut Vec::new(), false)
        });
        match decoded {
            Ok(()) => {
                joined = Some(match joined {
                    Some(joined) => join(joined, outcome),
                    None => outcome,
                });
            }
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    match (joined, failure) {
        (Some(joined), _) => {
            *slot = joined;
            Ok(())
        }
        (None, failure) => Err(failure.unwrap_or_else(|| json_error("no collision winner"))),
    }
}

/// The facts two successful outcomes share; a differing value becomes unknown.
fn join(left: Yaml, right: Yaml) -> Yaml {
    match (left, right) {
        (left, right) if left == right => left,
        (Yaml::Mapping(left), Yaml::Mapping(mut right))
            if left.len() == right.len() && left.keys().all(|key| right.contains_key(key)) =>
        {
            Yaml::Mapping(
                left.into_iter()
                    .map(|(key, value)| {
                        let other = right.remove(&key).unwrap_or_else(unknown);
                        (key, join(value, other))
                    })
                    .collect(),
            )
        }
        (Yaml::Sequence(left), Yaml::Sequence(right)) if left.len() == right.len() => {
            Yaml::Sequence(
                left.into_iter()
                    .zip(right)
                    .map(|(left, right)| join(left, right))
                    .collect(),
            )
        }
        _ => unknown(),
    }
}

/// A `float64` from Go's generic JSON decoding, kept integral when it is.
fn number(value: f64) -> Yaml {
    if value.fract() == 0.0 {
        let decimal = format!("{value:.0}");
        if let Ok(integer) = decimal.parse::<i64>() {
            return Yaml::from(integer);
        }
        if let Ok(integer) = decimal.parse::<u64>() {
            return Yaml::from(integer);
        }
    }
    Yaml::from(value)
}

/// Helm's `MergeMaps` transfer, with the later document taking precedence.
pub fn merge_values(destination: &mut Yaml, incoming: Yaml) {
    match (destination, incoming) {
        (Yaml::Mapping(target), Yaml::Mapping(incoming)) => {
            for (key, value) in incoming {
                merge_values(target.entry(key).or_insert(Yaml::Null), value);
            }
        }
        (Yaml::Tagged(_), Yaml::Mapping(_)) => {}
        (target, value) => *target = value,
    }
}
