use serde_json::{Map, Value};

use crate::{
    BlockScalar, MappingEntry, Node, ScalarPart, ScalarParts, SequenceItem, TemplatedDocument,
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
        if scalar
            .parts
            .iter()
            .any(|part| matches!(part, ScalarPart::Hole(_)))
        {
            return None;
        }
        self.literal_source(scalar.span.start, scalar.span.end)
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
                    let key = self.literal_scalar(&entry.key)?;
                    mapping.insert(
                        key.as_str()?.to_string(),
                        self.literal_mapping_value(entry)?,
                    );
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

    fn literal_block(&self, block: &BlockScalar) -> Option<Value> {
        if !block.holes.is_empty() {
            return None;
        }
        self.literal_source(block.header.start, block.body.end.max(block.header.end))
    }

    fn literal_source(&self, start: usize, end: usize) -> Option<Value> {
        let source = self.source.get(start..end)?.trim();
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(source).ok()?;
        serde_json::to_value(yaml).ok()
    }
}
