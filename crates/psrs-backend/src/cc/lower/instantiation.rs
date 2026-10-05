//! Checked scheme/use evidence for representation conversion.
//!
//! The evidence borrows the module that owns the relation, not the lowerer, so
//! a later conversion can mutably borrow the lowerer.

use psrs_core::{Instantiation, Module, TypeId};
use psrs_hir::TypeVariableId;
use std::collections::HashSet;

/// Checked scheme/use bindings. Leading quantifiers are pre-seeded so opening
/// them retains constructor replacements. Relations whose type ids still exist
/// use the immutable source module.
pub(in crate::cc::lower) fn instantiation_at<'a>(
    source: &'a Module,
    physical: &'a Module,
    scheme: TypeId,
    instance: TypeId,
) -> Option<Instantiation<'a>> {
    let relation =
        if (scheme.0 as usize) < source.types.len() && (instance.0 as usize) < source.types.len() {
            source
        } else {
            physical
        };
    let quantified = scheme_quantifiers(relation, scheme);
    relation.checked_instantiation(scheme, &quantified, instance)
}

/// Leading quantifiers of a scheme. Pre-seeding these keeps their constructor
/// bindings when instantiation opens the same quantifiers.
fn scheme_quantifiers(module: &Module, mut ty: TypeId) -> Vec<TypeVariableId> {
    let mut variables = Vec::new();
    let mut seen = HashSet::new();
    while let Some((bound, body)) = psrs_core::forall_parts(&module.types, ty) {
        if !seen.insert(ty) {
            break;
        }
        variables.extend(bound.iter().copied());
        ty = body;
    }
    variables
}
