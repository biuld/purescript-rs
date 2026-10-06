//! Quantifiers needed by checked implementation types, including phantom
//! instantiations that do not occur in a declaration's public result type.

use super::*;

#[derive(Default)]
struct BodyVariables {
    candidates: Vec<u32>,
    bound: HashSet<u32>,
}

impl Checker {
    /// Extends a finished binding scheme with flexible variables occurring only
    /// in its implementation. Call after constraint solving and ambiguity checks;
    /// this supplies lexical binders, never evidence for an unsolved constraint.
    pub(in crate::typecheck) fn generalize_body(
        &mut self,
        scheme: Scheme,
        value: &InferredExpr,
        outer_level: u32,
    ) -> Scheme {
        let mut body_variables = BodyVariables::default();
        self.collect_body_variables(value, outer_level, &mut body_variables);
        body_variables.candidates.retain(|variable| {
            !body_variables.bound.contains(variable)
                && !self.state.rigid.contains(variable)
                && !self.state.generic_variables.contains(variable)
        });
        let mut variables = scheme.variables;
        variables.extend(body_variables.candidates);
        self.scheme(variables, scheme.constraints, scheme.ty)
    }

    fn collect_implementation_type(&self, ty: &InferType, level: u32, out: &mut BodyVariables) {
        let ty = self.resolve_type(ty.clone());
        self.collect_generalizable(&ty, level, &mut out.candidates);
        collect_structural_binders(&ty, &mut out.bound);
    }

    fn collect_body_variables(&self, value: &InferredExpr, level: u32, out: &mut BodyVariables) {
        let mut collect_type = |ty: &InferType| {
            self.collect_implementation_type(ty, level, out);
        };
        collect_type(&value.ty);
        match &value.kind {
            InferredExprKind::CoerceFunction {
                wanted,
                source,
                target,
            } => {
                collect_type(source);
                collect_type(target);
                self.collect_wanted_variables(*wanted, level, out, &mut HashSet::new());
            }
            InferredExprKind::UnsafeCoerceFunction { source, target, .. } => {
                collect_type(source);
                collect_type(target);
            }
            InferredExprKind::Lambda { binder, body } => {
                collect_type(&binder.scheme.ty);
                self.collect_body_variables(body, level, out);
            }
            InferredExprKind::Array(elements) => {
                for element in elements {
                    self.collect_body_variables(element, level, out);
                }
            }
            InferredExprKind::Record(fields) => {
                for (_, field) in fields {
                    self.collect_body_variables(field, level, out);
                }
            }
            InferredExprKind::RecordUpdate { expression, fields } => {
                self.collect_body_variables(expression, level, out);
                for (_, field) in fields {
                    self.collect_body_variables(field, level, out);
                }
            }
            InferredExprKind::DictionaryApplication { function, wanted } => {
                self.collect_body_variables(function, level, out);
                self.collect_wanted_variables(*wanted, level, out, &mut HashSet::new());
            }
            InferredExprKind::FieldAccess { expression, .. } => {
                self.collect_body_variables(expression, level, out);
            }
            InferredExprKind::Application(function, argument) => {
                self.collect_body_variables(function, level, out);
                self.collect_body_variables(argument, level, out);
            }
            InferredExprKind::Let { bindings, body } => {
                for binding in bindings {
                    self.collect_implementation_type(&binding.binder.scheme.ty, level, out);
                    self.collect_body_variables(&binding.value, level, out);
                }
                self.collect_body_variables(body, level, out);
            }
            InferredExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                for child in [condition, then_branch, else_branch] {
                    self.collect_body_variables(child, level, out);
                }
            }
            InferredExprKind::Case {
                scrutinee,
                branches,
            } => {
                self.collect_body_variables(scrutinee, level, out);
                for branch in branches {
                    self.collect_pattern_variables(&branch.pattern, level, out);
                    self.collect_body_variables(&branch.value, level, out);
                }
            }
            InferredExprKind::Local(_)
            | InferredExprKind::Global(_)
            | InferredExprKind::Integer(_)
            | InferredExprKind::Number(_)
            | InferredExprKind::Boolean(_)
            | InferredExprKind::String(_)
            | InferredExprKind::Char(_) => {}
            InferredExprKind::Method { wanted, .. } | InferredExprKind::Evidence(wanted) => {
                self.collect_wanted_variables(*wanted, level, out, &mut HashSet::new());
            }
        }
    }

    fn collect_wanted_variables(
        &self,
        index: usize,
        level: u32,
        out: &mut BodyVariables,
        visited: &mut HashSet<u32>,
    ) {
        if let Some(wanted) = self.state.wanted.get(index) {
            self.collect_evidence_variables(wanted, level, out, visited);
        }
    }

    fn collect_evidence_variables(
        &self,
        wanted: &WantedConstraint,
        level: u32,
        out: &mut BodyVariables,
        visited: &mut HashSet<u32>,
    ) {
        let Some(solution) = &wanted.solution else {
            return;
        };
        if !visited.insert(wanted.id) {
            return;
        }
        for ty in std::iter::once(&wanted.dictionary_type).chain(&wanted.arguments) {
            self.collect_implementation_type(ty, level, out);
        }
        match solution {
            WantedSolution::Instance {
                constructor_type,
                context,
                ..
            } => {
                self.collect_implementation_type(constructor_type, level, out);
                for id in context {
                    if let Some(child) = self.state.wanted.iter().find(|wanted| wanted.id == *id) {
                        self.collect_evidence_variables(child, level, out, visited);
                    }
                }
            }
            WantedSolution::Superclass { parent, .. } => {
                self.collect_evidence_variables(parent, level, out, visited);
            }
            WantedSolution::Coercible { source, target } => {
                for ty in [source, target] {
                    self.collect_implementation_type(ty, level, out);
                }
            }
            WantedSolution::Primitive { arguments } => {
                for ty in arguments {
                    self.collect_implementation_type(ty, level, out);
                }
            }
            WantedSolution::Given(_)
            | WantedSolution::Abstracted(_)
            | WantedSolution::Global(_) => {}
        }
    }

    fn collect_pattern_variables(
        &self,
        pattern: &InferredPattern,
        level: u32,
        out: &mut BodyVariables,
    ) {
        self.collect_implementation_type(&pattern.ty, level, out);
        match &pattern.kind {
            InferredPatternKind::Array { elements }
            | InferredPatternKind::Constructor {
                arguments: elements,
                ..
            } => {
                for element in elements {
                    self.collect_pattern_variables(element, level, out);
                }
            }
            InferredPatternKind::Named { pattern, .. } => {
                self.collect_pattern_variables(pattern, level, out);
            }
            InferredPatternKind::Record { fields } => {
                for (_, field) in fields {
                    self.collect_pattern_variables(field, level, out);
                }
            }
            InferredPatternKind::Var { ty, .. } => {
                self.collect_implementation_type(ty, level, out);
            }
            InferredPatternKind::Wildcard | InferredPatternKind::Literal { .. } => {}
        }
    }
}

// Binder identity belongs to the checked type, even after the scope that
// skolemized it has ended. Never recapture such a binder from a child node.
fn collect_structural_binders(ty: &InferType, out: &mut HashSet<u32>) {
    match ty {
        InferType::ForAll { variables, body } => {
            out.extend(variables);
            collect_structural_binders(body, out);
        }
        InferType::Application(function, argument) => {
            collect_structural_binders(function, out);
            collect_structural_binders(argument, out);
        }
        InferType::Constrained { constraints, body } => {
            for argument in constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
            {
                collect_structural_binders(argument, out);
            }
            collect_structural_binders(body, out);
        }
        InferType::RowExtend { ty, tail, .. } => {
            collect_structural_binders(ty, out);
            collect_structural_binders(tail, out);
        }
        InferType::Variable(_)
        | InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelInt(_)
        | InferType::TypeLevelString(_) => {}
    }
}
