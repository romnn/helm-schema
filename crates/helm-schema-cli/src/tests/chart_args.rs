use clap::Parser as _;
use color_eyre::eyre;
use helm_schema::generation::{AuthoringPolicy, DeclaredTypes, RootPolicy};
use test_util::prelude::sim_assert_eq;

use crate::Cli;

fn authoring(arguments: &[&str]) -> eyre::Result<AuthoringPolicy> {
    let cli = Cli::try_parse_from(["helm-schema", "chart"].iter().chain(arguments))?;
    Ok(cli.chart.authoring_policy())
}

/// The strict authoring policy is the default; each flag relaxes exactly one
/// assertion, and the two compose.
#[test]
fn authoring_flags_default_strict_and_parse_independently() -> eyre::Result<()> {
    let have = vec![
        authoring(&[])?,
        authoring(&["--open-root"])?,
        authoring(&["--declared-types", "annotate"])?,
        authoring(&["--declared-types=assert"])?,
        authoring(&["--open-root", "--declared-types=annotate"])?,
    ];
    let policy = |root, declared_types| AuthoringPolicy {
        root,
        declared_types,
    };
    sim_assert_eq!(
        have: have,
        want: vec![
            policy(RootPolicy::Closed, DeclaredTypes::Assert),
            policy(RootPolicy::Open, DeclaredTypes::Assert),
            policy(RootPolicy::Closed, DeclaredTypes::Annotate),
            policy(RootPolicy::Closed, DeclaredTypes::Assert),
            policy(RootPolicy::Open, DeclaredTypes::Annotate),
        ]
    );
    sim_assert_eq!(have: authoring(&[])?, want: AuthoringPolicy::default());
    sim_assert_eq!(
        have: Cli::try_parse_from(["helm-schema", "chart", "--declared-types", "infer"]).is_err(),
        want: true
    );
    Ok(())
}
