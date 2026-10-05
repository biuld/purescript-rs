//! Constraint types, dictionary uses, and finalized THIR evidence.

mod finalize;
mod typing;

use super::super::*;

impl Checker {
    /// Makes the declaration's dictionary parameters available as given
    /// evidence while its body is checked.
    pub(in crate::typecheck) fn begin_givens(
        &mut self,
        constraints: &[ClassConstraint],
        parameters: &[(LocalId, InferType)],
    ) {
        self.scope.givens.clear();
        self.scope.given_rigid.clear();
        for (constraint, (id, _)) in constraints.iter().zip(parameters) {
            for argument in &constraint.arguments {
                let mut variables = HashSet::new();
                super::fundeps::collect_infer_variables(argument, &mut variables);
                for variable in variables {
                    if self.state.rigid.insert(variable) {
                        self.scope.given_rigid.push(variable);
                    }
                }
            }
            self.scope
                .givens
                .push((constraint.clone(), WantedSolution::Given(*id)));
        }
    }

    pub(in crate::typecheck) fn end_givens(&mut self) {
        self.scope.givens.clear();
        for variable in self.scope.given_rigid.drain(..) {
            self.state.rigid.remove(&variable);
        }
    }

    /// Structural equality of two resolved inference types. Distinct unsolved
    /// variables are not equal, so ambiguity never selects an instance.
    pub(in crate::typecheck) fn infer_types_equal(
        &self,
        left: &InferType,
        right: &InferType,
    ) -> bool {
        match (
            self.resolve_type(left.clone()),
            self.resolve_type(right.clone()),
        ) {
            (InferType::Variable(a), InferType::Variable(b)) => a == b,
            (InferType::Constructor(a), InferType::Constructor(b)) => a == b,
            (InferType::TypeLevelString(a), InferType::TypeLevelString(b)) => a == b,
            (InferType::TypeLevelInt(a), InferType::TypeLevelInt(b)) => a == b,
            (InferType::Application(f1, a1), InferType::Application(f2, a2)) => {
                self.infer_types_equal(&f1, &f2) && self.infer_types_equal(&a1, &a2)
            }
            (InferType::RowEmpty, InferType::RowEmpty) => true,
            (
                InferType::RowExtend {
                    label: l1,
                    ty: t1,
                    tail: r1,
                },
                InferType::RowExtend {
                    label: l2,
                    ty: t2,
                    tail: r2,
                },
            ) => l1 == l2 && self.infer_types_equal(&t1, &t2) && self.infer_types_equal(&r1, &r2),
            _ => false,
        }
    }

    pub(in crate::typecheck) fn display_constraint(
        &self,
        class_id: hir::TypeId,
        arguments: &[InferType],
    ) -> String {
        let name = self
            .env
            .type_names
            .get(&class_id)
            .cloned()
            .unwrap_or_else(|| "?".into());
        let arguments = arguments
            .iter()
            .map(|argument| self.display_type(argument))
            .collect::<Vec<_>>()
            .join(" ");
        if arguments.is_empty() {
            name
        } else {
            format!("{name} {arguments}")
        }
    }
}

pub(super) fn record_field_type(record: &InferType, wanted: &str) -> Option<InferType> {
    let mut row = super::super::record_row(record)?;
    loop {
        match row {
            InferType::RowExtend { label, ty, .. } if label == wanted => return Some(*ty),
            InferType::RowExtend { tail, .. } => row = *tail,
            InferType::RowEmpty => return None,
            _ => return None,
        }
    }
}
