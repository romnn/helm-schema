#![allow(warnings)]

pub mod logging;

use color_eyre::eyre;
use helm_schema_go_template_value::Value;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct HelmValue(serde_yaml::Value);

impl From<serde_yaml::Value> for HelmValue {
    fn from(value: serde_yaml::Value) -> Self {
        Self(value)
    }
}

impl From<HelmValue> for helm_schema_go_template_value::Value {
    fn from(value: HelmValue) -> Self {
        match value.0 {
            serde_yaml::Value::Null => Value::NoValue,
            serde_yaml::Value::String(v) => Value::String(v),
            serde_yaml::Value::Number(v) => {
                if let Some(i64) = v.as_i64() {
                    Value::Number(i64.into())
                    // helm_schema_go_template_value::Number::from(i64)
                } else if let Some(u64) = v.as_u64() {
                    Value::Number(u64.into())
                    // helm_schema_go_template_value::Number::from(u64)
                } else {
                    panic!()
                }
            }
            serde_yaml::Value::Sequence(v) => {
                Value::Array(v.into_iter().map(|v| HelmValue(v).into()).collect())
            }
            serde_yaml::Value::Mapping(v) => Value::Map(
                v.into_iter()
                    .map(|(k, v)| {
                        (
                            k.as_str().unwrap_or_default().to_string(),
                            HelmValue(v).into(),
                        )
                    })
                    .collect(),
            ),
            serde_yaml::Value::Bool(v) => Value::Bool(v),
            serde_yaml::Value::Tagged(v) => {
                unimplemented!("tagged YAML values")
            }
        }
    }
}

// impl helm_schema_go_template_value::Value for HelmValues {}

fn process_template(contents: &str, values: HelmValue) -> eyre::Result<()> {
    use helm_schema_go_template::{Func, FuncError, Value, gtmpl_fn, template};
    use helm_schema_go_template_value::Function;

    // helm_schema_go_template::gtmpl_fn!(
    //     fn add(a: u64, b: u64) -> Result<u64, FuncError> {
    //         Ok(a + b)
    //     }
    // );

    // fn plus_one(args: &[Value]) -> Result<Value, FuncError> {
    //     if let Value::Object(ref o) = &args[0] {
    //         if let Some(Value::Number(ref n)) = o.get("num") {
    //             if let Some(i) = n.as_i64() {
    //                 return Ok((i + 1).into());
    //             }
    //         }
    //     }
    //     Err(FuncError::Generic("failed to quote"))
    //     // Err(anyhow!("integer required, got: {:?}", args).into())
    // }

    // fn quote(args: &[Value]) -> Result<Value, FuncError> {
    //     if let Value::String(o) = &args[0] {
    //         return Ok(Value::String(format!("{o}")));
    //     }
    //     // if let Value::Object(ref o) = &args[0] {
    //     //     if let Some(Value::Number(ref n)) = o.get("num") {
    //     //         if let Some(i) = n.as_i64() {
    //     //             return Ok((i + 1).into());
    //     //         }
    //     //     }
    //     // }
    //     Err(FuncError::Generic("failed to quote".to_string()))
    //     // Err(anyhow!("integer required, got: {:?}", args).into())
    // }

    // #[derive(helm_schema_go_template_derive::Gtmpl)]
    // struct HelmContext {
    //     pub bar: String,
    //     // pub quote: Func,
    // }
    //
    // let ctx = HelmContext {
    //     bar: "test".to_string(),
    //     // quote,
    // };

    use helm_schema_go_template::{Context, Template};
    use helm_schema_go_template::{lexer::Lexer, parse::Parser};
    use std::collections::{HashMap, VecDeque};

    if false {
        dbg!(&contents);
        let lex = helm_schema_go_template::lexer::Lexer::new(contents.to_string());
        // let mut parser = Parser::new("parser_name".to_string());
        let funcs = ["quote"].into_iter().map(ToString::to_string).collect();
        let mut parser = Parser {
            name: String::from("foo"),
            funcs,
            lex: Some(lex),
            line: 0,
            token: VecDeque::new(),
            peek_count: 0,
            tree_set: HashMap::new(),
            tree_id: 0,
            tree: None,
            tree_stack: VecDeque::new(),
            max_tree_id: 0,
        };

        parser.parse_tree();
        let tree = parser.tree;
        dbg!(tree);
    }

    let mut tmpl = Template::default();
    // tmpl.add_func("quote", quote);
    tmpl.add_func("quote", |values: &[Value]| -> Result<Value, FuncError> {
        Ok(Value::String(values[0].to_string()))
    });
    tmpl.add_func("include", |values: &[Value]| -> Result<Value, FuncError> {
        // Ok(values[0].to_owned())
        Ok(Value::String("<TODO(include)>".to_string()))
    });
    tmpl.add_func("nindent", |values: &[Value]| -> Result<Value, FuncError> {
        Ok(values[0].to_owned())
        // Ok(Value::String("".to_string()))
    });
    tmpl.parse(contents)?;

    // dbg!(&tmpl.tree_set);
    let templated = tmpl.render(&Context::from(values))?;

    // let templated = helm_schema_go_template::template(&contents, ctx)?;
    // println!("=== templated ===");
    // println!("{templated}");
    Ok(())
}

fn process_chart(archive_path: &Path) -> eyre::Result<()> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    use tar::Archive;

    let file = std::fs::File::open(&archive_path)?;
    let reader = std::io::BufReader::new(file);
    let gz = GzDecoder::new(reader);
    let mut archive = Archive::new(gz);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let header = entry.header();

        if header.entry_type().is_file() {
            let path = entry.path()?.into_owned();
            dbg!(&path);

            let mut contents = Vec::with_capacity(header.size()? as usize);
            entry.read_to_end(&mut contents)?;
            let contents = String::from_utf8_lossy(&contents);

            // use gtmpl_derive::Gtmpl;
            println!("\n\n\n");
            println!("=== {path:?} ===");
            println!("{contents}");
            let values = serde_yaml::Value::default();
            process_template(&contents, values.into())?;
            // files.push((path, contents));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::process_template;

    use super::process_chart;
    use color_eyre::eyre;
    use indoc::indoc;
    use similar_asserts::assert_eq as sim_assert_eq;
    use std::path::PathBuf;

    #[test]
    fn iter_template() -> eyre::Result<()> {
        crate::logging::setup_logging(None, None, termcolor::ColorChoice::Auto);
        let template = indoc! {r#"
            {{- if (.Values.networkPolicies).enabled }}
            ---
            # default: deny all
            apiVersion: networking.k8s.io/v1
            kind: NetworkPolicy
            metadata:
              name: nats-default-deny
              {{- with .Values.networkPolicies.annotations }}
              annotations:
                {{- range $key, $value := . }}
                {{ $key }}: {{ $value | quote }}
                {{- end }}
              {{- end }}
              labels:
                {{- include "common.labels" . | nindent 4 }}
            spec:
              podSelector: {}
              policyTypes:
                - Egress
                - Ingress
            ---
            # allow all ingress and egress between pods in the nats namespace
            kind: NetworkPolicy
            apiVersion: networking.k8s.io/v1
            metadata:
              name: nats-allow-same-namespace
              {{- with .Values.networkPolicies.annotations }}
              annotations:
                {{- range $key, $value := . }}
                {{ $key }}: {{ $value | quote }}
                {{- end }}
              {{- end }}
              labels:
                {{- include "common.labels" . | nindent 4 }}
            spec:
              podSelector: {}
              ingress:
                - {{ include "common.netpol.ingress.same-namespace" . | nindent 6 }}
              egress:
                - {{ include "common.netpol.egress.same-namespace" . | nindent 6 }}
            ---
            # allow dns for all services
            kind: NetworkPolicy
            apiVersion: networking.k8s.io/v1
            metadata:
              name: nats-allow-kube-dns-outbound
              {{- with .Values.networkPolicies.annotations }}
              annotations:
                {{- range $key, $value := . }}
                {{ $key }}: {{ $value | quote }}
                {{- end }}
              {{- end }}
              labels:
                {{- include "common.labels" . | nindent 4 }}
            spec:
              podSelector: {}
              egress:
                - {{- include "common.netpol.egress.dns" . | nindent 6 }}
            {{- end }}"#
        };

        let values =
            std::fs::read_to_string("/Users/roman/dev/luup/deployment/charts/nats/values.yaml")?;
        let values: serde_yaml::Value = serde_yaml::from_str(&values)?;
        dbg!(&values);

        process_template(&template, values.into())?;
        Ok(())
    }

    #[test]
    fn parses_all_charts() -> eyre::Result<()> {
        let charts: Vec<PathBuf> =
            glob::glob("/Users/roman/dev/luup/deployment/charts/*/charts/*.tgz")?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
        dbg!(
            &charts
                .iter()
                .map(|p| p.file_name().unwrap())
                .collect::<Vec<_>>()
        );

        for chart in charts {
            dbg!(chart.file_name().unwrap());
            process_chart(&chart)?;
        }
        // dbg!(&output);
        // sim_assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
        Ok(())
    }

    #[test]
    fn parse_golang_template() {
        let output = helm_schema_go_template::template("Finally! Some {{ . }} for Rust", "gtmpl");
        dbg!(&output);
        sim_assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
    }
}
