use super::super::*;

impl Checker {
    pub(in crate::typecheck) fn reject_skolem_escape(
        &mut self,
        variable: u32,
        ty: &InferType,
        span: TextRange,
    ) -> bool {
        let variable_level = self
            .state
            .levels
            .get(&variable)
            .copied()
            .unwrap_or(TOP_LEVEL);
        fn visit(
            checker: &Checker,
            ty: &InferType,
            variable_level: u32,
            bound: &mut HashSet<u32>,
        ) -> Option<u32> {
            match ty {
                InferType::Variable(id)
                    if checker.state.rigid.contains(id)
                        && !bound.contains(id)
                        && checker.state.levels.get(id).copied().unwrap_or(TOP_LEVEL)
                            > variable_level =>
                {
                    Some(*id)
                }
                InferType::Application(function, argument) => {
                    visit(checker, function, variable_level, bound)
                        .or_else(|| visit(checker, argument, variable_level, bound))
                }
                InferType::ForAll { variables, body } => {
                    let newly_bound = variables
                        .iter()
                        .filter(|id| !bound.contains(id))
                        .copied()
                        .collect::<Vec<_>>();
                    bound.extend(newly_bound.iter().copied());
                    let found = visit(checker, body, variable_level, bound);
                    for id in newly_bound {
                        bound.remove(&id);
                    }
                    found
                }
                InferType::Constrained { constraints, body } => constraints
                    .iter()
                    .flat_map(|constraint| &constraint.arguments)
                    .find_map(|argument| visit(checker, argument, variable_level, bound))
                    .or_else(|| visit(checker, body, variable_level, bound)),
                InferType::RowExtend { ty, tail, .. } => visit(checker, ty, variable_level, bound)
                    .or_else(|| visit(checker, tail, variable_level, bound)),
                InferType::Variable(_)
                | InferType::Constructor(_)
                | InferType::RowEmpty
                | InferType::TypeLevelString(_)
                | InferType::TypeLevelInt(_) => None,
            }
        }
        let Some(skolem) = visit(self, ty, variable_level, &mut HashSet::new()) else {
            return false;
        };
        self.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::SkolemEscape,
            span,
            format!("rigid type variable _T{skolem} escapes its scope"),
        ));
        true
    }
}
