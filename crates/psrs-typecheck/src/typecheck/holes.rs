//! Typed holes.
//!
//! A hole is one inference variable with a source name. An expression hole has
//! kind `Type`, as `infer'` does for `Hole` in `TypeChecker/Types.hs`. A hole
//! written in a type is a fresh variable, as `replaceTypeWildcards` does for
//! `HoleWildcard`. The diagnostic is emitted after the module's substitution
//! is complete, so the reported type is the type the hole was solved to.

use super::*;
use psrs_kind::type_kind;

/// A hole recorded during elaboration or inference, before substitution.
#[derive(Clone, Debug)]
pub(super) struct TypedHole {
    pub(super) name: String,
    pub(super) span: TextRange,
    pub(super) variable: u32,
}

impl Checker {
    /// Elaborates a type hole `?name` to a fresh inference variable.
    pub(super) fn note_type_hole(&mut self, name: &str, span: TextRange) -> InferType {
        self.note_hole(name, span, false)
    }

    /// Infers an expression hole `?name` at kind `Type`.
    pub(super) fn note_expr_hole(&mut self, name: &str, span: TextRange) -> InferType {
        self.note_hole(name, span, true)
    }

    fn note_hole(&mut self, name: &str, span: TextRange, value: bool) -> InferType {
        let ty = self.fresh();
        if let InferType::Variable(id) = ty {
            if value {
                self.record_variable_kind(id, type_kind());
            }
            self.state.holes.push(TypedHole {
                name: name.to_owned(),
                span,
                variable: id,
            });
        }
        ty
    }

    /// Reports every hole still in the solver, using the solved type.
    pub(super) fn report_solved_holes(&mut self) {
        let holes = std::mem::take(&mut self.state.holes);
        for hole in holes {
            let solved = self.resolve_type(InferType::Variable(hole.variable));
            let rendered = self.display_type(&solved);
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::HoleInferredType,
                hole.span,
                format!("hole `?{}` has inferred type {rendered}", hole.name),
            ));
        }
    }
}
