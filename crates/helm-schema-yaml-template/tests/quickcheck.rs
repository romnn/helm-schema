use helm_schema_yaml_template::{Yaml, YamlEmitter, YamlLoader};
use quickcheck::TestResult;

quickcheck::quickcheck! {
    fn test_check_weird_keys(xs: Vec<String>) -> TestResult {
        if xs.iter().any(|s| s.contains('\0')) {
            return TestResult::discard();
        }
        let mut out_str = String::new();
        let input = Yaml::Array(xs.into_iter().map(Yaml::String).collect());
        {
            let mut emitter = YamlEmitter::new(&mut out_str);
            emitter.dump(&input).unwrap();
        }
        match YamlLoader::load_from_str(&out_str) {
            Ok(output) => TestResult::from_bool(output.len() == 1 && input == output[0]),
            Err(err) => TestResult::error(err.to_string()),
        }
    }
}
