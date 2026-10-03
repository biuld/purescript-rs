use super::super::unify::substitute;
use super::super::*;
use super::fundeps::collect_infer_variables;

/// The maximum instance-context search depth. Recursive instances increase the
/// structure of the wanted types, so a finite bound terminates every search
/// and reports a bounded failure rather than looping.
const MAX_SOLVE_DEPTH: usize = 64;

impl Checker {
    /// Solves every unsolved wanted constraint against the current givens and
    /// the declared instances, reporting each unresolved constraint. Functional
    /// dependencies improve unknown types before the search (see `fundeps`).
    ///
    /// `wanted_start` is the index at which this declaration's constraints
    /// begin; earlier entries are already solved and retained for evidence.
    /// When `result` is supplied, the new constraints are also checked for
    /// ambiguity: every remaining type variable must be determined by the
    /// result type and the class functional dependencies.
    pub(in crate::typecheck) fn solve_wanted_constraints(
        &mut self,
        result: Option<&InferType>,
        wanted_start: usize,
    ) {
        let mut wanted = std::mem::take(&mut self.state.wanted);
        let start = wanted_start.min(wanted.len());
        for constraint in &mut wanted[start..] {
            constraint.arguments = constraint
                .arguments
                .iter()
                .map(|argument| self.resolve_type(argument.clone()))
                .collect::<Vec<_>>();
        }
        self.improve_wanted(&mut wanted[start..]);
        let mut solved = Vec::with_capacity(wanted.len());
        for mut constraint in wanted {
            if constraint.solution.is_none() {
                constraint.arguments = constraint
                    .arguments
                    .iter()
                    .map(|argument| self.resolve_type(argument.clone()))
                    .collect::<Vec<_>>();
                let errors_before = self.state.errors.len();
                let givens = constraint.givens.clone();
                let found = self
                    .with_given_chain(givens, |checker| checker.solve_constraint(&constraint, 0));
                constraint.solution = found;
                let reported_resolution_error =
                    self.state.errors[errors_before..].iter().any(|error| {
                        matches!(
                            error.kind,
                            TypeCheckErrorKind::OverlappingInstances
                                | TypeCheckErrorKind::NoInstance
                        )
                    });
                if constraint.solution.is_none() && !reported_resolution_error {
                    let rendered =
                        self.display_constraint(constraint.class_id, &constraint.arguments);
                    self.state.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::NoInstance,
                        constraint.span,
                        format!("no instance for constraint {rendered}"),
                    ));
                }
            }
            solved.push(constraint);
        }
        self.state.wanted = solved;
        if let Some(result) = result {
            self.check_ambiguity(result, start);
        }
    }

    /// Searches givens, then superclass projections, then instances for a
    /// dictionary proving `constraint`. An instance's context is solved
    /// recursively before the instance is selected.
    fn solve_constraint(
        &mut self,
        constraint: &WantedConstraint,
        depth: usize,
    ) -> Option<WantedSolution> {
        if depth > MAX_SOLVE_DEPTH {
            return None;
        }
        let class_id = constraint.class_id;
        let arguments = constraint.arguments.clone();
        if class_id == hir::TypeId::COERCIBLE {
            if arguments.len() == 2
                && (self.proves_coercible(&arguments[0], &arguments[1], constraint.span)
                    || self.superclass_solution(constraint, depth).is_some())
            {
                return Some(WantedSolution::Coercible {
                    source: arguments[0].clone(),
                    target: arguments[1].clone(),
                });
            }
            return None;
        }
        for (given, solution) in self.scope.givens.clone() {
            if given.class_id == class_id
                && self.constraint_arguments_match_or_unify(
                    &given.arguments,
                    &arguments,
                    constraint.span,
                )
            {
                return Some(solution);
            }
        }
        if let Some(solution) = self.superclass_solution(constraint, depth) {
            return Some(solution);
        }
        let mut selected = self.select_instance_groups(class_id, &arguments);

        if selected.len() > 1 {
            let rendered = self.display_constraint(class_id, &arguments);
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::OverlappingInstances,
                constraint.span,
                format!("overlapping instances for constraint {rendered}"),
            ));
            return None;
        }
        let (instance, mapping) = selected.pop()?;
        let errors_before = self.state.errors.len();
        let mapping = self.instantiate_selected_instance(&instance, &mapping, constraint);
        if self.state.errors.len() != errors_before {
            return None;
        }
        // Selection commits to the first matching head before solving its
        // context. A context failure cannot fall through to another branch.
        if instance.context.is_empty() {
            return Some(WantedSolution::Global(instance.symbol));
        }
        self.solve_instance(&instance, &mapping, constraint, depth)
    }

    /// Derives a wanted superclass constraint from a given subclass dictionary
    /// and its superclass closure.
    fn superclass_solution(
        &mut self,
        wanted: &WantedConstraint,
        depth: usize,
    ) -> Option<WantedSolution> {
        for (given, solution) in self.scope.givens.clone() {
            if let Some(found) =
                self.superclass_path(given.class_id, &given.arguments, solution, wanted, depth)
            {
                return Some(found);
            }
        }
        None
    }

    fn superclass_path(
        &mut self,
        base_class: hir::TypeId,
        base_arguments: &[InferType],
        base_solution: WantedSolution,
        wanted: &WantedConstraint,
        depth: usize,
    ) -> Option<WantedSolution> {
        if depth > MAX_SOLVE_DEPTH {
            return None;
        }
        let class = self.env.classes.get(&base_class).cloned()?;
        for superclass in &class.superclasses {
            let mut arguments = Vec::with_capacity(superclass.arguments.len());
            let mut valid = true;
            for name in &superclass.arguments {
                match class
                    .parameters
                    .iter()
                    .position(|parameter| parameter == name)
                {
                    Some(index) => arguments.push(base_arguments[index].clone()),
                    None => {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid {
                continue;
            }
            let parent = self.build_solution_constraint(
                base_class,
                base_arguments.to_vec(),
                wanted.span,
                base_solution.clone(),
            );
            let solution = WantedSolution::Superclass {
                parent: Box::new(parent),
                field: superclass.field.clone(),
            };
            if superclass.class_id == wanted.class_id
                && self.constraint_arguments_match_or_unify(
                    &arguments,
                    &wanted.arguments,
                    wanted.span,
                )
            {
                return Some(solution);
            }
            if let Some(found) =
                self.superclass_path(superclass.class_id, &arguments, solution, wanted, depth + 1)
            {
                return Some(found);
            }
        }
        None
    }

    /// Matches a wanted constraint against a given or projected superclass.
    /// Wanted type variables can be refined to the known argument types, but a
    /// failed candidate must leave no substitution, level, kind, or evidence
    /// behind. Its diagnostic is discarded, because a candidate that does not
    /// match is not itself an error.
    fn constraint_arguments_match_or_unify(
        &mut self,
        expected: &[InferType],
        actual: &[InferType],
        span: TextRange,
    ) -> bool {
        if expected.len() != actual.len() {
            return false;
        }
        self.speculate(|checker| {
            let errors_before = checker.state.errors.len();
            for (expected, actual) in expected.iter().zip(actual) {
                checker.unify(actual.clone(), expected.clone(), span);
            }
            (checker.state.errors.len() == errors_before).then_some(())
        })
        .is_some()
    }

    /// Solves one instance's context and, on success, returns the instance
    /// selection with the context dictionaries and the constructor's full
    /// arrow type.
    fn solve_instance(
        &mut self,
        instance: &InstanceInfo,
        mapping: &HashMap<u32, InferType>,
        constraint: &WantedConstraint,
        depth: usize,
    ) -> Option<WantedSolution> {
        let mut context = Vec::with_capacity(instance.context.len());
        for child in &instance.context {
            let arguments = child
                .arguments
                .iter()
                .map(|argument| self.resolve_type(substitute(argument, mapping)))
                .collect::<Vec<_>>();
            let mut wanted = self.build_constraint(child.class_id, arguments, child.span);
            let errors_before = self.state.errors.len();
            let Some(solution) = self.solve_constraint(&wanted, depth + 1) else {
                let has_nested_diagnostic =
                    self.state.errors[errors_before..].iter().any(|error| {
                        matches!(
                            error.kind,
                            TypeCheckErrorKind::NoInstance
                                | TypeCheckErrorKind::OverlappingInstances
                        )
                    });
                if !has_nested_diagnostic {
                    let rendered = self.display_constraint(child.class_id, &wanted.arguments);
                    self.state.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::NoInstance,
                        wanted.span,
                        format!("no instance for constraint {rendered}"),
                    ));
                }
                return None;
            };
            wanted.solution = Some(solution);
            context.push(wanted);
        }
        let result = self.dictionary_type(&ClassConstraint {
            class_id: constraint.class_id,
            arguments: constraint.arguments.clone(),
            span: constraint.span,
        });
        let mut constructor_type = result;
        for child in context.iter().rev() {
            constructor_type = arrow(child.dictionary_type.clone(), constructor_type);
        }
        Some(WantedSolution::Instance {
            constructor: instance.symbol,
            constructor_type,
            context,
        })
    }

    /// Freshens a selected instance for this particular use and unifies every
    /// head position with the wanted constraint. Functional dependencies may
    /// let matching choose a branch before all positions are known, so those
    /// positions must be connected to fresh instance variables before solving
    /// the instance context.
    fn instantiate_selected_instance(
        &mut self,
        instance: &InstanceInfo,
        mapping: &HashMap<u32, InferType>,
        constraint: &WantedConstraint,
    ) -> HashMap<u32, InferType> {
        let mut mapping = mapping.clone();
        let mut variables = HashSet::new();
        for argument in &instance.head_arguments {
            collect_infer_variables(argument, &mut variables);
        }
        for child in &instance.context {
            for argument in &child.arguments {
                collect_infer_variables(argument, &mut variables);
            }
        }
        for variable in variables {
            if let std::collections::hash_map::Entry::Vacant(entry) = mapping.entry(variable) {
                entry.insert(self.fresh());
            }
        }

        for (head, wanted) in instance.head_arguments.iter().zip(&constraint.arguments) {
            self.unify(substitute(head, &mapping), wanted.clone(), constraint.span);
        }
        mapping
            .into_iter()
            .map(|(variable, ty)| (variable, self.resolve_type(ty)))
            .collect()
    }

    /// Builds a solved constraint node for evidence elaboration.
    fn build_solution_constraint(
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
            class_id,
            arguments,
            dictionary_type,
            span,
            givens: self.scope.givens.clone(),
            solution: Some(solution),
        }
    }

    fn build_constraint(
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
            class_id,
            arguments,
            dictionary_type,
            span,
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
    pub(super) fn instance_candidate_modules(
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
