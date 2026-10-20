//! Bounded dependency controls recovered from the chart loader's metadata.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{
    AcceptanceInputs, ValuesError,
    chart::{Chart, Requirement},
    dependencies,
    go_value::{table_from_json, table_to_json},
};

/// One loaded dependency instance and its ordered activation controls.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DependencyControl {
    /// Alias-aware path to the dependency values scope.
    pub scope: Vec<String>,
    /// Condition paths in Helm's fallback order, relative to the root.
    pub conditions: Vec<Vec<String>>,
    /// Tag paths relative to the root.
    pub tags: Vec<Vec<String>>,
    /// Whether chart metadata declares this instance.
    pub declared: bool,
}

/// The input operation a dependency row exercises.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DependencyProbeKind {
    /// Unmodified chart defaults.
    Defaults,
    /// Activate a dependency through a condition or tag.
    Enable,
    /// Deactivate a dependency through a condition or tag.
    Disable,
    /// Delete one condition and observe fallback processing.
    ConditionNull,
    /// Delete the dependency values scope.
    ScopeNull,
    /// Set one parsed subchart default under its enabled, alias-aware parent scope.
    PartialOverride,
}

/// A deterministic dependency overlay and its observed activation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DependencyProbe {
    /// Complete values overlay, including ancestor activation.
    pub overlay: Value,
    /// Dependency values scope; empty for defaults.
    pub scope: Vec<String>,
    /// Input operation represented by this row.
    pub kind: DependencyProbeKind,
    /// Actual activation after Helm's dependency processing.
    pub enabled: Option<bool>,
    /// Intrinsic Helm abort observed while processing this overlay, if any.
    pub not_validated: Option<String>,
}

impl AcceptanceInputs {
    /// Return loaded dependency instances with alias-aware activation paths.
    ///
    /// # Errors
    ///
    /// Refuses inventories deeper than 64 levels or incompatible version syntax.
    pub fn dependency_controls(&self) -> Result<Vec<DependencyControl>, ValuesError> {
        let mut instances = Vec::new();
        inventory(&self.chart, &[], &mut instances)?;
        Ok(instances
            .into_iter()
            .map(|DependencyInstance { control, chart: _ }| control)
            .collect())
    }

    /// Produce defaults and bounded dependency activation, deletion, and partial override rows.
    ///
    /// Null rows record observed activation rather than assuming deletion disables a chart.
    /// Every row is processed using a fresh deep copy of the cached chart.
    ///
    /// # Errors
    ///
    /// Fails instead of truncating inventories or more than 8,192 distinct overlays.
    /// Loader, dependency processing, or ineffective activation controls also fail.
    pub fn dependency_probes(&self) -> Result<Vec<DependencyProbe>, ValuesError> {
        let mut instances = Vec::new();
        inventory(&self.chart, &[], &mut instances)?;
        let mut probes = vec![DependencyProbe {
            overlay: json!({}),
            scope: Vec::new(),
            kind: DependencyProbeKind::Defaults,
            enabled: None,
            not_validated: None,
        }];
        let mut distinct = BTreeSet::from(["{}".to_string()]);
        for DependencyInstance { control, chart } in &instances {
            let mut ancestors = json!({});
            for DependencyInstance {
                control: ancestor,
                chart: _,
            } in &instances
            {
                if control.scope.starts_with(&ancestor.scope)
                    && ancestor.scope.len() < control.scope.len()
                {
                    activate(&mut ancestors, ancestor, true);
                }
            }
            let row_count = control.conditions.len().saturating_mul(3)
                + usize::from(!control.tags.is_empty()) * 2
                + 2;
            if probes.len().saturating_add(row_count) > 8192 {
                return Err(ValuesError::Unmodelled(
                    "dependency row cap exceeded: more than 8192 rows".to_string(),
                ));
            }
            for PlannedProbe {
                overlay,
                kind,
                expected,
            } in planned_probes(control, &ancestors, chart)
            {
                let mut not_validated = None;
                let enabled = match self.dependency_enabled(&overlay, &control.scope) {
                    Ok(enabled) => Some(enabled),
                    Err(ValuesError::NotValidated(reason)) => {
                        not_validated = Some(reason);
                        None
                    }
                    Err(err) => return Err(err),
                };
                if expected.is_some() && expected != enabled {
                    return Err(ValuesError::Unmodelled(format!(
                        "ineffective dependency {kind:?} control for {}",
                        control.scope.join(".")
                    )));
                }
                distinct.insert(overlay.to_string());
                if distinct.len() > 8192 {
                    return Err(ValuesError::Unmodelled(
                        "dependency probe cap exceeded: more than 8192 overlays".to_string(),
                    ));
                }
                probes.push(DependencyProbe {
                    overlay,
                    scope: control.scope.clone(),
                    kind,
                    enabled,
                    not_validated,
                });
            }
        }
        Ok(probes)
    }

    fn dependency_enabled(&self, overlay: &Value, scope: &[String]) -> Result<bool, ValuesError> {
        let mut chart = self.chart.deep_copy();
        let values = table_from_json(overlay.as_object().cloned().unwrap_or_default());
        dependencies::process_dependencies(&mut chart, &values, &mut BTreeSet::new())?;
        let mut current = &chart;
        for segment in scope {
            let Some(child) = current
                .dependencies
                .iter()
                .find(|child| &child.name == segment)
            else {
                return Ok(false);
            };
            current = child;
        }
        Ok(true)
    }
}

fn activate(overlay: &mut Value, control: &DependencyControl, enabled: bool) {
    if let Some(condition) = control.conditions.first() {
        assign(overlay, condition, Value::Bool(enabled));
    }
    for tag in &control.tags {
        assign(overlay, tag, Value::Bool(enabled));
    }
}

fn assign(overlay: &mut Value, path: &[String], value: Value) {
    let Some((key, parents)) = path.split_last() else {
        return;
    };
    let mut current = overlay;
    for parent in parents {
        if !current.is_object() {
            *current = json!({});
        }
        current = &mut current[parent];
    }
    if !current.is_object() {
        *current = json!({});
    }
    current[key] = value;
}

struct DependencyInstance<'a> {
    control: DependencyControl,
    chart: &'a Chart,
}

fn inventory<'a>(
    chart: &'a Chart,
    scope: &[String],
    instances: &mut Vec<DependencyInstance<'a>>,
) -> Result<(), ValuesError> {
    if scope.len() > 64 {
        return Err(ValuesError::Unmodelled(
            "dependency depth cap exceeded: more than 64 levels".to_string(),
        ));
    }
    if instances.len() > 8192 {
        return Err(ValuesError::Unmodelled(
            "dependency inventory cap exceeded".to_string(),
        ));
    }
    for child in &chart.dependencies {
        let mut matched = false;
        for requirement in chart.requirements.iter().flatten() {
            if child.name != requirement.name
                || !super::semver::is_compatible_range(&requirement.version, &child.version)?
            {
                continue;
            }
            matched = true;
            let name = if requirement.alias.is_empty() {
                &child.name
            } else {
                &requirement.alias
            };
            let mut child_scope = scope.to_vec();
            child_scope.push(name.clone());
            instances.push(DependencyInstance {
                control: dependency_control(scope, child_scope.clone(), Some(requirement), true)?,
                chart: child,
            });
            inventory(child, &child_scope, instances)?;
        }
        if !matched {
            let mut child_scope = scope.to_vec();
            child_scope.push(child.name.clone());
            // Helm can disable an unmatched loaded version through a same-name requirement.
            let requirement = chart.requirements.iter().flatten().find(|requirement| {
                requirement.name == child.name
                    && (requirement.alias.is_empty() || requirement.alias == child.name)
            });
            instances.push(DependencyInstance {
                control: dependency_control(scope, child_scope.clone(), requirement, false)?,
                chart: child,
            });
            inventory(child, &child_scope, instances)?;
        }
    }
    Ok(())
}

fn dependency_control(
    parent_scope: &[String],
    scope: Vec<String>,
    requirement: Option<&Requirement>,
    declared: bool,
) -> Result<DependencyControl, ValuesError> {
    let mut control = DependencyControl {
        scope,
        conditions: Vec::new(),
        tags: Vec::new(),
        declared,
    };
    if let Some(requirement) = requirement {
        if requirement.condition.split(',').count() > 8192 || requirement.tags.len() > 8192 {
            return Err(ValuesError::Unmodelled(
                "dependency control path cap exceeded".to_string(),
            ));
        }
        for path in requirement
            .condition
            .trim()
            .split(',')
            .filter(|path| !path.is_empty())
        {
            let mut full = parent_scope.to_vec();
            full.extend(path.split('.').map(str::to_string));
            control.conditions.push(full);
        }
        control.tags = requirement
            .tags
            .iter()
            .map(|tag| vec!["tags".to_string(), tag.clone()])
            .collect();
    }
    Ok(control)
}

struct PlannedProbe {
    overlay: Value,
    kind: DependencyProbeKind,
    expected: Option<bool>,
}

fn planned_probes(
    control: &DependencyControl,
    ancestors: &Value,
    chart: &Chart,
) -> Vec<PlannedProbe> {
    let mut rows = Vec::new();
    for (index, condition) in control.conditions.iter().enumerate() {
        for enabled in [true, false] {
            let mut overlay = ancestors.clone();
            for preceding in control.conditions.iter().take(index) {
                assign(&mut overlay, preceding, Value::Null);
            }
            assign(&mut overlay, condition, Value::Bool(enabled));
            rows.push(PlannedProbe {
                overlay,
                kind: if enabled {
                    DependencyProbeKind::Enable
                } else {
                    DependencyProbeKind::Disable
                },
                expected: Some(enabled),
            });
        }
        let mut overlay = ancestors.clone();
        activate(&mut overlay, control, true);
        assign(&mut overlay, condition, Value::Null);
        rows.push(PlannedProbe {
            overlay,
            kind: DependencyProbeKind::ConditionNull,
            expected: None,
        });
    }
    if !control.tags.is_empty() {
        for enabled in [true, false] {
            let mut overlay = ancestors.clone();
            for condition in &control.conditions {
                assign(&mut overlay, condition, Value::Null);
            }
            for tag in &control.tags {
                assign(&mut overlay, tag, Value::Bool(enabled));
            }
            rows.push(PlannedProbe {
                overlay,
                kind: if enabled {
                    DependencyProbeKind::Enable
                } else {
                    DependencyProbeKind::Disable
                },
                expected: Some(enabled),
            });
        }
    }
    let mut overlay = ancestors.clone();
    activate(&mut overlay, control, true);
    assign(&mut overlay, &control.scope, Value::Null);
    rows.push(PlannedProbe {
        overlay,
        kind: DependencyProbeKind::ScopeNull,
        expected: None,
    });
    let mut overlay = json!({});
    assign(
        &mut overlay,
        &control.scope,
        partial_default(&Value::Object(table_to_json(&chart.values))),
    );
    let mut overlay = super::coalesce_tables(ancestors, &overlay);
    // Activation wins when the selected default is itself a false condition or tag.
    activate(&mut overlay, control, true);
    rows.push(PlannedProbe {
        overlay,
        kind: DependencyProbeKind::PartialOverride,
        expected: Some(true),
    });
    rows
}

fn partial_default(value: &Value) -> Value {
    let Value::Object(values) = value else {
        return value.clone();
    };
    // Null deletes a parent default instead of supplying an ordinary partial override.
    // Empty/all-null values still need an explicit scope control, including library charts.
    values
        .iter()
        .find(|(_, value)| !value.is_null())
        .map_or_else(
            || json!({}),
            |(key, value)| json!({key: partial_default(value)}),
        )
}
