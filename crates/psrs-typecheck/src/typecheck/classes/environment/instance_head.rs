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
/// Officially the determined set falls out of `computeCoveringSets`:
/// `determinedArgs = argumentIndices S.\\ fold coveringSets`, so this derives
/// it from the one shared covering-sets definition in `classes::fundeps`
/// rather than re-running the frontier search here. An argument that ends up
/// needed by every covering set is one the dependencies can never pin down,
/// so it must be a selectable part of the head.
pub(in crate::typecheck) fn determined_fundep_arguments(
    parameter_count: usize,
    fundeps: &[FundepInfo],
) -> HashSet<usize> {
    let mut determined: HashSet<usize> = (0..parameter_count).collect();
    for set in super::super::fundeps::covering_sets(parameter_count, fundeps) {
        for position in set {
            determined.remove(&position);
        }
    }
    determined
}
