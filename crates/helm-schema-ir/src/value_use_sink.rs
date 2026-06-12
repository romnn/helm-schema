use crate::{Guard, ValueKind, YamlPath};

pub(crate) trait ValueUseSink {
    fn emit_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind);

    fn emit_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    );
}
