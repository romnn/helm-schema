//! What does and does not invalidate a produced corpus.

use std::collections::BTreeSet;
use std::path::Path;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_test_support::manifest::{BuildProvenance, InputDigester};
use helm_schema_test_support::registry::{
    GenerationRecipe, IrRecipe, TemplateProvider, TemplateRecipe,
};
use helm_schema_test_support::source_digest::{GENERATION_CRATES, generation_source_digest};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

fn write(root: &Path, relative: &str, contents: &str) -> eyre::Result<()> {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().ok_or_eyre("no parent")?)?;
    std::fs::write(&path, contents).wrap_err_with(|| format!("write {}", path.display()))
}

/// A miniature workspace with every generation crate and the files around it.
fn workspace() -> eyre::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    for file in ["Cargo.toml", "Cargo.lock", ".cargo/config.toml"] {
        write(root.path(), file, "base")?;
    }
    for name in GENERATION_CRATES {
        for file in [
            "Cargo.toml",
            "src/lib.rs",
            "src/tests/mod.rs",
            "tests/it.rs",
            "examples/e.rs",
        ] {
            write(root.path(), &format!("crates/{name}/{file}"), "base")?;
        }
    }
    let grammar = "crates/helm-schema-template-grammar/grammars/tree-sitter-go-template";
    write(root.path(), &format!("{grammar}/src/parser.c"), "base")?;
    write(
        root.path(),
        &format!("{grammar}/dialects/helm/src/parser.c"),
        "base",
    )?;
    write(root.path(), &format!("{grammar}/package.json"), "base")?;
    Ok(root)
}

/// Whether editing `relative` changes the generation source digest.
fn invalidates(relative: &str) -> eyre::Result<bool> {
    let root = workspace()?;
    let before = generation_source_digest(root.path())?;
    write(root.path(), relative, "edited")?;
    Ok(generation_source_digest(root.path())? != before)
}

#[test]
fn source_edits_invalidate_and_test_only_edits_do_not() -> eyre::Result<()> {
    let grammar = "crates/helm-schema-template-grammar/grammars/tree-sitter-go-template";
    let cases = [
        ("crates/helm-schema-ir/src/lib.rs", true),
        ("crates/helm-schema-gen/src/defaults.yaml", true),
        ("crates/helm-schema-template-grammar/build.rs", true),
        ("crates/helm-schema/Cargo.toml", true),
        ("Cargo.lock", true),
        (".cargo/config.toml", true),
        (&format!("{grammar}/src/parser.c"), true),
        ("crates/helm-schema-ir/src/tests/mod.rs", false),
        ("crates/helm-schema-ir/tests/it.rs", false),
        ("crates/helm-schema/examples/e.rs", false),
        (&format!("{grammar}/dialects/helm/src/parser.c"), false),
        (&format!("{grammar}/package.json"), false),
        ("crates/helm-schema-cli/src/main.rs", false),
    ];
    let mut verdicts = Vec::new();
    for (path, _) in cases {
        verdicts.push((path.to_string(), invalidates(path)?));
    }
    sim_assert_eq!(
        have: verdicts,
        want: cases
            .iter()
            .map(|(path, expected)| (path.to_string(), *expected))
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn a_new_build_script_invalidates() -> eyre::Result<()> {
    let root = workspace()?;
    write(root.path(), "crates/helm-schema-ir/build.rs", "base")?;
    let before = generation_source_digest(root.path())?;
    write(root.path(), "crates/helm-schema-ir/build.rs", "edited")?;
    eyre::ensure!(generation_source_digest(root.path())? != before);
    Ok(())
}

#[test]
fn this_build_was_compiled_from_the_current_tree() -> eyre::Result<()> {
    BuildProvenance::compiled().check_current()?;
    Ok(())
}

/// The digest covers exactly the workspace crates the test-support crate links.
#[test]
fn generation_crates_are_the_linked_workspace_crates() -> eyre::Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = std::process::Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--offline"])
        .current_dir(test_util::workspace_root())
        .output()
        .wrap_err("run cargo metadata")?;
    eyre::ensure!(output.status.success(), "cargo metadata failed");
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let packages = metadata["packages"].as_array().ok_or_eyre("no packages")?;
    let mut linked = BTreeSet::new();
    let mut pending = vec!["helm-schema-test-support".to_string()];
    while let Some(name) = pending.pop() {
        if !linked.insert(name.clone()) {
            continue;
        }
        let package = packages
            .iter()
            .find(|package| package["name"] == name.as_str())
            .ok_or_eyre("unknown package")?;
        for dependency in package["dependencies"].as_array().ok_or_eyre("no deps")? {
            let normal_or_build = dependency["kind"].is_null() || dependency["kind"] == "build";
            if normal_or_build && dependency["path"].is_string() {
                pending.push(dependency["name"].as_str().ok_or_eyre("name")?.to_string());
            }
        }
    }
    sim_assert_eq!(
        have: linked,
        want: GENERATION_CRATES.iter().map(|name| (*name).to_string()).collect::<BTreeSet<_>>()
    );
    Ok(())
}

const TEMPLATE: &str = "charts/x/templates/cm.yaml";
const HELPERS: test_util::DefineSourceSpec<'static> = test_util::DefineSourceSpec {
    helper_templates: &["charts/x/templates/_helpers.tpl"],
    helper_template_dirs: &[("charts/common/templates", "tpl")],
    file_sources: &[],
};

fn testdata() -> eyre::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    for file in [
        TEMPLATE,
        "charts/x/templates/_helpers.tpl",
        "charts/x/values.yaml",
        "charts/common/templates/_labels.tpl",
        "charts/common/templates/nested/_deep.tpl",
        "charts/common/templates/NOTES.txt",
        "provider-bundle/kubernetes-json-schema-cache/v.json",
    ] {
        write(root.path(), file, "base")?;
    }
    Ok(root)
}

/// Whether editing `relative` below a fresh testdata tree changes `recipe`'s input digest.
fn input_invalidates(recipe: &GenerationRecipe, relative: &str) -> eyre::Result<bool> {
    let root = testdata()?;
    let before = InputDigester::new(root.path().to_path_buf()).digest(recipe)?;
    write(root.path(), relative, "edited")?;
    Ok(InputDigester::new(root.path().to_path_buf()).digest(recipe)? != before)
}

#[test]
fn recipe_inputs_are_exactly_what_generation_reads() -> eyre::Result<()> {
    let ir = GenerationRecipe::Ir(IrRecipe {
        template_path: TEMPLATE,
        define_sources: HELPERS,
    });
    let template = |inline_values| {
        GenerationRecipe::Template(TemplateRecipe {
            template_path: TEMPLATE,
            values_path: "charts/x/values.yaml",
            inline_values,
            define_sources: HELPERS,
            provider: TemplateProvider::K8s("v"),
        })
    };
    let bundle = "provider-bundle/kubernetes-json-schema-cache/v.json";
    let verdicts = [
        input_invalidates(&ir, TEMPLATE)?,
        input_invalidates(&ir, "charts/x/templates/_helpers.tpl")?,
        input_invalidates(&ir, "charts/common/templates/_labels.tpl")?,
        input_invalidates(&ir, "charts/common/templates/nested/_deep.tpl")?,
        input_invalidates(&ir, "charts/common/templates/NOTES.txt")?,
        input_invalidates(&ir, bundle)?,
        input_invalidates(&template(None), "charts/x/values.yaml")?,
        input_invalidates(&template(Some("inline: true")), "charts/x/values.yaml")?,
        input_invalidates(&template(None), bundle)?,
    ];
    sim_assert_eq!(
        have: verdicts,
        want: [true, true, true, false, false, false, true, false, true]
    );
    Ok(())
}
