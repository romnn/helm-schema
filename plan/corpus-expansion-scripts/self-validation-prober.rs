//! Standalone self-validation prober: does a chart's own values.yaml validate
//! against the schema helm-schema generated for it?
//!
//! Mirrors crates/helm-schema-cli/tests/common/values_validation.rs exactly:
//! parse values.yaml as JSON, delete null-valued map keys along MAP chains
//! only (arrays are atomic under Helm coalescing), then validate.

use serde_json::Value;

fn drop_nulls(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if v.is_null() {
                    continue;
                }
                out.insert(k.clone(), drop_nulls(v));
            }
            Value::Object(out)
        }
        _ => v.clone(),
    }
}

fn fail(status: &str, err: String) {
    println!("{}", serde_json::json!({"status": status, "error": err}));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: corpus-prober <values.yaml> <schema.json>");
        std::process::exit(2);
    }
    let values_text = match std::fs::read_to_string(&args[1]) {
        Ok(t) => t,
        Err(e) => return fail("values_read_error", e.to_string()),
    };
    let values: Value = match serde_yaml::from_str(&values_text) {
        Ok(v) => v,
        Err(e) => return fail("values_parse_error", e.to_string()),
    };
    let values = drop_nulls(&values);
    let schema_text = match std::fs::read_to_string(&args[2]) {
        Ok(t) => t,
        Err(e) => return fail("schema_read_error", e.to_string()),
    };
    let schema: Value = match serde_json::from_str(&schema_text) {
        Ok(v) => v,
        Err(e) => return fail("schema_parse_error", e.to_string()),
    };
    let validator = match jsonschema::validator_for(&schema) {
        Ok(v) => v,
        Err(e) => return fail("schema_compile_error", e.to_string()),
    };
    let errors: Vec<String> = validator
        .iter_errors(&values)
        .map(|e| format!("{}: {}", e.instance_path(), e))
        .take(40)
        .collect();
    let status = if errors.is_empty() { "accept" } else { "reject" };
    println!(
        "{}",
        serde_json::json!({"status": status, "error_count": errors.len(), "errors": errors})
    );
}
