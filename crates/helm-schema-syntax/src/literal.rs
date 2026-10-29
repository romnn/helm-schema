use serde_json::{Map, Value};

use crate::{
    BlockScalar, HelmYamlDocument, HelmYamlKey, MappingEntry, Node, ScalarPart, ScalarParts,
    SequenceItem, TemplatedDocument, helm_yaml_documents, helm_yaml_key,
};

impl TemplatedDocument<'_> {
    /// Projects the literal portion of a mapping entry's value.
    #[must_use]
    pub fn literal_mapping_value(&self, entry: &MappingEntry) -> Option<Value> {
        if let Some(value) = &entry.value {
            return self.literal_scalar(value);
        }
        if let Some(block) = &entry.block {
            return self.literal_block(block);
        }
        if entry.children.is_empty() {
            return Some(Value::Null);
        }
        self.literal_nodes(&entry.children)
    }

    /// Projects the literal portion of a sequence item's value.
    #[must_use]
    pub fn literal_sequence_value(&self, item: &SequenceItem) -> Option<Value> {
        if let Some(value) = &item.value {
            return self.literal_scalar(value);
        }
        if let Some(block) = &item.block {
            return self.literal_block(block);
        }
        if item.children.is_empty() {
            return Some(Value::Null);
        }
        self.literal_nodes(&item.children)
    }

    /// Projects scalar parts when they contain no template-action hole.
    #[must_use]
    pub fn literal_scalar(&self, scalar: &ScalarParts) -> Option<Value> {
        if has_hole(scalar) {
            return None;
        }
        self.literal_source(scalar.span.start, scalar.span.end)
    }

    /// How Helm reads a mapping key without a template-action hole.
    #[must_use]
    pub fn literal_key(&self, key: &ScalarParts) -> Option<HelmYamlKey> {
        if has_hole(key) {
            return None;
        }
        Some(helm_yaml_key(
            self.source.get(key.span.start..key.span.end)?,
        ))
    }

    fn literal_nodes(&self, nodes: &[Node]) -> Option<Value> {
        let mut mapping = Map::new();
        let mut sequence = Vec::new();
        let mut shape = None;

        for node in nodes {
            match node {
                Node::Mapping(entry) => {
                    if shape == Some(false) {
                        return None;
                    }
                    shape = Some(true);
                    // A merge key, or a duplicate whose reading may depend on
                    // its key's type, leaves the mapping unknown.
                    let Some(HelmYamlKey::Key(key)) = self.literal_key(&entry.key) else {
                        return None;
                    };
                    if mapping.contains_key(&key) {
                        return None;
                    }
                    mapping.insert(key, self.literal_mapping_value(entry)?);
                }
                Node::Sequence(item) => {
                    if shape == Some(true) {
                        return None;
                    }
                    shape = Some(false);
                    sequence.push(self.literal_sequence_value(item)?);
                }
                Node::Comment(_) | Node::Control(_) | Node::Output(_) | Node::Opaque(_) => {}
                Node::Scalar(_) => return None,
            }
        }

        match shape {
            Some(true) => Some(Value::Object(mapping)),
            Some(false) => Some(Value::Array(sequence)),
            None => Some(Value::Null),
        }
    }

    /// Read alone, a block scalar loses its trailing blank lines and its
    /// parent's indentation, so keep chomping and an explicit indentation
    /// indicator stay unknown.
    fn literal_block(&self, block: &BlockScalar) -> Option<Value> {
        let header = self.source.get(block.header.start..block.header.end)?;
        if !block.holes.is_empty() || header.contains(|c: char| c == '+' || c.is_ascii_digit()) {
            return None;
        }
        self.literal_source(block.header.start, block.body.end.max(block.header.end))
    }

    /// The literal text as Helm decodes it, when that is fully known.
    fn literal_source(&self, start: usize, end: usize) -> Option<Value> {
        let source = self.source.get(start..end)?.trim();
        match helm_yaml_documents(source).as_slice() {
            [] => Some(Value::Null),
            [HelmYamlDocument::Decoded { value, unknown }] if unknown.is_empty() => {
                Some(value.clone())
            }
            _ => None,
        }
    }
}

fn has_hole(scalar: &ScalarParts) -> bool {
    scalar
        .parts
        .iter()
        .any(|part| matches!(part, ScalarPart::Hole(_)))
}
