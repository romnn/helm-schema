use std::collections::BTreeMap;

use color_eyre::eyre;
use test_util::prelude::sim_assert_eq;

use crate::corpus_replay::validate_census_charts;

#[test]
fn every_census_document_belongs_to_a_scheduled_chart() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("census-chart-coverage")?;
    let valid = root
        .path()
        .join("cases/chart-demo/run/schema-verdicts.json")
        .display()
        .to_string()
        + "#/document";
    validate_census_charts(
        root.path(),
        ["demo"],
        &BTreeMap::from([(valid, vec!["A".into()])]),
    )?;
    for relative in [
        "cases/chart-typo/missing/coalesced.json",
        "cases/chart-signoz-postgresql/missing/coalesced.json",
        "cases/chart-demolition/missing/coalesced.json",
        "unrelated/coalesced.json",
    ] {
        let source = root.path().join(relative).display().to_string();
        let classes = BTreeMap::from([(source.clone(), vec!["A".into()])]);
        let result = validate_census_charts(root.path(), ["demo"], &classes);
        sim_assert_eq!(
            have: result.err().map(|err| err.to_string()),
            want: Some(format!("invalid replay input: census document outside replay chart schedule: {source}"))
        );
    }
    Ok(())
}
