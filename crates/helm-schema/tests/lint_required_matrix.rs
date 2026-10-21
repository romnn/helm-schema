//! Helm differential coverage for lint-visible dependency requirements.

mod lint_required_support;

use color_eyre::eyre;
use lint_required_support::{Cell, DefaultValue, Feature, Matrix, Override, Transport};
use test_util::prelude::sim_assert_eq;

#[test]
fn dependency_presence_matches_pinned_helm_across_the_matrix() -> eyre::Result<()> {
    let mut matrix = Matrix::new()?;
    for depth in [1, 2] {
        for child in DefaultValue::ALL {
            for parent in DefaultValue::ALL {
                matrix.check_configuration(Cell {
                    depth,
                    child,
                    parent,
                    feature: Feature::Plain,
                    guarded: true,
                    transport: Transport::File,
                    overlay: Override::Absent,
                })?;
            }
        }
    }
    for feature in [
        Feature::Alias,
        Feature::Condition,
        Feature::Tags,
        Feature::Global,
        Feature::ImportChild,
        Feature::ImportParent,
        Feature::Library,
    ] {
        matrix.check_configuration(Cell {
            depth: 1,
            child: DefaultValue::Set,
            parent: DefaultValue::Absent,
            feature,
            guarded: true,
            transport: Transport::File,
            overlay: Override::Absent,
        })?;
    }
    // An unguarded provider sink exposes the original mandatory-constraint bypass.
    for depth in [1, 2] {
        for parent in [DefaultValue::Absent, DefaultValue::Set] {
            for transport in [Transport::File, Transport::Chart] {
                for overlay in [Override::Absent, Override::Partial, Override::Set] {
                    matrix.check(Cell {
                        depth,
                        child: DefaultValue::Set,
                        parent,
                        feature: Feature::Plain,
                        guarded: false,
                        transport,
                        overlay,
                    })?;
                }
            }
        }
    }
    let failures = matrix.finish()?;
    sim_assert_eq!(have: failures, want: Vec::<String>::new());
    Ok(())
}

/// Unguarded provider sinks reached through every dependency feature path, including
/// dependencies enabled by a condition or a tag and a global only the child defaults.
#[test]
fn unguarded_partial_overrides_match_pinned_helm_for_every_feature_path() -> eyre::Result<()> {
    let mut matrix = Matrix::new()?;
    let nested = [
        Feature::Alias,
        Feature::ConditionEnabled,
        Feature::TagsEnabled,
        Feature::Global,
        Feature::GlobalChildOnly,
    ];
    let mut paths = Vec::new();
    for feature in nested {
        paths.push((1, feature));
        paths.push((2, feature));
    }
    // Import and library cells select the root's direct dependency.
    for feature in [
        Feature::ImportChild,
        Feature::ImportParent,
        Feature::Library,
    ] {
        paths.push((1, feature));
    }
    for (depth, feature) in paths {
        for parent in [DefaultValue::Absent, DefaultValue::Set] {
            for transport in [Transport::File, Transport::Chart] {
                for overlay in [Override::Absent, Override::Partial, Override::Set] {
                    matrix.check(Cell {
                        depth,
                        child: DefaultValue::Set,
                        parent,
                        feature,
                        guarded: false,
                        transport,
                        overlay,
                    })?;
                }
            }
        }
    }
    let failures = matrix.finish()?;
    sim_assert_eq!(have: failures, want: Vec::<String>::new());
    Ok(())
}

/// A requirement a valid child default satisfies stays satisfiable from a parent that
/// omits it, while explicitly invalid tokens still abort and fail the schema.
#[test]
fn default_satisfied_template_requirements_accept_parent_omission() -> eyre::Result<()> {
    let mut matrix = Matrix::new()?;
    for feature in [Feature::RequiredDefaulted, Feature::FailDefaulted] {
        for transport in [Transport::File, Transport::Chart] {
            matrix.check_defaulted_required(Cell {
                depth: 1,
                child: DefaultValue::Set,
                parent: DefaultValue::Absent,
                feature,
                guarded: true,
                transport,
                overlay: Override::Absent,
            })?;
        }
    }
    let failures = matrix.finish()?;
    sim_assert_eq!(have: failures, want: Vec::<String>::new());
    Ok(())
}

#[test]
fn explicit_template_requirements_remain_required() -> eyre::Result<()> {
    let mut matrix = Matrix::new()?;
    for feature in [Feature::Required, Feature::Fail] {
        for transport in [Transport::File, Transport::Chart] {
            matrix.check_required(Cell {
                depth: 1,
                child: DefaultValue::Set,
                parent: DefaultValue::Absent,
                feature,
                guarded: true,
                transport,
                overlay: Override::Set,
            })?;
        }
    }
    let failures = matrix.finish()?;
    sim_assert_eq!(have: failures, want: Vec::<String>::new());
    Ok(())
}

#[test]
fn public_umbrella_defaults_match_pinned_helm() -> eyre::Result<()> {
    let mut matrix = Matrix::new()?;
    matrix.check_public_chart(&test_util::workspace_testdata().join("charts/daskhub"))?;
    let failures = matrix.finish()?;
    sim_assert_eq!(have: failures, want: Vec::<String>::new());
    Ok(())
}
