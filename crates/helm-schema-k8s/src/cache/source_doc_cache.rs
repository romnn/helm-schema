use std::collections::HashMap;
use std::fs;
use std::hash::Hash;
use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;

use crate::schema_doc::SchemaDoc;

#[derive(Debug, Default)]
pub(crate) struct SourceDocCache<K> {
    mem: Mutex<HashMap<K, SchemaDoc>>,
}

impl<K> SourceDocCache<K>
where
    K: Eq + Hash,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            mem: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn read(&self, key: &K) -> Option<SchemaDoc> {
        self.mem
            .lock()
            .ok()
            .and_then(|guard| guard.get(key).cloned())
    }

    pub(crate) fn write(&self, key: K, doc: SchemaDoc) {
        if let Ok(mut guard) = self.mem.lock() {
            guard.insert(key, doc);
        }
    }
}

pub(crate) fn read_cached_json_doc(path: &Path) -> Option<SchemaDoc> {
    let bytes = fs::read(path).ok()?;
    let doc = serde_json::from_slice::<Value>(&bytes).ok()?;
    Some(SchemaDoc::new(doc))
}
