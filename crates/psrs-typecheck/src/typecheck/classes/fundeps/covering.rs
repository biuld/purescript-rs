//! The minimal covering sets a class's functional dependencies define. Both
//! the overlap check and the orphan check read them.

use super::super::super::vocabulary::FundepInfo;
use std::collections::HashSet;

/// Computes the minimal sets of class parameters that determine all parameters
/// under the class's functional dependencies. A class without dependencies has
/// one covering set over every parameter, mirroring the one covering set
/// `purs`'s `typeClassCoveringSets` records.
pub(in crate::typecheck::classes) fn covering_sets(
    parameter_count: usize,
    fundeps: &[FundepInfo],
) -> Vec<Vec<usize>> {
    if fundeps.is_empty() {
        return vec![(0..parameter_count).collect()];
    }
    let mut result: Vec<Vec<usize>> = Vec::new();
    enumerate_covering_sets(parameter_count, fundeps, 0, &mut Vec::new(), &mut result);
    result
}

fn enumerate_covering_sets(
    parameter_count: usize,
    fundeps: &[FundepInfo],
    next: usize,
    chosen: &mut Vec<usize>,
    result: &mut Vec<Vec<usize>>,
) {
    if dependency_closure(chosen, fundeps).len() == parameter_count {
        if !result.iter().any(|cover| is_subset(cover, chosen)) {
            result.push(chosen.clone());
        }
        return;
    }
    let mut possible = chosen.clone();
    possible.extend(next..parameter_count);
    if dependency_closure(&possible, fundeps).len() != parameter_count || next == parameter_count {
        return;
    }
    chosen.push(next);
    enumerate_covering_sets(parameter_count, fundeps, next + 1, chosen, result);
    chosen.pop();
    enumerate_covering_sets(parameter_count, fundeps, next + 1, chosen, result);
}

fn is_subset(left: &[usize], right: &[usize]) -> bool {
    left.iter().all(|index| right.contains(index))
}

/// Closes a seed of parameter positions over the dependencies: each dependency
/// whose determining positions are all present adds its determined ones.
fn dependency_closure(seed: &[usize], fundeps: &[FundepInfo]) -> HashSet<usize> {
    let mut closure = seed.iter().copied().collect::<HashSet<_>>();
    loop {
        let before = closure.len();
        for fundep in fundeps {
            if fundep
                .determining
                .iter()
                .all(|index| closure.contains(index))
            {
                closure.extend(fundep.determined.iter().copied());
            }
        }
        if closure.len() == before {
            return closure;
        }
    }
}
