use super::super::unify::substitute;
use super::super::*;

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
        let mut wanted = std::mem::take(&mut self.wanted);
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
                let found = self.solve_constraint(&constraint, 0);
                constraint.solution = found;
                if constraint.solution.is_none() {
                    let rendered =
                        self.display_constraint(constraint.class_id, &constraint.arguments);
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::NoInstance,
                        constraint.span,
                        format!("no instance for constraint {rendered}"),
                    ));
                }
            }
            solved.push(constraint);
        }
        self.wanted = solved;
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
        for (given, solution) in self.givens.clone() {
            if given.class_id == class_id && self.constraints_match(&given.arguments, &arguments) {
                return Some(solution);
            }
        }
        if let Some(solution) = self.superclass_solution(constraint, depth) {
            return Some(solution);
        }
        for instance in self.instances.clone() {
            if instance.class_id != class_id {
                continue;
            }
            let Some(mapping) = self.match_instance(&instance.head_arguments, &arguments) else {
                continue;
            };
            // A context-free instance is an ordinary top-level dictionary
            // value, preserving the direct `Global` selection form.
            if instance.context.is_empty() {
                return Some(WantedSolution::Global(instance.symbol));
            }
            if let Some(solution) = self.solve_instance(&instance, &mapping, constraint, depth) {
                return Some(solution);
            }
        }
        None
    }

    /// Derives a wanted superclass constraint from a given subclass dictionary
    /// and its superclass closure.
    fn superclass_solution(
        &mut self,
        wanted: &WantedConstraint,
        depth: usize,
    ) -> Option<WantedSolution> {
        for (given, solution) in self.givens.clone() {
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
        let class = self.classes.get(&base_class).cloned()?;
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
                && self.constraints_match(&arguments, &wanted.arguments)
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
            let solution = self.solve_constraint(&wanted, depth + 1)?;
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
            solution: None,
        }
    }

    /// One-way matches a wanted constraint's arguments against an instance
    /// head, binding the head's variables. A concrete head position requires a
    /// concrete wanted position so a wanted variable never grounds an
    /// instance.
    fn match_instance(
        &self,
        head: &[InferType],
        actual: &[InferType],
    ) -> Option<HashMap<u32, InferType>> {
        if head.len() != actual.len() {
            return None;
        }
        let mut mapping = HashMap::new();
        for (pattern, value) in head.iter().zip(actual) {
            if !self.match_type(pattern, value, &mut mapping) {
                return None;
            }
        }
        Some(mapping)
    }

    pub(super) fn match_type(
        &self,
        pattern: &InferType,
        value: &InferType,
        mapping: &mut HashMap<u32, InferType>,
    ) -> bool {
        let value = self.resolve_type(value.clone());
        match pattern {
            InferType::Variable(variable) => {
                if let Some(bound) = mapping.get(variable) {
                    self.infer_types_equal(bound, &value)
                } else {
                    mapping.insert(*variable, value);
                    true
                }
            }
            InferType::Constructor(constructor) => {
                matches!(&value, InferType::Constructor(other) if other == constructor)
            }
            InferType::Application(function, argument) => match value {
                InferType::Application(value_function, value_argument) => {
                    self.match_type(function, &value_function, mapping)
                        && self.match_type(argument, &value_argument, mapping)
                }
                _ => false,
            },
            InferType::RowEmpty => matches!(value, InferType::RowEmpty),
            InferType::RowExtend { label, ty, tail } => match value {
                InferType::RowExtend {
                    label: value_label,
                    ty: value_ty,
                    tail: value_tail,
                } if value_label == *label => {
                    self.match_type(ty, &value_ty, mapping)
                        && self.match_type(tail, &value_tail, mapping)
                }
                _ => false,
            },
        }
    }
}
