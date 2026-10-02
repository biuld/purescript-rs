use super::super::*;

impl Checker {
    /// Checks visible instance declarations for overlaps, excluding branches
    /// that share one ordered chain identity. This prevents a later wanted
    /// constraint from turning ordinary declaration order into dispatch.
    pub(super) fn validate_instance_overlaps(&mut self, module: &hir::Module) {
        let instances = self.instances.clone();
        for (index, left) in instances.iter().enumerate() {
            for right in instances.iter().skip(index + 1) {
                if left.class_id != right.class_id
                    || (left.symbol.module == right.symbol.module
                        && left.chain_id == right.chain_id)
                {
                    continue;
                }
                let class = &self.classes[&left.class_id];
                let covering_sets = covering_sets(class.parameters.len(), &class.fundeps);
                if instances_are_apart(&covering_sets, &left.head_arguments, &right.head_arguments)
                {
                    continue;
                }
                let mut candidate_modules = HashSet::from([module.id, left.class_id.module]);
                for argument in left.head_arguments.iter().chain(&right.head_arguments) {
                    collect_user_type_modules(argument, &mut candidate_modules);
                }
                if !candidate_modules.contains(&left.symbol.module)
                    || !candidate_modules.contains(&right.symbol.module)
                {
                    continue;
                }
                let class_name = self
                    .type_names
                    .get(&left.class_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Class{}", left.class_id.index));
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::OverlappingInstances,
                    module.span,
                    format!("overlapping instances of class `{class_name}` are visible here"),
                ));
            }
        }
    }
}

fn collect_user_type_modules(ty: &InferType, modules: &mut HashSet<hir::ModuleId>) {
    match ty {
        InferType::Constructor(TypeConstructor::User(id)) => {
            modules.insert(id.module);
        }
        InferType::Application(function, argument) => {
            collect_user_type_modules(function, modules);
            collect_user_type_modules(argument, modules);
        }
        InferType::RowExtend { ty, tail, .. } => {
            collect_user_type_modules(ty, modules);
            collect_user_type_modules(tail, modules);
        }
        InferType::ForAll { body, .. } => collect_user_type_modules(body, modules),
        InferType::Constrained { constraints, body } => {
            for argument in constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
            {
                collect_user_type_modules(argument, modules);
            }
            collect_user_type_modules(body, modules);
        }
        InferType::Variable(_)
        | InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => {}
    }
}

/// Computes the minimal sets of class arguments that determine all arguments
/// under the class's functional dependencies.
fn covering_sets(parameter_count: usize, fundeps: &[FundepInfo]) -> Vec<Vec<usize>> {
    if fundeps.is_empty() {
        return vec![(0..parameter_count).collect()];
    }
    let mut result: Vec<Vec<usize>> = Vec::new();
    enumerate_covering_sets(parameter_count, fundeps, 0, &mut Vec::new(), &mut result);
    result
}

fn enumerate_covering_sets(
    parameter_count: usize,
    fundeps: &[FundepInfo],
    next: usize,
    chosen: &mut Vec<usize>,
    result: &mut Vec<Vec<usize>>,
) {
    if dependency_closure(chosen, fundeps).len() == parameter_count {
        if !result.iter().any(|cover| is_subset(cover, chosen)) {
            result.push(chosen.clone());
        }
        return;
    }
    let mut possible = chosen.clone();
    possible.extend(next..parameter_count);
    if dependency_closure(&possible, fundeps).len() != parameter_count || next == parameter_count {
        return;
    }
    chosen.push(next);
    enumerate_covering_sets(parameter_count, fundeps, next + 1, chosen, result);
    chosen.pop();
    enumerate_covering_sets(parameter_count, fundeps, next + 1, chosen, result);
}

fn is_subset(left: &[usize], right: &[usize]) -> bool {
    left.iter().all(|index| right.contains(index))
}

fn dependency_closure(seed: &[usize], fundeps: &[FundepInfo]) -> HashSet<usize> {
    let mut closure = seed.iter().copied().collect::<HashSet<_>>();
    loop {
        let before = closure.len();
        for fundep in fundeps {
            if fundep
                .determining
                .iter()
                .all(|index| closure.contains(index))
            {
                closure.extend(fundep.determined.iter().copied());
            }
        }
        if closure.len() == before {
            return closure;
        }
    }
}

fn instances_are_apart(
    covering_sets: &[Vec<usize>],
    left: &[InferType],
    right: &[InferType],
) -> bool {
    !covering_sets.is_empty()
        && covering_sets.iter().all(|set| {
            set.iter().any(|&index| {
                left.get(index)
                    .zip(right.get(index))
                    .is_some_and(|(left, right)| type_heads_apart(left, right))
            })
        })
}

/// Instance heads are apart when at least one nominal constructor or closed
/// row shape cannot unify. A type variable can still match any shape.
fn type_heads_apart(left: &InferType, right: &InferType) -> bool {
    match (left, right) {
        (InferType::Variable(_), _) | (_, InferType::Variable(_)) => false,
        (InferType::Constructor(left), InferType::Constructor(right)) => left != right,
        (InferType::Application(lf, la), InferType::Application(rf, ra)) => {
            type_heads_apart(lf, rf) || type_heads_apart(la, ra)
        }
        (InferType::RowEmpty, InferType::RowEmpty) => false,
        (
            InferType::RowExtend {
                label: left_label,
                ty: left_ty,
                tail: left_tail,
            },
            InferType::RowExtend {
                label: right_label,
                ty: right_ty,
                tail: right_tail,
            },
        ) => {
            left_label != right_label
                || type_heads_apart(left_ty, right_ty)
                || type_heads_apart(left_tail, right_tail)
        }
        // Two decided literals that are equal are the same head; unequal ones
        // cannot unify.
        (InferType::TypeLevelString(l), InferType::TypeLevelString(r)) => l != r,
        (InferType::TypeLevelInt(l), InferType::TypeLevelInt(r)) => l != r,
        _ => true,
    }
}
