use helm_schema_ast::{DefineIndex, HelmParser};

pub fn build_define_index(
    parser: &dyn HelmParser,
    spec: test_util::DefineSourceSpec<'_>,
) -> DefineIndex {
    let loaded = spec.load();
    let mut idx = DefineIndex::new();
    for source in loaded.helper_templates {
        idx.add_source(parser, &source)
            .expect("helper source should parse");
    }
    for (name, source) in loaded.file_sources {
        idx.add_file_source(&name, &source);
    }
    idx
}
