//! Type wildcards.
//!
//! A `_` in a type is one fresh inference variable, as `replaceTypeWildcards`
//! does for `UnnamedWildcard` in `TypeChecker/Unify.hs`. The warning is
//! emitted after the module's substitution is complete, so the reported type
//! is the type the wildcard was solved to. Official solving prints that as
//! `WildcardInferredType`.

use super::*;

impl Checker {
    /// Elaborates a type wildcard `_` and remembers its source range.
    pub(super) fn note_type_wildcard(&mut self, span: TextRange) -> InferType {
        match self.fresh() {
            InferType::Variable(id) => {
                self.state.wildcard_variables.insert(id, span);
                InferType::Variable(id)
            }
            other => other,
        }
    }

    /// Reports every type wildcard still in the solver, using the solved type.
    pub(super) fn report_inferred_wildcards(&mut self) {
        let wildcards = std::mem::take(&mut self.state.wildcard_variables);
        let mut ordered = wildcards.into_iter().collect::<Vec<_>>();
        ordered.sort_by_key(|(variable, span)| (span.start, *variable));
        for (variable, span) in ordered {
            let solved = self.resolve_type(InferType::Variable(variable));
            let rendered = self.display_type(&solved);
            self.state.warnings.push(TypeCheckWarning {
                span,
                message: format!("Wildcard type definition has the inferred type {rendered}"),
                code: "WildcardInferredType",
            });
        }
    }
}
