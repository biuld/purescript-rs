use super::super::unify::substitute;
use super::super::*;

/// Functional-dependency improvement and ambiguity checking for wanted
/// constraints. Improvement assigns only the positions a dependency determines
/// and only when the determining positions are already known; ambiguity rejects
/// a solved constraint whose remaining variables no dependency determines.
impl Checker {
    /// Applies class functional dependencies to every wanted constraint until
    /// no further type information is produced. Only determined positions are
    /// assigned; the determining positions must already be known.
    pub(super) fn improve_wanted(&mut self, wanted: &mut [WantedConstraint]) {
        loop {
            let mut changed = false;
            for constraint in wanted.iter_mut() {
                if self.improve_constraint(constraint) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Improves one constraint from the class's functional dependencies. Each
    /// dependency whose determining positions are all known contributes the
    /// determined positions of every givens or instance head that agrees on
    /// those determining positions. Returns whether any type was assigned.
    fn improve_constraint(&mut self, constraint: &mut WantedConstraint) -> bool {
        let Some(class) = self.env.classes.get(&constraint.class_id).cloned() else {
            return false;
        };
        if class.fundeps.is_empty() {
            return false;
        }
        let mut changed = false;
        for fundep in &class.fundeps {
            if !self.determiners_are_known(&constraint.arguments, &fundep.determining) {
                continue;
            }
            let mut sources: Vec<Vec<InferType>> = Vec::new();
            for (given, _) in constraint.givens.clone() {
                if given.class_id == constraint.class_id
                    && fundep.determining.iter().all(|&index| {
                        self.infer_types_equal(
                            &given.arguments[index],
                            &constraint.arguments[index],
                        )
                    })
                {
                    sources.push(given.arguments.clone());
                }
            }
            for (instance, mapping) in
                self.selected_instances(constraint.class_id, &constraint.arguments)
            {
                let mut instance_variables = HashSet::new();
                for argument in &instance.head_arguments {
                    collect_infer_variables(argument, &mut instance_variables);
                }
                let mapped_variables = mapping.keys().copied().collect::<HashSet<_>>();
                let unmapped_variables = instance_variables
                    .difference(&mapped_variables)
                    .copied()
                    .collect::<HashSet<_>>();
                let source = instance
                    .head_arguments
                    .iter()
                    .map(|argument| substitute(argument, &mapping))
                    .collect::<Vec<_>>();
                // A determined head variable absent from the matched
                // positions is not an improvement. Keep it as an
                // instance-local schema variable until branch selection
                // freshens and unifies the full head. Linking a wanted type to
                // this stored variable would leak one instance template into
                // every use; freshening here would create endless progress
                // across the improvement fixed-point loop.
                let source_uses_unmapped_variables = source.iter().any(|argument| {
                    let mut variables = HashSet::new();
                    collect_infer_variables(argument, &mut variables);
                    !variables.is_disjoint(&unmapped_variables)
                });
                if !source_uses_unmapped_variables {
                    sources.push(source);
                }
            }
            for &index in &fundep.determined {
                for source in &sources {
                    if self.assign_determined(
                        &constraint.arguments[index],
                        &source[index],
                        constraint.span,
                    ) {
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    /// Whether every determining parameter position holds a type that is not a
    /// bare, flexible inference variable.
    fn determiners_are_known(&self, arguments: &[InferType], determining: &[usize]) -> bool {
        determining.iter().all(|&index| {
            !matches!(
                self.resolve_type(arguments[index].clone()),
                InferType::Variable(variable) if !self.state.rigid.contains(&variable)
            )
        })
    }

    /// Assigns the determined position `wanted` from a fundep source, binding
    /// only flexible variables. A conflict between two known types is reported
    /// as a functional-dependency error.
    fn assign_determined(
        &mut self,
        wanted: &InferType,
        source: &InferType,
        span: TextRange,
    ) -> bool {
        let wanted = self.resolve_type(wanted.clone());
        let source = self.resolve_type(source.clone());
        if self.infer_types_equal(&wanted, &source) {
            return false;
        }
        match (&wanted, &source) {
            (InferType::Variable(variable), _) => {
                if self.state.rigid.contains(variable) {
                    return false;
                }
                self.bind_type_variable(*variable, source, span)
            }
            (InferType::Application(wf, wa), InferType::Application(sf, sa)) => {
                let mut changed = self.assign_determined(wf, sf, span);
                changed |= self.assign_determined(wa, sa, span);
                changed
            }
            (
                InferType::RowExtend {
                    label: wanted_label,
                    ty: wanted_ty,
                    tail: wanted_tail,
                },
                InferType::RowExtend {
                    label: source_label,
                    ty: source_ty,
                    tail: source_tail,
                },
            ) if wanted_label == source_label => {
                let mut changed = self.assign_determined(wanted_ty, source_ty, span);
                changed |= self.assign_determined(wanted_tail, source_tail, span);
                changed
            }
            _ => {
                // An underdetermined source cannot improve an already known
                // wanted type. Only a conflict between two fully known types is
                // a functional-dependency error.
                if !self.has_flexible_variable(&wanted) && !self.has_flexible_variable(&source) {
                    self.report_fundep_conflict(&wanted, &source, span);
                }
                false
            }
        }
    }

    fn has_flexible_variable(&self, ty: &InferType) -> bool {
        match self.resolve_type(ty.clone()) {
            InferType::Variable(variable) => !self.state.rigid.contains(&variable),
            InferType::Application(function, argument) => {
                self.has_flexible_variable(&function) || self.has_flexible_variable(&argument)
            }
            InferType::RowExtend { ty, tail, .. } => {
                self.has_flexible_variable(&ty) || self.has_flexible_variable(&tail)
            }
            InferType::ForAll { body, .. } => self.has_flexible_variable(&body),
            InferType::Constrained { constraints, body } => {
                constraints
                    .iter()
                    .flat_map(|constraint| &constraint.arguments)
                    .any(|argument| self.has_flexible_variable(argument))
                    || self.has_flexible_variable(&body)
            }
            InferType::Constructor(_)
            | InferType::RowEmpty
            | InferType::TypeLevelString(_)
            | InferType::TypeLevelInt(_) => false,
        }
    }

    fn report_fundep_conflict(&mut self, wanted: &InferType, source: &InferType, span: TextRange) {
        let wanted_display = self.display_type(wanted);
        let source_display = self.display_type(source);
        let message = format!(
            "functional dependency conflict: {wanted_display} is also determined to be {source_display}"
        );
        if self
            .state
            .reported_fundep_conflicts
            .insert((span, message.clone()))
        {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::FundepConflict,
                span,
                message,
            ));
        }
    }

    /// Rejects a solved constraint that still mentions a variable not
    /// determined by the result type or the class functional dependencies.
    /// Only constraints introduced by the current declaration (`start` onward)
    /// are considered; earlier entries were checked with their own result type.
    pub(super) fn check_ambiguity(&mut self, result: &InferType, start: usize) {
        let start = start.min(self.state.wanted.len());
        if start == self.state.wanted.len() {
            return;
        }
        let mut determined = HashSet::new();
        collect_infer_variables(&self.resolve_type(result.clone()), &mut determined);
        self.fundep_determined(&self.state.wanted[start..], &mut determined);
        for constraint in &self.state.wanted[start..] {
            if constraint.solution.is_none() {
                continue;
            }
            // A wanted discharged directly from a lexical dictionary is
            // already determined by that dictionary's scope. This commonly
            // occurs inside a rank-N method body, where the skolem appears in
            // the wanted but intentionally does not escape through the
            // method's result type. Do not apply this exemption to an
            // instance dictionary merely because one of its context
            // constraints comes from a given: the instance head itself may
            // still contain an ambiguous variable.
            if constraint
                .solution
                .as_ref()
                .is_some_and(solution_uses_lexical_given)
            {
                continue;
            }
            let mut variables = HashSet::new();
            for argument in &constraint.arguments {
                collect_infer_variables(&self.resolve_type(argument.clone()), &mut variables);
            }
            let ambiguous = variables
                .difference(&determined)
                .copied()
                .collect::<Vec<_>>();
            if ambiguous.is_empty() {
                continue;
            }
            let rendered = self.display_constraint(constraint.class_id, &constraint.arguments);
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::AmbiguousConstraint,
                constraint.span,
                format!(
                    "ambiguous constraint {rendered}: its type variables are not determined by the result type or a functional dependency"
                ),
            ));
        }
    }

    /// Adds, to `determined`, every variable the class functional dependencies
    /// determine across `constraints`, to a fixed point. A dependency
    /// contributes only when all of its determining positions are already
    /// determined, so the closure reaches what a chain of dependencies reaches
    /// and no further.
    ///
    /// This is the one closure both ambiguity checks run: the one over the
    /// constraints a declaration discharged and the one a retained constraint is
    /// measured against. They must be the same closure, or a variable one
    /// accepts and the other rejects would be the same variable.
    pub(super) fn fundep_determined(
        &self,
        constraints: &[WantedConstraint],
        determined: &mut HashSet<u32>,
    ) {
        loop {
            let mut changed = false;
            for constraint in constraints {
                let Some(class) = self.env.classes.get(&constraint.class_id) else {
                    continue;
                };
                if class.fundeps.is_empty() {
                    continue;
                }
                let argument_variables = constraint
                    .arguments
                    .iter()
                    .map(|argument| {
                        let mut variables = HashSet::new();
                        collect_infer_variables(
                            &self.resolve_type(argument.clone()),
                            &mut variables,
                        );
                        variables
                    })
                    .collect::<Vec<_>>();
                for fundep in &class.fundeps {
                    if fundep.determining.iter().all(|&index| {
                        argument_variables
                            .get(index)
                            .is_some_and(|variables| variables.is_subset(determined))
                    }) {
                        for &index in &fundep.determined {
                            if let Some(variables) = argument_variables.get(index) {
                                for variable in variables {
                                    if determined.insert(*variable) {
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Rejects a retained constraint whose variables the declaration's result
    /// type and the class functional dependencies do not determine.
    ///
    /// Generalizing such a constraint would quantify a variable nothing pins
    /// down: no use could instantiate it and no dictionary could be chosen for
    /// it. Official PureScript raises the same set as `AmbiguousTypeVariables`
    /// after taking the same dependency closure over the retained constraints,
    /// which is [`Self::fundep_determined`] here.
    pub(in crate::typecheck) fn check_residual_ambiguity(
        &mut self,
        residual: &[WantedConstraint],
        result: &InferType,
        declaration: &str,
        declaration_span: TextRange,
    ) {
        if residual.is_empty() {
            return;
        }
        let mut determined = HashSet::new();
        collect_infer_variables(&self.resolve_type(result.clone()), &mut determined);
        self.fundep_determined(residual, &mut determined);
        for constraint in residual {
            let mut variables = HashSet::new();
            for argument in &constraint.arguments {
                collect_infer_variables(&self.resolve_type(argument.clone()), &mut variables);
            }
            let ambiguous = variables
                .difference(&determined)
                .copied()
                .collect::<Vec<_>>();
            if ambiguous.is_empty() {
                continue;
            }
            let rendered = self.display_constraint(constraint.class_id, &constraint.arguments);
            let names = ambiguous
                .iter()
                .map(|variable| self.display_unknown(*variable))
                .collect::<Vec<_>>()
                .join(" ");
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::AmbiguousConstraint,
                declaration_span,
                format!(
                    "ambiguous constraint {rendered} in the type inferred for `{declaration}`: {names} is not determined by the result type or a functional dependency"
                ),
            ));
        }
    }

    /// A solver variable's name for a diagnostic: the source name a signature or
    /// a type hole gave it, and its identity otherwise.
    fn display_unknown(&self, variable: u32) -> String {
        self.scope
            .type_variable_names
            .get(&variable)
            .cloned()
            .unwrap_or_else(|| format!("_T{variable}"))
    }
}

fn solution_uses_lexical_given(solution: &WantedSolution) -> bool {
    match solution {
        WantedSolution::Given(_) => true,
        WantedSolution::Superclass { parent, .. } => parent
            .solution
            .as_ref()
            .is_some_and(solution_uses_lexical_given),
        // An abstracted dictionary is a parameter of the declaration itself, so
        // it determines nothing the result type and the dependencies do not.
        WantedSolution::Global(_)
        | WantedSolution::Instance { .. }
        | WantedSolution::Abstracted(_)
        | WantedSolution::Coercible { .. } => false,
    }
}

/// The inference variables used anywhere in a type.
pub(in crate::typecheck) fn collect_infer_variables(ty: &InferType, out: &mut HashSet<u32>) {
    match ty {
        InferType::Variable(variable) => {
            out.insert(*variable);
        }
        InferType::Application(function, argument) => {
            collect_infer_variables(function, out);
            collect_infer_variables(argument, out);
        }
        InferType::RowExtend { ty, tail, .. } => {
            collect_infer_variables(ty, out);
            collect_infer_variables(tail, out);
        }
        InferType::ForAll { variables, body } => {
            let mut nested = HashSet::new();
            collect_infer_variables(body, &mut nested);
            nested.retain(|variable| !variables.contains(variable));
            out.extend(nested);
        }
        InferType::Constrained { constraints, body } => {
            for argument in constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
            {
                collect_infer_variables(argument, out);
            }
            collect_infer_variables(body, out);
        }
        InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => {}
    }
}
