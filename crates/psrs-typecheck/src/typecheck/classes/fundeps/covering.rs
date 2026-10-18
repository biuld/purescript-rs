//! The inclusion-minimal covering sets a class's functional dependencies
//! define, official `typeClassCoveringSets` from `Environment.computeCoveringSets`.
//! Both the overlap check and the orphan check read them, and the
//! determined-instance-head rule derives its fully-determined arguments from
//! them, so this is the one definition of the notion in the typechecker.

use super::super::super::vocabulary::FundepInfo;
use std::collections::{BTreeMap, BTreeSet};

/// Computes the minimal sets of class parameters that determine all parameters
/// under the class's functional dependencies. A class without dependencies has
/// one covering set over every parameter, mirroring the one covering set
/// `purs`'s `typeClassCoveringSets` records.
///
/// The search is the official frontier: start from the full parameter set,
/// which is known to cover everything, and repeatedly remove a parameter some
/// still-applicable dependency determines, dropping the dependencies that
/// stop applying, until a needed set no dependency can reduce remains. Each
/// recorded set is locally minimal (no fundep reduces it), but frontiers can
/// still reach sets that contain one another, so the result is finally reduced
/// to the inclusion-minimal sets exactly as `computeCoveringSets` does.
pub(in crate::typecheck) fn covering_sets(
    parameter_count: usize,
    fundeps: &[FundepInfo],
) -> BTreeSet<BTreeSet<usize>> {
    // Without dependencies no fundep reduces the full parameter set, so the
    // one covering set is every parameter.
    let all = (0..parameter_count).collect::<BTreeSet<_>>();
    let mut locally_minimal = BTreeSet::from([all.clone()]);
    if fundeps.is_empty() {
        return locally_minimal;
    }
    // The frontier: a needed set paired, for each determined parameter it can
    // still remove, with the determining sets its dependencies offer. Several
    // dependencies may determine the same parameter, so the positions
    // accumulate lists like the official `M.fromListWith (<>)` does.
    let initial = fundeps
        .iter()
        .flat_map(|fundep| {
            fundep.determined.iter().map(|&target| {
                (
                    target,
                    fundep.determining.iter().copied().collect::<BTreeSet<_>>(),
                )
            })
        })
        .fold(
            BTreeMap::new(),
            |mut edges: BTreeMap<usize, Vec<BTreeSet<usize>>>, (target, sources)| {
                edges.entry(target).or_default().push(sources);
                edges
            },
        );
    // The official frontier is keyed by the needed set and merges repeated
    // arrivals keeping the first, so the same entry does here.
    let mut frontier: BTreeMap<BTreeSet<usize>, BTreeMap<usize, Vec<BTreeSet<usize>>>> =
        BTreeMap::from([(all, initial)]);
    while let Some((needed, in_edges)) = frontier.pop_first() {
        if in_edges.is_empty() {
            locally_minimal.insert(needed);
            continue;
        }
        let source_positions = in_edges
            .values()
            .flatten()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>();
        // A dependency that determines an argument no surviving dependency
        // uses is acyclic: removing it cannot spur other removals, so trying
        // only it is what the official search prefers.
        let acyclic = in_edges
            .keys()
            .find(|&y| !source_positions.contains(y))
            .copied();
        let candidates = match acyclic {
            Some(y) => vec![y],
            None => in_edges.keys().copied().collect(),
        };
        for y in candidates {
            let mut next_needed = needed.clone();
            next_needed.remove(&y);
            let mut next_edges = in_edges.clone();
            next_edges.remove(&y);
            if acyclic.is_none() {
                for sources in next_edges.values_mut() {
                    sources.retain(|source| !source.contains(&y));
                }
                next_edges.retain(|_, sources| !sources.is_empty());
            }
            frontier.entry(next_needed).or_insert(next_edges);
        }
    }
    // Reduce to the inclusion-minimal sets: a set another recorded set
    // properly contains is not minimal.
    locally_minimal
        .iter()
        .filter(|set| {
            !locally_minimal
                .iter()
                .any(|other| other.len() < set.len() && other.is_subset(set))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one fundep from parameter positions.
    fn fundep(determining: &[usize], determined: &[usize]) -> FundepInfo {
        FundepInfo {
            determining: determining.to_vec(),
            determined: determined.to_vec(),
        }
    }

    #[test]
    fn a_dependency_through_an_extra_parameter_covers_the_pair_it_names() {
        // The corpus shape `NonOrphanInstanceFunDepExtra`:
        // `class C f l r | l -> r`. The official
        // `typeClassCoveringSets` records exactly `{{f, l}}` (the comment in
        // the corpus case's `Lib.purs` states this), so the determined third
        // parameter must not appear and the full parameter set must not be
        // kept as a superset of `{f, l}`.
        let sets = covering_sets(3, &[fundep(&[1], &[2])]);
        assert_eq!(sets, BTreeSet::from([BTreeSet::from([0, 1])]));
    }

    #[test]
    fn a_class_without_dependencies_covers_every_parameter_once() {
        let sets = covering_sets(3, &[]);
        assert_eq!(sets, BTreeSet::from([BTreeSet::from([0, 1, 2])]));
    }

    #[test]
    fn a_mutual_cycle_leaves_each_cycle_member_as_its_own_covering_set() {
        // The official `makeTypeClassData` example:
        // `class C a b c | a -> b, b -> a, b -> c`. The `a`/`b` cycle makes
        // each of them cover everything on its own, and `c` is fully
        // determined, so the minimal sets are `{{a}, {b}}`.
        let sets = covering_sets(
            3,
            &[fundep(&[0], &[1]), fundep(&[1], &[0]), fundep(&[1], &[2])],
        );
        assert_eq!(
            sets,
            BTreeSet::from([BTreeSet::from([0]), BTreeSet::from([1])])
        );
    }

    #[test]
    fn a_chained_dependency_collapses_to_the_head_of_the_chain() {
        // `class C a b c | a -> b, b -> c`: `a` alone determines both, and the
        // supersets `{a, b}` and `{a, b, c}` the enumeration passes through
        // are not minimal.
        let sets = covering_sets(3, &[fundep(&[0], &[1]), fundep(&[1], &[2])]);
        assert_eq!(sets, BTreeSet::from([BTreeSet::from([0])]));
    }
}
