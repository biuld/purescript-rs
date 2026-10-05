use super::*;

pub(super) fn solution_uses_lexical_given(
    solution: &WantedSolution,
    solutions: &HashMap<u32, WantedSolution>,
    visited: &mut HashSet<u32>,
) -> bool {
    match solution {
        WantedSolution::Given(_) => true,
        WantedSolution::Superclass { parent, .. } => parent
            .solution
            .as_ref()
            .is_some_and(|solution| solution_uses_lexical_given(solution, solutions, visited)),
        WantedSolution::Instance { context, .. } => context.iter().any(|id| {
            if !visited.insert(*id) {
                return false;
            }
            let uses_given = solutions
                .get(id)
                .is_some_and(|solution| solution_uses_lexical_given(solution, solutions, visited));
            visited.remove(id);
            uses_given
        }),
        // An abstracted dictionary is a parameter of the declaration itself, so
        // it determines nothing the result type and the dependencies do not.
        WantedSolution::Global(_)
        | WantedSolution::Abstracted(_)
        | WantedSolution::Coercible { .. }
        | WantedSolution::Primitive { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_hir::{LocalId, ModuleId, SymbolId};

    #[test]
    fn instance_context_using_a_given_determines_its_rank_n_wanted() {
        let selected = WantedSolution::Instance {
            constructor: SymbolId::new(ModuleId(0), 1),
            constructor_type: InferType::Variable(0),
            context: vec![7],
        };
        let solutions = HashMap::from([(7, WantedSolution::Given(LocalId(2)))]);

        assert!(solution_uses_lexical_given(
            &selected,
            &solutions,
            &mut HashSet::new()
        ));
    }

    #[test]
    fn context_free_instance_does_not_determine_its_wanted_variables() {
        let selected = WantedSolution::Instance {
            constructor: SymbolId::new(ModuleId(0), 1),
            constructor_type: InferType::Variable(0),
            context: Vec::new(),
        };

        assert!(!solution_uses_lexical_given(
            &selected,
            &HashMap::new(),
            &mut HashSet::new()
        ));
    }
}
