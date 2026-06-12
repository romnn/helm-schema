use crate::{Guard, ResourceRef, ValueKind, YamlPath};

pub(crate) trait ValueUseSink {
    fn emit_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind);

    fn emit_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    );

    fn emit_document_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
        _resource: Option<ResourceRef>,
    ) {
        self.emit_use_with_extra_guards(source_expr, path, kind, extra_guards);
    }

    fn emit_helper_use_kind_with_extra_guards(
        &mut self,
        source_expr: String,
        kind: ValueKind,
        extra_guards: &[Guard],
    );
}
