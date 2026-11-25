use helm_schema_ast::contains_template_action;
use helm_schema_ir::{
    ContractIr, GraphContracts, Guard, SymbolicChart, SymbolicIrContext, SymbolicTemplate,
};
use helm_schema_k8s::LocalResourceSchema;

use super::local_crd_projection::local_resource_schemas_from_template_source;
use crate::chart;
use crate::error::EngineResult;

/// Executes every chart instance's templates as one Helm render (ADR 0072 §8).
///
/// Values reads carry their root-relative input identity, so no contract path is rescoped by
/// the file that consumed it. Dependency globals are still projected through Helm's
/// parent-first coalesce per owning chart until the heap carries the coalesced global maps.
#[tracing::instrument(skip_all, fields(charts = charts.len()))]
pub(crate) fn collect_manifest_contract_for_charts(
    charts: &[chart::ChartContext],
    symbolic_context: &SymbolicIrContext,
) -> EngineResult<ManifestContractAnalysis> {
    let mut contract = ContractIr::default();
    let mut local_resource_schemas = Vec::new();
    let mut graph = Vec::new();
    let mut loaded = Vec::new();
    for chart in charts {
        let mut static_root_strings = chart.static_root_strings.clone();
        // An aliased dependency renders under its alias (`chart/v2/util/dependencies.go:122`).
        if let Some(name) = chart.values_prefix.last() {
            static_root_strings.insert(vec!["Chart".to_owned(), "Name".to_owned()], name.clone());
        }
        graph.push(SymbolicChart {
            namespace: chart.template_namespace.clone(),
            values_path: values_root_path(&chart.values_prefix),
            enabled: activation_predicate(helm_enabled_guard_sets(
                &chart.dependency_activation_chain,
            )),
            static_root_strings,
        });
        // Helm parses only a library chart's partials (`engine.go:592`).
        if chart.is_library {
            continue;
        }
        // Helm executes every template that is not a partial; NOTES.txt is not a manifest.
        let manifests = chart
            .files
            .files_with_role(chart::FileRole::ManifestTemplate)
            .map(|file| (file, false));
        let notes = chart
            .files
            .files_with_role(chart::FileRole::NotesTemplate)
            .map(|file| (file, true));
        for (file, textual) in manifests.chain(notes) {
            loaded.push((chart, file, file.template_name(), textual));
        }
    }
    let files = loaded
        .iter()
        .map(|(chart, file, name, textual)| {
            Ok(ManifestSource {
                path: &file.path,
                name: chart::template_execution_name(chart, name),
                source: file.source()?,
                textual: *textual,
                values_prefix: &chart.values_prefix,
            })
        })
        .collect::<EngineResult<Vec<_>>>()?;
    let sources: Vec<_> = files
        .iter()
        .map(|file| SymbolicTemplate {
            source: file.source,
            source_path: &file.name,
        })
        .collect();
    let GraphContracts {
        files: contracts,
        startup,
    } = symbolic_context.generate_contract_irs_for_render_graph(&sources, &graph);
    contract.append(startup);
    for (file, mut manifest_contract) in files.iter().zip(contracts) {
        // NOTES and other non-YAML templates execute in the same schedule, but their text is
        // not a YAML manifest.
        if file.textual {
            manifest_contract.mark_rendered_output_textual();
        } else {
            local_resource_schemas.extend(local_resource_schemas_from_template_source(
                file.source,
                file.path,
                contains_template_action(file.source)?,
            ));
        }
        manifest_contract.project_dependency_global_contracts(file.values_prefix);
        contract.append(manifest_contract);
    }
    Ok(ManifestContractAnalysis {
        contract,
        local_resource_schemas,
    })
}

fn values_root_path(prefix: &[String]) -> helm_schema_core::ValuesPath {
    helm_schema_core::ValuesPath::from_segments(
        prefix.iter().cloned().map(helm_schema_core::Segment::from),
    )
}

struct ManifestSource<'a> {
    source: &'a str,
    path: &'a str,
    name: String,
    textual: bool,
    values_prefix: &'a [String],
}

pub(crate) struct ManifestContractAnalysis {
    pub(crate) contract: ContractIr,
    pub(crate) local_resource_schemas: Vec<LocalResourceSchema>,
}

/// The activation alternatives for a whole ancestor chain, with Helm's own dependency
/// enablement per level ([`helm_level_enabled_guard_sets`]): a nested chart renders only while
/// EVERY level activates, so the alternatives are the cross product of the per-level guard
/// sets. An empty result means the chart is unconditionally active.
pub(crate) fn helm_enabled_guard_sets(
    chain: &[chart::ChartDependencyActivation],
) -> Vec<Vec<Guard>> {
    let mut product: Vec<Vec<Guard>> = Vec::new();
    for level in chain {
        let level_sets = helm_level_enabled_guard_sets(level);
        if level_sets.is_empty() {
            continue;
        }
        if product.is_empty() {
            product = level_sets;
            continue;
        }
        let mut next = Vec::new();
        for prefix in &product {
            for set in &level_sets {
                let mut combined = prefix.clone();
                combined.extend(set.iter().cloned());
                next.push(combined);
            }
        }
        product = next;
    }
    product
}

/// The predicate under which one of `guard_sets` holds; true when there are
/// none.
pub(crate) fn activation_predicate(guard_sets: Vec<Vec<Guard>>) -> helm_schema_core::Predicate {
    let alternatives = guard_sets
        .into_iter()
        .map(|set| {
            helm_schema_core::Predicate::all(
                set.into_iter()
                    .map(helm_schema_core::Predicate::from)
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    if alternatives.is_empty() {
        helm_schema_core::Predicate::True
    } else {
        helm_schema_core::Predicate::Or(alternatives)
    }
}

/// Helm v4.2.3's enablement of one dependency (`processDependencyTags`, then
/// `processDependencyConditions`): the first condition holding a BOOLEAN
/// decides, a missing or non-boolean one is skipped; with none deciding, a
/// tag holding `true` enables, else a tag holding `false` disables, else the
/// dependency stays enabled.
fn helm_level_enabled_guard_sets(activation: &chart::ChartDependencyActivation) -> Vec<Vec<Guard>> {
    let condition_paths = normalized_ordered_paths(&activation.condition_paths);
    let tag_paths = normalized_sorted_paths(&activation.tag_paths);
    if condition_paths.is_empty() && tag_paths.is_empty() {
        return Vec::new();
    }
    let equals = |path: &helm_schema_core::ValuesPath, value| Guard::Eq {
        path: path.clone(),
        value: helm_schema_core::GuardValue::Bool(value),
    };
    let differs = |path: &helm_schema_core::ValuesPath, value| Guard::NotEq {
        path: path.clone(),
        value: helm_schema_core::GuardValue::Bool(value),
    };
    let mut guard_sets = Vec::new();
    let mut undecided = Vec::new();
    for path in &condition_paths {
        let mut enabled = undecided.clone();
        enabled.push(equals(path, true));
        guard_sets.push(enabled);
        undecided.push(differs(path, true));
        undecided.push(differs(path, false));
    }
    for path in &tag_paths {
        let mut enabled = undecided.clone();
        enabled.push(equals(path, true));
        guard_sets.push(enabled);
    }
    undecided.extend(tag_paths.iter().map(|path| differs(path, false)));
    guard_sets.push(undecided);
    guard_sets
}

fn trimmed_nonempty(paths: &[String]) -> impl Iterator<Item = helm_schema_core::ValuesPath> + '_ {
    paths.iter().filter_map(|path| {
        let path = path.trim();
        (!path.is_empty()).then(|| helm_schema_core::ValuesPath::parse(path))
    })
}

fn normalized_ordered_paths(paths: &[String]) -> Vec<helm_schema_core::ValuesPath> {
    let mut seen = std::collections::BTreeSet::new();
    let mut normalized = Vec::new();
    for path in trimmed_nonempty(paths) {
        if seen.insert(path.clone()) {
            normalized.push(path);
        }
    }
    normalized
}

fn normalized_sorted_paths(paths: &[String]) -> Vec<helm_schema_core::ValuesPath> {
    let mut normalized = trimmed_nonempty(paths).collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}
