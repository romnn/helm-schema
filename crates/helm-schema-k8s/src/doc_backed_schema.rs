use serde_json::Value;

use helm_schema_core::YamlPath;

use crate::kubernetes_openapi::resolve_ctx::{
    ResolveCtx, ResolvedSchemaLeaf, descend_schema_path_expanding_leaf_with_location,
};
use crate::lookup::source_bundle::{SourceBundleNode, bundle_source_definition};
use crate::lookup::{ProviderLookupResult, ProviderSchemaFragment, ProviderSchemaSource};
use crate::metadata_enrichment::{enrich_root_metadata_schema, enriched_metadata_schema};
use crate::schema_doc::SchemaDoc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalSchemaLeaf {
    schema: Value,
    source_schema: Option<Value>,
    pointer: Option<String>,
}

impl LocalSchemaLeaf {
    fn from_resolved_leaf(leaf: &ResolvedSchemaLeaf, keep_source: bool) -> Self {
        Self {
            schema: leaf.schema().clone(),
            source_schema: keep_source.then(|| leaf.source_schema().clone()),
            pointer: keep_source.then(|| leaf.location().pointer().to_string()),
        }
    }

    #[must_use]
    pub(crate) fn source_schema(&self) -> Option<&Value> {
        self.source_schema.as_ref()
    }

    #[must_use]
    pub(crate) fn pointer(&self) -> Option<&str> {
        self.pointer.as_deref()
    }

    #[must_use]
    pub(crate) fn into_schema(self) -> Value {
        self.schema
    }
}

pub(crate) fn fragment_for_source_leaf(
    root: &SchemaDoc,
    source: Option<ProviderSchemaSource>,
    leaf: LocalSchemaLeaf,
) -> ProviderSchemaFragment {
    let source_schema = leaf.source_schema().cloned();
    let mut fragment = ProviderSchemaFragment::new(leaf.into_schema());
    match (source, source_schema) {
        (Some(source), Some(source_schema)) => {
            let definition_schema = bundle_source_definition(
                source.filename(),
                source.pointer(),
                &source_schema,
                |current_location, reference| {
                    let pointer = reference.strip_prefix('#')?;
                    root.root().pointer(pointer).cloned().map(|schema| {
                        SourceBundleNode::new(&current_location.document, pointer, schema)
                    })
                },
            );
            fragment =
                fragment.with_source_definition_schema(source, source_schema, definition_schema);
        }
        (Some(source), None) => {
            fragment = fragment.with_source(source);
        }
        (None, _) => {}
    }
    fragment
}

pub(crate) fn lookup_root_metadata_path(
    root: &SchemaDoc,
    path: &YamlPath,
    source_for_leaf: impl FnOnce(&LocalSchemaLeaf) -> Option<ProviderSchemaSource>,
) -> ProviderLookupResult {
    fragment_for_root_metadata_path(root, path, source_for_leaf).map_or(
        ProviderLookupResult::PathUnresolved,
        |schema| ProviderLookupResult::Found {
            schema,
            resolved_k8s_version: None,
        },
    )
}

fn fragment_for_root_metadata_path(
    root: &SchemaDoc,
    path: &YamlPath,
    source_for_leaf: impl FnOnce(&LocalSchemaLeaf) -> Option<ProviderSchemaSource>,
) -> Option<ProviderSchemaFragment> {
    let leaf = descend_schema_path_expanding_leaf_with_root_metadata_source(root.root(), &path.0)?;
    Some(fragment_for_source_leaf(root, source_for_leaf(&leaf), leaf))
}

#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub(crate) fn descend_schema_path_expanding_leaf_with_source(
    root: &Value,
    path: &[String],
) -> Option<LocalSchemaLeaf> {
    descend_schema_path_with_resolver(root, root, path, true)
}

#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub(crate) fn descend_schema_path_expanding_leaf_with_root_metadata_source(
    root: &Value,
    path: &[String],
) -> Option<LocalSchemaLeaf> {
    let Some(first_segment) = path.first() else {
        let enriched_root = enrich_root_metadata_schema(root.clone());
        return descend_schema_path_with_resolver(&enriched_root, &enriched_root, &[], false);
    };

    if first_segment != "metadata" {
        return descend_schema_path_expanding_leaf_with_source(root, path);
    }

    let metadata = enriched_metadata_schema(root);
    descend_schema_path_with_resolver(root, &metadata, &path[1..], false)
}

fn descend_schema_path_with_resolver(
    root: &Value,
    schema: &Value,
    path: &[String],
    keep_source: bool,
) -> Option<LocalSchemaLeaf> {
    const LOCAL_DOC: &str = "local";
    let mut ctx = ResolveCtx::new(
        |_| None,
        LOCAL_DOC.to_string(),
        SchemaDoc::new(root.clone()),
    );
    let leaf = descend_schema_path_expanding_leaf_with_location(&mut ctx, LOCAL_DOC, schema, path)?;
    Some(LocalSchemaLeaf::from_resolved_leaf(&leaf, keep_source))
}
