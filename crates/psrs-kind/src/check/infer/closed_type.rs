//! Kind unknowns left inside a constructor field or a synonym body.
//!
//! Official `checkTypeQuantification` rejects a `forall`, a kind application,
//! or a constrained type that still contains an unknown there. Those positions
//! do not implicitly generalize it.

use super::*;
use crate::solve::substitute;

impl Checker<'_> {
    /// A fresh kind variable. Inside a closed type it is recorded so
    /// [`Self::report_closed_unknowns`] can see that it was never solved.
    pub(super) fn fresh_closed_unknown(&mut self) -> Kind {
        let kind = self.fresh();
        if self.in_closed_type
            && let Kind::Variable(variable) = kind
        {
            self.closed_unknowns.push(variable);
        }
        kind
    }

    /// Instantiates an inferred scheme's flexible parameter kinds, leaving the
    /// declaration being checked on its shared variables.
    pub(super) fn instantiate_inferred(&mut self, scheme: &KindScheme) -> Kind {
        let mut free = Vec::new();
        collect_kind_variables(&scheme.kind, &mut free);
        let mut mapping = HashMap::new();
        for variable in free {
            if self.kind_is_guarded(variable) || self.state.is_rigid(variable) {
                continue;
            }
            let Kind::Variable(resolved) = self.state.resolve(Kind::Variable(variable)) else {
                continue;
            };
            if resolved != variable {
                continue;
            }
            mapping.insert(variable, self.fresh_closed_unknown());
        }
        if mapping.is_empty() {
            scheme.kind.clone()
        } else {
            substitute(&scheme.kind, &mapping)
        }
    }

    /// Kind-checks `ty` as a constructor field or synonym body.
    pub(super) fn kind_of_closed_type(
        &mut self,
        ty: &hir::Type,
        scope: &mut HashMap<String, Kind>,
    ) -> Kind {
        self.in_closed_type = true;
        self.closed_unknowns.clear();
        let kind = self.kind_of_type(ty, scope);
        self.in_closed_type = false;
        kind
    }

    /// Reports `QuantificationCheckFailureInType` when a closed type still
    /// holds a kind unknown. A variable solved to a concrete kind, to this
    /// declaration's own parameter, or to a variable that appears in the
    /// declaration's kind is one generalization may keep. `forall a. f a -> g a`
    /// is the last of those: `a`'s kind is the parameter kind the synonym
    /// quantifies, not an unknown left inside the field.
    pub(super) fn report_closed_unknowns(&mut self, span: TextRange) {
        let mut generalized = Vec::new();
        if let Some(kind) = self
            .current_type
            .and_then(|id| self.schemes.get(&id).map(|scheme| scheme.kind.clone()))
        {
            collect_kind_variables(&self.state.resolve(kind), &mut generalized);
        }
        let unknowns = std::mem::take(&mut self.closed_unknowns);
        for variable in unknowns {
            let Kind::Variable(resolved) = self.state.resolve(Kind::Variable(variable)) else {
                continue;
            };
            if self.state.is_rigid(resolved)
                || self.kind_is_guarded(resolved)
                || generalized.contains(&resolved)
            {
                continue;
            }
            self.report(
                QUANTIFICATION_CHECK_FAILURE_IN_TYPE,
                span,
                "cannot unambiguously generalize kinds appearing in the elaborated type",
            );
            return;
        }
    }

    /// Reports `VisibleQuantificationCheckFailureInType` when a synonym's
    /// generalized kind still names a type variable.
    ///
    /// Official `checkVisibleTypeQuantification` collects `freeTypeVariables`
    /// of the generalized synonym kind. `(TProxy :: KProxy k)` makes that kind
    /// mention `k`. A kind unknown is a different error.
    pub(super) fn report_visible_type_quantification(&mut self, span: TextRange) {
        let Some(id) = self.current_type else {
            return;
        };
        let Some(kind) = self.schemes.get(&id).map(|scheme| scheme.kind.clone()) else {
            return;
        };
        let kind = self.state.resolve(kind);
        let Some(name) = first_type_variable(&kind) else {
            return;
        };
        self.report(
            VISIBLE_QUANTIFICATION_CHECK_FAILURE_IN_TYPE,
            span,
            format!("Visible dependent quantification of type variable {name} is not supported."),
        );
    }

    fn kind_is_guarded(&self, variable: u32) -> bool {
        self.guarded_kinds.iter().any(|guarded| {
            *guarded == variable
                || matches!(
                    self.state.resolve(Kind::Variable(*guarded)),
                    Kind::Variable(resolved) if resolved == variable
                )
        })
    }
}

/// The first type variable left in `kind`, if the generalized kind still
/// mentions one.
fn first_type_variable(kind: &Kind) -> Option<&str> {
    match kind {
        Kind::TypeVariable(name) => Some(name),
        Kind::App(function, argument) | Kind::Function(function, argument) => {
            first_type_variable(function).or_else(|| first_type_variable(argument))
        }
        Kind::RowExtend { field, tail, .. } => {
            first_type_variable(field).or_else(|| first_type_variable(tail))
        }
        Kind::ForAll { body, .. } => first_type_variable(body),
        Kind::Variable(_) | Kind::Builtin(_) | Kind::Named(_) | Kind::RowEmpty => None,
    }
}
