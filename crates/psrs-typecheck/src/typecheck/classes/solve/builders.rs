//! Building the constraint and dictionary types a selected solution implies.

use crate::typecheck::*;
impl Checker {
    /// Allocates an identity used by solved nested dictionaries to find their
    /// current wanted entry after generalization updates its solution.
    pub(in crate::typecheck) fn fresh_wanted_id(&mut self) -> u32 {
        let id = self.state.next_wanted_id;
        self.state.next_wanted_id += 1;
        id
    }

    /// Builds a solved constraint node for evidence elaboration.
    pub(in crate::typecheck) fn build_solution_constraint(
        &mut self,
        class_id: hir::TypeId,
        arguments: Vec<InferType>,
        span: TextRange,
        solution: WantedSolution,
    ) -> WantedConstraint {
        let dictionary_type = self.dictionary_type(&ClassConstraint {
            class_id,
            arguments: arguments.clone(),
            span,
        });
        WantedConstraint {
            id: self.fresh_wanted_id(),
            class_id,
            arguments,
            dictionary_type,
            span,
            report_span: self.scope.report_origin.unwrap_or(span),
            givens: self.scope.givens.clone(),
            solution: Some(solution),
        }
    }

    pub(in crate::typecheck) fn build_constraint(
        &mut self,
        class_id: hir::TypeId,
        arguments: Vec<InferType>,
        span: TextRange,
    ) -> WantedConstraint {
        let dictionary_type = self.dictionary_type(&ClassConstraint {
            class_id,
            arguments: arguments.clone(),
            span,
        });
        WantedConstraint {
            id: self.fresh_wanted_id(),
            class_id,
            arguments,
            dictionary_type,
            span,
            report_span: self.scope.report_origin.unwrap_or(span),
            givens: self.scope.givens.clone(),
            solution: None,
        }
    }

    /// The modules whose instances may prove a wanted `class_id arguments`.
    ///
    /// This mirrors the official compiler's dictionary lookup: it searches the
    /// current module, the module that declares the class, and the modules that
    /// declare the nominal types occurring in the constraint's type arguments.
    /// An instance declared in any other module is an orphan and is not visible
    /// from here, even if that module is in the program.
    pub(in crate::typecheck) fn instance_candidate_modules(
        &self,
        class_id: hir::TypeId,
        arguments: &[InferType],
    ) -> HashSet<hir::ModuleId> {
        let mut modules = HashSet::new();
        modules.insert(self.env.module_id);
        modules.insert(class_id.module);
        for argument in arguments {
            collect_user_type_modules(argument, &mut modules);
        }
        modules
    }
}

/// Collects the defining module of every user type constructor occurring in a
/// type. Builtin constructors (`Int`, `Array`, function, record, and `Effect`)
/// live in `Prim`, which declares no source instances here, so they contribute
/// no candidate module.
fn collect_user_type_modules(ty: &InferType, out: &mut HashSet<hir::ModuleId>) {
    match ty {
        InferType::Constructor(TypeConstructor::User(id)) => {
            out.insert(id.module);
        }
        InferType::Application(function, argument) => {
            collect_user_type_modules(function, out);
            collect_user_type_modules(argument, out);
        }
        InferType::RowExtend { ty, tail, .. } => {
            collect_user_type_modules(ty, out);
            collect_user_type_modules(tail, out);
        }
        InferType::ForAll { body, .. } => collect_user_type_modules(body, out),
        InferType::Constrained { constraints, body } => {
            for argument in constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
            {
                collect_user_type_modules(argument, out);
            }
            collect_user_type_modules(body, out);
        }
        InferType::Variable(_)
        | InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => {}
    }
}
