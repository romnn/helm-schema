use helm_schema_ast::contains_template_action;
use helm_schema_ir::{ContractIr, Guard, SymbolicIrContext};
use helm_schema_k8s::LocalResourceSchema;
use vfs::VfsPath;

use super::local_crd_projection::local_resource_schemas_from_template_source;
use crate::chart;
use crate::error::EngineResult;

#[tracing::instrument(skip_all, fields(prefix_len = chart.values_prefix.len()))]
pub(crate) fn collect_manifest_contract_for_chart(
    chart: &chart::ChartContext,
    symbolic_context: &SymbolicIrContext,
    include_tests: bool,
    optional_helpers: &[OptionalDependencyHelpers],
    defines: &helm_schema_ast::DefineIndex,
) -> EngineResult<ManifestContractAnalysis> {
    let mut contract = ContractIr::default();
    let mut local_resource_schemas = Vec::new();
    let activation_guard_sets = chart_activation_guard_sets(&chart.dependency_activation_chain);

    let manifests = chart::files_with_role(
        &chart.chart_dir,
        include_tests,
        chart::FileRole::ManifestTemplate,
    )?;
    for path in manifests {
        let source = path.read_to_string()?;
        let (mut manifest_contract, template_local_resource_schemas) =
            collect_manifest_contract_for_template(&path, symbolic_context)?;
        manifest_contract
            .map_value_paths(|path| chart::scope_values_path(path, &chart.values_prefix));
        // Rendering this template unconditionally reaches these helper
        // names; a name only an inactive optional dependency defines aborts
        // with "no template". The activation predicates are already
        // root-relative, so the clause is added AFTER this chart's own path
        // scoping and BEFORE its activation guards (an optional chart's
        // clause must itself only fire while the chart is active).
        if !optional_helpers.is_empty() {
            let reached = unconditional_include_closure(&source, defines);
            for entry in optional_helpers {
                if reached.iter().any(|name| entry.helper_names.contains(name)) {
                    manifest_contract.add_terminal_fail_condition(entry.inactive.clone());
                }
            }
        }
        manifest_contract =
            apply_chart_activation_guard_sets(manifest_contract, &activation_guard_sets);
        contract.append(manifest_contract);
        local_resource_schemas.extend(template_local_resource_schemas);
    }

    // Helm executes `templates/NOTES.txt` too: its consumers and terminal
    // effects (a `tpl` on a values string, a migration `fail`) are schema
    // evidence like any manifest's. Its prose is NOT a manifest — resource
    // schema extraction would try to YAML-parse free text (ASCII art,
    // indented URLs) and fail, so only the contract lane runs here.
    let notes = chart::files_with_role(
        &chart.chart_dir,
        include_tests,
        chart::FileRole::NotesTemplate,
    )?;
    for path in notes {
        let source = path.read_to_string()?;
        let mut notes_contract =
            symbolic_context.generate_contract_ir_for_source(&source, path.as_str());
        // NOTES is a Go-template text program, not YAML. Direct holes use
        // Go's textual formatting and therefore impose no input shape; real
        // strict calls and terminal effects remain in their own channels.
        notes_contract.mark_rendered_output_textual();
        notes_contract.map_value_paths(|path| chart::scope_values_path(path, &chart.values_prefix));
        notes_contract = apply_chart_activation_guard_sets(notes_contract, &activation_guard_sets);
        contract.append(notes_contract);
    }

    Ok(ManifestContractAnalysis {
        contract,
        local_resource_schemas,
    })
}

pub(crate) struct ManifestContractAnalysis {
    pub(crate) contract: ContractIr,
    pub(crate) local_resource_schemas: Vec<LocalResourceSchema>,
}

#[tracing::instrument(skip_all)]
fn collect_manifest_contract_for_template(
    path: &VfsPath,
    symbolic_context: &SymbolicIrContext,
) -> EngineResult<(ContractIr, Vec<LocalResourceSchema>)> {
    let source = path.read_to_string()?;
    let contract = symbolic_context.generate_contract_ir_for_source(&source, path.as_str());
    let local_resource_schemas = local_resource_schemas_from_template_source(
        &source,
        path.as_str(),
        contains_template_action(&source)?,
    )?;
    Ok((contract, local_resource_schemas))
}

fn apply_chart_activation_guard_sets(
    contract: ContractIr,
    activation_guard_sets: &[Vec<Guard>],
) -> ContractIr {
    if activation_guard_sets.is_empty() {
        return contract;
    }

    let mut activated_contract = ContractIr::default();
    for guards in activation_guard_sets {
        let mut guarded_contract = contract.clone();
        guarded_contract.append_guards_to_all_uses(guards);
        activated_contract.append(guarded_contract);
    }
    activated_contract
}

/// The activation alternatives for a whole ancestor chain: a nested chart
/// renders only while EVERY level activates, so the alternatives are the
/// cross product of the per-level guard sets. An empty result means the
/// chart is unconditionally active.
fn chart_activation_guard_sets(chain: &[chart::ChartDependencyActivation]) -> Vec<Vec<Guard>> {
    let mut product: Vec<Vec<Guard>> = Vec::new();
    for level in chain {
        let level_sets = level_activation_guard_sets(level);
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

fn level_activation_guard_sets(activation: &chart::ChartDependencyActivation) -> Vec<Vec<Guard>> {
    let condition_paths = normalized_ordered_paths(&activation.condition_paths);
    let tag_paths = normalized_sorted_paths(&activation.tag_paths);

    if condition_paths.is_empty() && tag_paths.is_empty() {
        return Vec::new();
    }

    let mut guard_sets = Vec::new();
    for (index, condition_path) in condition_paths.iter().enumerate() {
        let mut guards = absent_guards(&condition_paths[..index]);
        guards.push(Guard::Truthy {
            path: condition_path.clone(),
        });
        guard_sets.push(guards);
    }

    let mut fallback_guards = absent_guards(&condition_paths);
    if tag_paths.is_empty() {
        guard_sets.push(fallback_guards);
    } else {
        let mut tag_truthy_guards = fallback_guards.clone();
        tag_truthy_guards.push(truthy_tag_guard(&tag_paths));
        guard_sets.push(tag_truthy_guards);

        fallback_guards.extend(absent_guards(&tag_paths));
        guard_sets.push(fallback_guards);
    }

    guard_sets
}

fn trimmed_nonempty(paths: &[String]) -> impl Iterator<Item = String> + '_ {
    paths.iter().filter_map(|path| {
        let path = path.trim();
        (!path.is_empty()).then_some(path.to_string())
    })
}

fn normalized_ordered_paths(paths: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut normalized = Vec::new();
    for path in trimmed_nonempty(paths) {
        if seen.insert(path.clone()) {
            normalized.push(path);
        }
    }
    normalized
}

fn normalized_sorted_paths(paths: &[String]) -> Vec<String> {
    let mut normalized = trimmed_nonempty(paths).collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn absent_guards(paths: &[String]) -> Vec<Guard> {
    paths
        .iter()
        .cloned()
        .map(|path| Guard::Absent { path })
        .collect()
}

fn truthy_tag_guard(tag_paths: &[String]) -> Guard {
    match tag_paths {
        [path] => Guard::Truthy { path: path.clone() },
        paths => Guard::Or {
            paths: paths.to_vec(),
        },
    }
}

/// Helper names owned SOLELY by one optional direct dependency of a chart,
/// with the predicate under which that dependency is INACTIVE. An
/// unconditionally reached include of such a helper aborts rendering with
/// "no template" while the dependency is inactive, so the inactive states
/// become terminal clauses on the including chart.
pub(crate) struct OptionalDependencyHelpers {
    pub(crate) helper_names: std::collections::BTreeSet<String>,
    pub(crate) inactive: helm_schema_core::Predicate,
}

pub(crate) fn optional_dependency_helpers_for_chart(
    chart: &chart::ChartContext,
    charts: &[chart::ChartContext],
    defines: &helm_schema_ast::DefineIndex,
) -> Vec<OptionalDependencyHelpers> {
    // Owner of each define: the deepest chart whose directory contains the
    // defining file. Names defined in several places have ambiguous
    // resolution (Helm's global namespace, last parse wins) and abstain.
    let mut owner_dirs: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    // Define-index paths are chart-relative while chart dirs carry a
    // leading slash; compare both without it and on directory boundaries.
    let normalized_dir =
        |chart: &chart::ChartContext| chart.chart_dir.as_str().trim_start_matches('/').to_string();
    let file_in_dir = |file: &str, dir: &str| {
        let file = file.trim_start_matches('/');
        dir.is_empty() || file == dir || file.starts_with(&format!("{dir}/"))
    };
    for (path, src) in defines.file_sources() {
        let Some(owner) = charts
            .iter()
            .map(normalized_dir)
            .filter(|dir| file_in_dir(path, dir))
            .max_by_key(String::len)
        else {
            continue;
        };
        for name in helm_schema_ir::define_names_in_source(src) {
            owner_dirs.entry(name).or_default().insert(owner.clone());
        }
    }
    let mut out = Vec::new();
    for dependency in charts {
        let direct_child = dependency.values_prefix.len() == chart.values_prefix.len() + 1
            && dependency.values_prefix.starts_with(&chart.values_prefix);
        if !direct_child {
            continue;
        }
        // The including chart's own activation guards are appended to its
        // whole contract (terminal clauses included) AFTER these predicates
        // are added, so the inactive predicate must cover only the edge
        // RELATIVE to the including chart: the dependency's chain minus the
        // parent's prefix.
        let relative_chain = dependency
            .dependency_activation_chain
            .get(chart.dependency_activation_chain.len()..)
            .unwrap_or_default();
        let activation_sets = chart_activation_guard_sets(relative_chain);
        if activation_sets.is_empty() {
            continue;
        }
        let dependency_dir = normalized_dir(dependency);
        let helper_names: std::collections::BTreeSet<String> = owner_dirs
            .iter()
            .filter(|(_, owners)| {
                owners.iter().all(|owner| {
                    owner == &dependency_dir || owner.starts_with(&format!("{dependency_dir}/"))
                })
            })
            .map(|(name, _)| name.clone())
            .collect();
        if helper_names.is_empty() {
            continue;
        }
        let alternatives: Vec<helm_schema_core::Predicate> = activation_sets
            .into_iter()
            .map(|set| {
                helm_schema_core::Predicate::all(
                    set.into_iter()
                        .map(helm_schema_core::Predicate::from)
                        .collect(),
                )
            })
            .collect();
        out.push(OptionalDependencyHelpers {
            helper_names,
            inactive: helm_schema_core::Predicate::Or(alternatives).negated(),
        });
    }
    out
}

/// The transitive closure of helper names a template source reaches through
/// UNCONDITIONAL includes: the source's own top-level includes, plus each
/// reached helper body's top-level includes.
pub(crate) fn unconditional_include_closure(
    source: &str,
    defines: &helm_schema_ast::DefineIndex,
) -> std::collections::BTreeSet<String> {
    let mut bodies: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (_, src) in defines.file_sources() {
        for (name, body) in helm_schema_ir::define_bodies_in_source(src) {
            bodies.insert(name, body);
        }
    }
    let mut reached = std::collections::BTreeSet::new();
    let mut queue: Vec<String> = helm_schema_ast::unconditional_include_names(source)
        .into_iter()
        .collect();
    while let Some(name) = queue.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        if let Some(body) = bodies.get(&name) {
            queue.extend(helm_schema_ast::unconditional_include_names(body));
        }
    }
    reached
}
