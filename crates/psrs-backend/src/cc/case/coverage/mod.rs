//! Usefulness analysis over the decision compiler's shared pattern matrix.
//!
//! Coverage and first-match compilation use the same typed pattern algebra,
//! literal normalization, and constructor, scalar, and array heads. The
//! semantic analysis lives in the `psrs-coverage` crate, the one owner shared
//! with the type checker's `Partial` flow; this module adapts the core
//! module's type facts and the backend's pattern algebra onto that engine.

use super::decision::{SurfacePattern, surface_pattern};
use psrs_core::{CaseBranch, Module, Pattern, Type, TypeConstructor, TypeId};
use psrs_coverage::{Branch, ConstructorRow, Pattern as SharedPattern, Types as CoverageTypes};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CoverageReport {
    pub(super) exhaustive: bool,
    pub(super) witness: Option<String>,
    pub(super) redundant_branches: Vec<usize>,
}

impl CoverageReport {
    pub(super) fn non_exhaustive_message(&self, fallback: &'static str) -> String {
        match &self.witness {
            Some(witness) => format!("non-exhaustive case; missing pattern {witness}"),
            None => fallback.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests;

pub(super) fn analyze(
    module: &Module,
    scrutinee_type: TypeId,
    branches: &[CaseBranch],
) -> CoverageReport {
    let analysis = psrs_coverage::analyze(
        &ModuleTypes(module),
        scrutinee_type,
        &branches
            .iter()
            .map(|branch| Branch {
                coverage: branch.coverage,
                pattern: coverage_pattern(&branch.pattern),
            })
            .collect::<Vec<_>>(),
    );
    CoverageReport {
        exhaustive: analysis.exhaustive,
        // The analysis query is one column, so the first rendered atom is the
        // pre-crate message exactly.
        witness: analysis
            .witness_atoms
            .and_then(|mut row| (!row.is_empty()).then(|| row.swap_remove(0))),
        redundant_branches: analysis.redundant_branches,
    }
}

fn coverage_pattern(pattern: &Pattern) -> SharedPattern<TypeId> {
    shared_shape(erase_binders(surface_pattern(pattern)))
}

/// Converts the backend algebra's binder-erased shape onto the shared one.
fn shared_shape(pattern: SurfacePattern) -> SharedPattern<TypeId> {
    match pattern {
        SurfacePattern::Any { ty } => SharedPattern::Any { ty },
        SurfacePattern::Var { ty, .. } => SharedPattern::Any { ty },
        SurfacePattern::Named { pattern, .. } => shared_shape(*pattern),
        SurfacePattern::Literal { value, ty, .. } => SharedPattern::Literal {
            literal: shared_literal(value),
            ty,
        },
        SurfacePattern::Array { elements, ty, .. } => SharedPattern::Array {
            elements: elements.into_iter().map(shared_shape).collect(),
            ty,
        },
        SurfacePattern::Constructor {
            symbol,
            arguments,
            ty,
            ..
        } => SharedPattern::Constructor {
            symbol,
            arguments: arguments.into_iter().map(shared_shape).collect(),
            ty,
        },
        SurfacePattern::Record { fields, ty, .. } => SharedPattern::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, shared_shape(value)))
                .collect(),
            ty,
        },
    }
}

fn shared_literal(value: psrs_core::Literal) -> psrs_coverage::Literal {
    match value {
        psrs_core::Literal::Integer(value) => psrs_coverage::Literal::Integer(value),
        psrs_core::Literal::Number(value) => psrs_coverage::Literal::Number(value),
        psrs_core::Literal::String(value) => psrs_coverage::Literal::String(value),
        psrs_core::Literal::Char(value) => psrs_coverage::Literal::Char(value),
        psrs_core::Literal::Boolean(value) => psrs_coverage::Literal::Boolean(value),
    }
}

fn erase_binders(pattern: SurfacePattern) -> SurfacePattern {
    match pattern {
        SurfacePattern::Var { ty, .. } => SurfacePattern::Any { ty },
        SurfacePattern::Named { pattern, .. } => erase_binders(*pattern),
        SurfacePattern::Array { elements, ty, span } => SurfacePattern::Array {
            elements: elements.into_iter().map(erase_binders).collect(),
            ty,
            span,
        },
        SurfacePattern::Constructor {
            symbol,
            arguments,
            ty,
            span,
        } => SurfacePattern::Constructor {
            symbol,
            arguments: arguments.into_iter().map(erase_binders).collect(),
            ty,
            span,
        },
        SurfacePattern::Record { fields, ty, span } => SurfacePattern::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, erase_binders(value)))
                .collect(),
            ty,
            span,
        },
        literal @ SurfacePattern::Literal { .. } | literal @ SurfacePattern::Any { .. } => literal,
    }
}

/// Supplies the core module's type facts to the shared engine.
struct ModuleTypes<'m>(&'m Module);

impl CoverageTypes<TypeId> for ModuleTypes<'_> {
    fn shape(&self, ty: TypeId) -> TypeId {
        crate::cc::layout::unquantified_type(self.0, ty)
    }

    fn array_element(&self, ty: TypeId) -> Option<TypeId> {
        let ty = crate::cc::layout::unquantified_type(self.0, ty);
        let Type::Application(function, element) = self.0.types.get(ty.0 as usize)? else {
            return None;
        };
        matches!(
            self.0.types.get(function.0 as usize),
            Some(Type::Constructor(TypeConstructor::Array))
        )
        .then_some(*element)
    }

    fn constructors_of(&self, ty: TypeId) -> Option<Vec<ConstructorRow<TypeId>>> {
        let ty = crate::cc::layout::unquantified_type(self.0, ty);
        let type_id = crate::cc::layout::user_type_id(self.0, ty)?;
        // An uninhabited user type has no constructors, but unchecked coercions
        // can still produce its values (the reference `Free` over `Val` builds
        // `Return` values this way). Coverage must not treat it as a complete
        // empty signature, so its shape is left unknown and the wildcard
        // matrix handles it.
        if !self.0.newtype_ids.contains(&type_id)
            && crate::cc::layout::is_uninhabited_data_type(self.0, type_id)
        {
            return None;
        }
        Some(
            self.0
                .constructors
                .iter()
                .filter(|constructor| constructor.type_id == type_id)
                .map(|constructor| ConstructorRow {
                    symbol: constructor.symbol,
                    fields: constructor.field_types.clone(),
                })
                .collect(),
        )
    }

    fn record_fields(&self, ty: TypeId) -> Option<Vec<(String, TypeId)>> {
        self.0
            .record_fields(crate::cc::layout::unquantified_type(self.0, ty))
    }

    fn boolean_type(&self, ty: TypeId) -> bool {
        let ty = crate::cc::layout::unquantified_type(self.0, ty);
        matches!(
            self.0.types.get(ty.0 as usize),
            Some(Type::Constructor(TypeConstructor::Boolean))
        )
    }

    fn constructor_name(&self, symbol: psrs_hir::SymbolId) -> String {
        self.0
            .constructors
            .iter()
            .find(|constructor| constructor.symbol == symbol)
            .map_or_else(
                || format!("Constructor{}", symbol.index),
                |constructor| constructor.name.clone(),
            )
    }
}
