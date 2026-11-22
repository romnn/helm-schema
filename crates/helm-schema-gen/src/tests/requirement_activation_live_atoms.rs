use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

use super::{AtomSet, AtomSets};
use crate::requirement_activation::tests::{DENSE_GROUPS, dense_atom_groups};

#[test]
fn dense_subset_supports_retain_one_node_per_symbolic_join() -> eyre::Result<()> {
    let atom_count = 1 << DENSE_GROUPS;
    let mut support = AtomSets::new(atom_count);
    let groups = dense_atom_groups()?
        .into_iter()
        .map(|group| support.leaf(group))
        .collect::<Vec<_>>();
    let mut subsets = vec![AtomSet::EMPTY];
    for mask in 1..atom_count {
        let smallest = usize::try_from(mask.trailing_zeros())?;
        let rest = mask & (mask - 1);
        let group = *groups.get(smallest).ok_or_eyre("group atom support")?;
        let predecessor = *subsets.get(rest).ok_or_eyre("subset atom support")?;
        subsets.push(
            support
                .union(predecessor, group)
                .ok_or_eyre("union atom support")?,
        );
    }
    sim_assert_eq!(have: support.nodes.len(), want: atom_count);
    for (mask, set) in subsets.into_iter().enumerate().step_by(31) {
        sim_assert_eq!(have: support.contains(set, 0), want: mask & 1 != 0);
        sim_assert_eq!(have: support.contains(set, 1), want: mask & 2 != 0);
    }
    Ok(())
}

#[test]
fn overlapping_distinct_frontiers_avoid_pairwise_comparisons() -> eyre::Result<()> {
    let shared_atoms = 4_096;
    let distinct_atoms = 512;
    let upper = shared_atoms + distinct_atoms;
    let mut support = AtomSets::new(upper + 1);
    let shared = support.leaf((0..shared_atoms).chain(std::iter::once(upper)).collect());
    let mut requested = Vec::new();
    for ordinal in shared_atoms..upper {
        let one = support.leaf(vec![ordinal]);
        requested.push(
            support
                .union(shared, one)
                .ok_or_eyre("continuation union")?,
        );
    }
    sim_assert_eq!(have: support.frontiers.len(), want: 0);
    let mut frontiers = std::collections::BTreeSet::new();
    for requested in requested {
        frontiers.insert(
            support
                .frontier(requested)
                .ok_or_eyre("continuation frontier")?,
        );
    }
    sim_assert_eq!(have: frontiers.len(), want: distinct_atoms);
    let comparisons = support.frontier_comparisons();
    eyre::ensure!(
        comparisons <= distinct_atoms,
        "expected at most {distinct_atoms} exact frontier comparisons, got {comparisons}"
    );
    Ok(())
}
