use crate::{Module, Type, TypeId, arrow_parts, record_row};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

mod rows;
use rows::RowForm;

pub(super) fn compatible(actual: TypeId, expected: TypeId, module: &Module) -> bool {
    Matcher::new(module).subsumes(actual, expected)
}

pub(super) fn scheme_instance(
    scheme: TypeId,
    quantified: &[TypeVariableId],
    instance: TypeId,
    module: &Module,
) -> bool {
    let mut matcher = Matcher::new(module);
    matcher.flexible.extend(quantified.iter().copied());
    matcher.subsumes(scheme, instance)
}

pub(super) fn application(
    function: TypeId,
    argument: TypeId,
    result: TypeId,
    module: &Module,
) -> bool {
    let mut matcher = Matcher::new(module);
    let mut function = function;
    let mut seen = HashSet::new();
    while let Some((variables, body)) = crate::forall_parts(&module.types, function) {
        if !seen.insert(function) {
            return false;
        }
        matcher.flexible.extend(variables.iter().copied());
        function = body;
    }
    let Some((parameter, function_result)) = arrow_parts(&module.types, function) else {
        return false;
    };
    matcher.subsumes(argument, parameter) && matcher.subsumes(function_result, result)
}

struct Matcher<'a> {
    module: &'a Module,
    flexible: HashSet<TypeVariableId>,
    replacements: HashMap<TypeVariableId, TypeId>,
    row_forms: HashMap<TypeVariableId, RowForm>,
    alpha: HashMap<TypeVariableId, TypeVariableId>,
    active: HashSet<(TypeId, TypeId)>,
}

impl<'a> Matcher<'a> {
    fn new(module: &'a Module) -> Self {
        Self {
            module,
            flexible: HashSet::new(),
            replacements: HashMap::new(),
            row_forms: HashMap::new(),
            alpha: HashMap::new(),
            active: HashSet::new(),
        }
    }

    fn subsumes(&mut self, actual: TypeId, expected: TypeId) -> bool {
        if !self.active.insert((actual, expected)) {
            return true;
        }
        let (Some(actual_type), Some(expected_type)) = (
            self.module.types.get(actual.0 as usize),
            self.module.types.get(expected.0 as usize),
        ) else {
            self.active.remove(&(actual, expected));
            return false;
        };
        if let Type::Variable(variable) = expected_type
            && self.flexible.contains(variable)
        {
            let result = self.bind(*variable, actual);
            self.active.remove(&(actual, expected));
            return result;
        }
        if matches!(expected_type, Type::ForAll { .. })
            && !matches!(actual_type, Type::ForAll { .. })
        {
            if let Type::Variable(variable) = actual_type
                && self.flexible.contains(variable)
            {
                let result = if let Some(previous) = self.replacements.get(variable) {
                    self.subsumes(*previous, expected)
                } else {
                    self.bind(*variable, expected)
                };
                self.active.remove(&(actual, expected));
                return result;
            }
            let Type::ForAll { body, variables } = expected_type else {
                unreachable!()
            };
            let saved = variables
                .iter()
                .map(|variable| (*variable, self.flexible.remove(variable)))
                .collect::<Vec<_>>();
            let result = self.subsumes(actual, *body);
            for (variable, was_flexible) in saved {
                if was_flexible {
                    self.flexible.insert(variable);
                }
            }
            self.active.remove(&(actual, expected));
            return result;
        }
        if let Type::ForAll {
            variables: actual_variables,
            body: actual_body,
        } = actual_type
        {
            if let Type::ForAll {
                variables: expected_variables,
                body: expected_body,
            } = expected_type
            {
                if actual_variables.len() != expected_variables.len()
                    || actual_variables
                        .iter()
                        .any(|variable| self.alpha.contains_key(variable))
                {
                    self.active.remove(&(actual, expected));
                    return false;
                }
                for (actual, expected) in actual_variables.iter().zip(expected_variables) {
                    self.alpha.insert(*actual, *expected);
                }
                let result = self.subsumes(*actual_body, *expected_body);
                for variable in actual_variables {
                    self.alpha.remove(variable);
                }
                self.active.remove(&(actual, expected));
                return result;
            }
            let added = actual_variables
                .iter()
                .copied()
                .filter(|variable| self.flexible.insert(*variable))
                .collect::<Vec<_>>();
            let result = self.subsumes(*actual_body, expected);
            for variable in added {
                self.flexible.remove(&variable);
                self.replacements.remove(&variable);
            }
            self.active.remove(&(actual, expected));
            return result;
        }
        if let (Type::Variable(left), Type::Variable(right)) = (actual_type, expected_type) {
            let result = if self.alpha.contains_key(left)
                || self.alpha.values().any(|bound| bound == left)
                || self.alpha.contains_key(right)
                || self.alpha.values().any(|bound| bound == right)
            {
                self.alpha_match(*left, *right)
            } else if self.flexible.contains(left) {
                self.bind(*left, expected)
            } else if self.flexible.contains(right) {
                self.bind(*right, actual)
            } else {
                left == right
            };
            self.active.remove(&(actual, expected));
            return result;
        }
        if let Type::Variable(variable) = actual_type {
            let result = if self.alpha.contains_key(variable)
                || self.alpha.values().any(|bound| bound == variable)
            {
                false
            } else if self.flexible.contains(variable) {
                self.bind(*variable, expected)
            } else {
                false
            };
            self.active.remove(&(actual, expected));
            return result;
        }
        if let Type::Variable(variable) = expected_type {
            let result = if self.alpha.contains_key(variable)
                || self.alpha.values().any(|bound| bound == variable)
            {
                false
            } else if self.flexible.contains(variable) {
                self.bind(*variable, actual)
            } else {
                false
            };
            self.active.remove(&(actual, expected));
            return result;
        }
        let result = match (actual_type, expected_type) {
            (Type::Constructor(left), Type::Constructor(right)) => left == right,
            (Type::Application(_, _), Type::Application(_, _)) => {
                if let (
                    Some((actual_parameter, actual_result)),
                    Some((expected_parameter, expected_result)),
                ) = (
                    arrow_parts(&self.module.types, actual),
                    arrow_parts(&self.module.types, expected),
                ) {
                    self.subsumes(expected_parameter, actual_parameter)
                        && self.subsumes(actual_result, expected_result)
                } else if record_row(&self.module.types, actual).is_some()
                    && record_row(&self.module.types, expected).is_some()
                {
                    self.record_subsumes(actual, expected)
                } else {
                    self.equal(actual, expected, &mut HashSet::new())
                }
            }
            (Type::RowEmpty, Type::RowEmpty) => true,
            // Two decided literals are equal exactly when their scalar
            // sequences, or their values, are equal. A literal is never
            // flexible, so it cannot be solved to anything else here.
            (Type::TypeLevelString(left), Type::TypeLevelString(right)) => left == right,
            (Type::TypeLevelInt(left), Type::TypeLevelInt(right)) => left == right,
            (
                Type::RowExtend {
                    label: left_label,
                    ty: left_ty,
                    tail: left_tail,
                },
                Type::RowExtend {
                    label: right_label,
                    ty: right_ty,
                    tail: right_tail,
                },
            ) => {
                left_label == right_label
                    && self.subsumes(*left_ty, *right_ty)
                    && self.equal(*left_tail, *right_tail, &mut HashSet::new())
            }
            _ => false,
        };
        self.active.remove(&(actual, expected));
        result
    }

    fn bind(&mut self, variable: TypeVariableId, replacement: TypeId) -> bool {
        if let Some(form) = self.row_forms.get(&variable).cloned() {
            return self.row_form_matches(&form, replacement);
        }
        if let Some(previous) = self.replacements.get(&variable) {
            self.equal(*previous, replacement, &mut HashSet::new())
        } else {
            if matches!(
                self.module.types.get(replacement.0 as usize),
                Some(Type::Variable(replacement_variable)) if *replacement_variable == variable
            ) {
                return true;
            }
            if self.free_variables(replacement).contains(&variable) {
                return false;
            }
            self.replacements.insert(variable, replacement);
            true
        }
    }

    fn alpha_match(&self, left: TypeVariableId, right: TypeVariableId) -> bool {
        if let Some(mapped) = self.alpha.get(&left) {
            return *mapped == right;
        }
        if let Some((source, _)) = self.alpha.iter().find(|(_, target)| **target == left) {
            return *source == right;
        }
        if self.alpha.contains_key(&right) || self.alpha.values().any(|target| *target == right) {
            return false;
        }
        left == right
    }

    fn record_subsumes(&mut self, actual: TypeId, expected: TypeId) -> bool {
        self.relate_records(actual, expected)
    }

    fn equal(
        &mut self,
        left_id: TypeId,
        right_id: TypeId,
        active: &mut HashSet<(TypeId, TypeId)>,
    ) -> bool {
        if !active.insert((left_id, right_id)) {
            return true;
        }
        let (Some(left_type), Some(right_type)) = (
            self.module.types.get(left_id.0 as usize),
            self.module.types.get(right_id.0 as usize),
        ) else {
            return false;
        };
        if let Type::Variable(variable) = left_type
            && let Some(replacement) = self.replacements.get(variable).copied()
            && replacement != left_id
        {
            let result = self.equal(replacement, right_id, active);
            active.remove(&(left_id, right_id));
            return result;
        }
        if let Type::Variable(variable) = right_type
            && let Some(replacement) = self.replacements.get(variable).copied()
            && replacement != right_id
        {
            let result = self.equal(left_id, replacement, active);
            active.remove(&(left_id, right_id));
            return result;
        }
        match (left_type, right_type) {
            (Type::Variable(left_variable), Type::Variable(right_variable)) => {
                if self.alpha_match(*left_variable, *right_variable) {
                    true
                } else if self.flexible.contains(left_variable) {
                    self.bind(*left_variable, right_id)
                } else if self.flexible.contains(right_variable) {
                    self.bind(*right_variable, left_id)
                } else {
                    false
                }
            }
            (Type::Variable(variable), _) if self.flexible.contains(variable) => {
                self.bind(*variable, right_id)
            }
            (_, Type::Variable(variable)) if self.flexible.contains(variable) => {
                self.bind(*variable, left_id)
            }
            (
                Type::ForAll {
                    variables: left_variables,
                    body: left_body,
                },
                Type::ForAll {
                    variables: right_variables,
                    body: right_body,
                },
            ) if left_variables.len() == right_variables.len() => {
                let mut alpha = self.alpha.clone();
                for (left, right) in left_variables.iter().zip(right_variables) {
                    alpha.insert(*left, *right);
                }
                equal_types(*left_body, *right_body, self.module, &alpha, active)
            }
            (Type::Constructor(left), Type::Constructor(right)) => left == right,
            (Type::Application(left_fn, left_arg), Type::Application(right_fn, right_arg)) => {
                self.equal(*left_fn, *right_fn, active) && self.equal(*left_arg, *right_arg, active)
            }
            (Type::RowEmpty, Type::RowEmpty) => true,
            (Type::TypeLevelString(left), Type::TypeLevelString(right)) => left == right,
            (Type::TypeLevelInt(left), Type::TypeLevelInt(right)) => left == right,
            (
                Type::RowExtend {
                    label: left_label,
                    ty: left_ty,
                    tail: left_tail,
                },
                Type::RowExtend {
                    label: right_label,
                    ty: right_ty,
                    tail: right_tail,
                },
            ) => {
                left_label == right_label
                    && self.equal(*left_ty, *right_ty, active)
                    && self.equal(*left_tail, *right_tail, active)
            }
            _ => false,
        }
    }

    fn free_variables(&self, id: TypeId) -> HashSet<TypeVariableId> {
        let mut free = HashSet::new();
        collect_free(
            id,
            &self.module.types,
            &mut HashMap::new(),
            &mut HashSet::new(),
            &mut free,
        );
        free
    }
}

fn equal_types(
    left: TypeId,
    right: TypeId,
    module: &Module,
    alpha: &HashMap<TypeVariableId, TypeVariableId>,
    active: &mut HashSet<(TypeId, TypeId)>,
) -> bool {
    if !active.insert((left, right)) {
        return true;
    }
    let (Some(left_type), Some(right_type)) = (
        module.types.get(left.0 as usize),
        module.types.get(right.0 as usize),
    ) else {
        return false;
    };
    match (left_type, right_type) {
        (Type::Variable(left), Type::Variable(right)) => {
            alpha.get(left).copied().unwrap_or(*left) == *right
        }
        (Type::Constructor(left), Type::Constructor(right)) => left == right,
        (Type::Application(lf, la), Type::Application(rf, ra)) => {
            equal_types(*lf, *rf, module, alpha, active)
                && equal_types(*la, *ra, module, alpha, active)
        }
        (
            Type::ForAll {
                variables: lv,
                body: lb,
            },
            Type::ForAll {
                variables: rv,
                body: rb,
            },
        ) if lv.len() == rv.len() => {
            let mut nested = alpha.clone();
            for (left, right) in lv.iter().zip(rv) {
                nested.insert(*left, *right);
            }
            equal_types(*lb, *rb, module, &nested, active)
        }
        (Type::RowEmpty, Type::RowEmpty) => true,
        (Type::TypeLevelString(left), Type::TypeLevelString(right)) => left == right,
        (Type::TypeLevelInt(left), Type::TypeLevelInt(right)) => left == right,
        (
            Type::RowExtend {
                label: ll,
                ty: lt,
                tail: ltail,
            },
            Type::RowExtend {
                label: rl,
                ty: rt,
                tail: rtail,
            },
        ) => {
            ll == rl
                && equal_types(*lt, *rt, module, alpha, active)
                && equal_types(*ltail, *rtail, module, alpha, active)
        }
        _ => false,
    }
}

fn collect_free(
    id: TypeId,
    types: &[Type],
    bound: &mut HashMap<TypeVariableId, usize>,
    active: &mut HashSet<TypeId>,
    free: &mut HashSet<TypeVariableId>,
) {
    if !active.insert(id) {
        return;
    }
    match types.get(id.0 as usize) {
        Some(Type::Variable(variable)) if !bound.contains_key(variable) => {
            free.insert(*variable);
        }
        Some(Type::Application(function, argument)) => {
            collect_free(*function, types, bound, active, free);
            collect_free(*argument, types, bound, active, free);
        }
        Some(Type::ForAll { variables, body }) => {
            for variable in variables {
                *bound.entry(*variable).or_default() += 1;
            }
            collect_free(*body, types, bound, active, free);
            for variable in variables {
                if let Some(count) = bound.get_mut(variable) {
                    *count -= 1;
                    if *count == 0 {
                        bound.remove(variable);
                    }
                }
            }
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            collect_free(*ty, types, bound, active, free);
            collect_free(*tail, types, bound, active, free);
        }
        _ => {}
    }
    active.remove(&id);
}
