//! The determined-instance-head rule the common instance recorder applies to
//! every local instance.
//!
//! `purs` raises `InvalidInstanceHead` from `TypeChecker.checkTypeClassInstance`
//! for any head argument that is not a type variable, a type-level literal, a
//! type constructor, a function or constructor application over those shapes,
//! or a row in an argument a functional dependency fully determines. The
//! triggers differ — a wildcard (`TypeWildcards3.purs`), a free unification
//! variable under a synonym (`TypeSynonyms7.purs`), a record head the
//! derivation cannot determine (`3510.purs`,
//! `InvalidDerivedInstance2.purs`), and an argument no dependency determines
//! (`RowInInstanceNotDetermined0/1/2.purs`) — but the check is one rule about
//! the head's shape, applied per argument after synonyms expand.

use super::super::super::*;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Applies the head rule to one instance's elaborated head arguments. The
/// return is `false` when an argument rejects, with the diagnostic already
/// reported at the instance head's range.
pub(in crate::typecheck) fn validate_head_shapes(
    checker: &mut Checker,
    class: &ClassInfo,
    head_arguments: &[InferType],
    head_span: TextRange,
) -> bool {
    let determined = determined_fundep_arguments(class.parameters.len(), &class.fundeps);
    let invalid = head_arguments
        .iter()
        .enumerate()
        .find_map(|(index, argument)| {
            (!valid_instance_head_argument(argument, determined.contains(&index)))
                .then_some(argument)
        });
    let Some(argument) = invalid else {
        return true;
    };
    let rendered = checker.display_type(argument);
    checker.state.errors.push(TypeCheckError::new(
        TypeCheckErrorKind::InvalidInstanceHead,
        head_span,
        format!(
            "an instance head argument cannot be `{rendered}`; \
             each head argument must be a type constructor application or a type \
             variable, and a row argument must be fully determined by the class's \
             functional dependencies",
        ),
    ));
    false
}

/// Whether one instance-head argument may appear at its position.
///
/// This is `checkTypeClassInstance`'s `check` in the official
/// `TypeChecker.hs`, ported to the inference representation. A row argument is
/// weakly determined, so it cannot influence instance selection; that is why a
/// row is legal only when a functional dependency fully determines its
/// position, and a row found inside a record application
/// (`Record (x :: Int | r)`) shares its argument's flag.
pub(in crate::typecheck) fn valid_instance_head_argument(
    ty: &InferType,
    fundep_determined: bool,
) -> bool {
    match ty {
        InferType::Variable(_)
        | InferType::Constructor(_)
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => true,
        InferType::Application(function, argument) => {
            valid_instance_head_argument(function, fundep_determined)
                && valid_instance_head_argument(argument, fundep_determined)
        }
        InferType::RowEmpty => fundep_determined,
        InferType::RowExtend {
            ty: field, tail, ..
        } => {
            fundep_determined
                && valid_instance_head_argument(field, fundep_determined)
                && valid_instance_head_argument(tail, fundep_determined)
        }
        InferType::ForAll { .. } | InferType::Constrained { .. } => false,
    }
}

/// The argument positions of a class that no covering set contains: official
/// `typeClassDeterminedArguments` in `Environment.hs`.
///
/// A covering set is an inclusion-minimal set of argument positions from which
/// every remaining argument follows through the class's functional
/// dependencies; an argument is fully determined exactly when it stands in no
/// covering set. This is a port of `computeCoveringSets`, reduced to the
/// determined set. The frontier starts at the full parameter set, and each
/// still-applicable dependency removes one parameter it determines — including
/// parameters that dependency no longer sees in its determining positions —
/// until nothing reduces any set further. An argument that ends up needed by
/// every covering set is one the dependencies can never pin down, so it must
/// be a selectable part of the head.
pub(in crate::typecheck) fn determined_fundep_arguments(
    parameter_count: usize,
    fundeps: &[FundepInfo],
) -> HashSet<usize> {
    // Without dependencies every argument is needed: the only covering set is
    // the full parameter list, and no argument is fully determined.
    if fundeps.is_empty() {
        return HashSet::new();
    }
    // The determined parameter each dependency can remove next, paired with
    // the determining sets it offers. Several dependencies may determine the
    // same parameter, so the positions accumulate lists like the official
    // `M.fromListWith (<>)` does.
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
    let all = (0..parameter_count).collect::<BTreeSet<_>>();
    // The frontier: a needed set paired with its applicable dependencies.
    // The official frontier is keyed by the needed set and merges repeated
    // arrivals keeping the first, so the same entry does here.
    let mut frontier: BTreeMap<BTreeSet<usize>, BTreeMap<usize, Vec<BTreeSet<usize>>>> =
        BTreeMap::from([(all, initial)]);
    let mut covering_sets: HashSet<BTreeSet<usize>> = HashSet::new();
    while let Some((needed, in_edges)) = frontier.pop_first() {
        if in_edges.is_empty() {
            covering_sets.insert(needed);
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
    // Reduce to the inclusion-minimal covering sets, then an argument is
    // determined when no covering set contains it.
    let minimal = covering_sets.iter().filter(|set| {
        !covering_sets
            .iter()
            .any(|other| other.len() < set.len() && other.is_subset(set))
    });
    let mut determined: HashSet<usize> = (0..parameter_count).collect();
    for set in minimal {
        for position in set.iter() {
            determined.remove(position);
        }
    }
    determined
}
