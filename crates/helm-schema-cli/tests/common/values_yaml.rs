use color_eyre::eyre::{self, WrapErr as _};

pub(crate) fn read_values_yaml_for_path(chart_relative_path: &str) -> eyre::Result<String> {
    let path = crate::schema_roundtrip::physical_chart_dir(chart_relative_path).join("values.yaml");
    std::fs::read_to_string(&path)
        .wrap_err_with(|| format!("read chart values file {}", path.display()))
}
