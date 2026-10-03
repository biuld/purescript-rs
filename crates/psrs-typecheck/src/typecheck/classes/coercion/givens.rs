use super::super::super::*;
use super::flatten_infer_spine;

impl Checker {
    pub(super) fn given_coercible(&self, source: &InferType, target: &InferType) -> bool {
        let source = self.resolve_type(source.clone());
        let target = self.resolve_type(target.clone());
        let mut direct = Vec::new();
        let mut edges = Vec::new();
        let mut relations = Vec::new();
        for (given, _) in &self.scope.givens {
            if given.class_id != hir::TypeId::COERCIBLE || given.arguments.len() != 2 {
                continue;
            }
            let left = self.resolve_type(given.arguments[0].clone());
            let right = self.resolve_type(given.arguments[1].clone());
            add_edge(&mut edges, left.clone(), right.clone(), self);
            add_edge(&mut edges, right.clone(), left.clone(), self);
            direct.push((left.clone(), right.clone()));
            if let Some(canonical) = self.canonical_given(&left, &right) {
                push_relation(&mut relations, canonical, self);
            }
        }

        // A given can discharge its exact relation (in either direction), and
        // explicit given proofs compose transitively. Canonicality restricts
        // inert-set rewriting below; it does not invalidate a chain of assumed
        // coercion proofs.
        if direct.iter().any(|(left, right)| {
            (self.infer_types_equal(&source, left) && self.infer_types_equal(&target, right))
                || (self.infer_types_equal(&source, right) && self.infer_types_equal(&target, left))
        }) {
            return true;
        }

        // Canonical given constraints interact by rewriting a type variable
        // through the role-aware structure of another canonical given. This
        // reaches a fixed point like the upstream inert-set solver while
        // refusing non-canonical recursive equations such as a ~ D a.
        const MAX_INTERACTIONS: usize = 128;
        for _ in 0..MAX_INTERACTIONS {
            let snapshot = relations.clone();
            let mut changed = false;
            // Interact canonical constraints with a shared variable on the
            // left: Coercible a x and Coercible a y imply Coercible x y.
            for left_index in 0..snapshot.len() {
                for right_index in left_index + 1..snapshot.len() {
                    let (left_variable, left_type) = &snapshot[left_index];
                    let (right_variable, right_type) = &snapshot[right_index];
                    if matches!(left_variable, InferType::Variable(_))
                        && self.infer_types_equal(left_variable, right_variable)
                    {
                        let before = relations.len();
                        self.collect_canonical_relations(
                            left_type,
                            right_type,
                            &mut relations,
                            &mut HashSet::new(),
                        );
                        changed |= relations.len() != before;
                    }
                }
            }
            for (rewrite_variable, replacement) in &snapshot {
                let InferType::Variable(rewrite_variable) = rewrite_variable else {
                    continue;
                };
                for (left, right) in &snapshot {
                    let InferType::Variable(left_variable) = left else {
                        continue;
                    };
                    if rewrite_variable == left_variable {
                        continue;
                    }
                    let Some((rewritten, did_rewrite)) =
                        self.rewrite_given_type(right, *rewrite_variable, replacement)
                    else {
                        continue;
                    };
                    if !did_rewrite {
                        continue;
                    }
                    let Some(canonical) = self.canonical_given(left, &rewritten) else {
                        let before = relations.len();
                        self.collect_canonical_relations(
                            left,
                            &rewritten,
                            &mut relations,
                            &mut HashSet::new(),
                        );
                        changed |= relations.len() != before;
                        continue;
                    };
                    if push_relation(&mut relations, canonical.clone(), self) {
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        for (left, right) in relations {
            add_edge(&mut edges, left.clone(), right.clone(), self);
            add_edge(&mut edges, right.clone(), left.clone(), self);
        }
        let mut pending = vec![source];
        let mut visited = Vec::new();
        while let Some(current) = pending.pop() {
            if self.infer_types_equal(&current, &target) {
                return true;
            }
            if visited
                .iter()
                .any(|visited| self.infer_types_equal(visited, &current))
            {
                continue;
            }
            visited.push(current.clone());
            for (left, right) in &edges {
                if self.infer_types_equal(&current, left) {
                    pending.push(right.clone());
                }
            }
        }
        false
    }

    fn collect_canonical_relations(
        &self,
        left: &InferType,
        right: &InferType,
        relations: &mut Vec<(InferType, InferType)>,
        path: &mut HashSet<(String, String)>,
    ) {
        let left = self.resolve_type(left.clone());
        let right = self.resolve_type(right.clone());
        if let Some(canonical) = self.canonical_given(&left, &right) {
            push_relation(relations, canonical, self);
            return;
        }
        if self.infer_types_equal(&left, &right) {
            return;
        }
        let key = (format!("{left:?}"), format!("{right:?}"));
        if !path.insert(key.clone()) {
            return;
        }
        match (&left, &right) {
            (
                InferType::RowExtend {
                    label: left_label,
                    ty: left_field,
                    tail: left_tail,
                },
                InferType::RowExtend {
                    label: right_label,
                    ty: right_field,
                    tail: right_tail,
                },
            ) if left_label == right_label => {
                self.collect_canonical_relations(left_field, right_field, relations, path);
                self.collect_canonical_relations(left_tail, right_tail, relations, path);
            }
            (InferType::RowEmpty, InferType::RowEmpty) => {}
            _ => {
                let (left_head, left_arguments) = flatten_infer_spine(&left);
                let (right_head, right_arguments) = flatten_infer_spine(&right);
                let (
                    InferType::Constructor(left_constructor),
                    InferType::Constructor(right_constructor),
                ) = (left_head, right_head)
                else {
                    path.remove(&key);
                    return;
                };
                if left_constructor != right_constructor
                    || left_arguments.len() != right_arguments.len()
                {
                    path.remove(&key);
                    return;
                }
                if let TypeConstructor::User(id) = left_constructor
                    && self
                        .env
                        .type_declarations
                        .get(id)
                        .is_some_and(|declaration| {
                            declaration.kind == hir::TypeDeclarationKind::Newtype
                        })
                {
                    path.remove(&key);
                    return;
                }
                let roles = self.roles_for_constructor(*left_constructor, left_arguments.len());
                for ((left_argument, right_argument), role) in
                    left_arguments.iter().zip(&right_arguments).zip(roles)
                {
                    match role {
                        hir::Role::Nominal => {
                            if !self.infer_types_equal(left_argument, right_argument) {
                                path.remove(&key);
                                return;
                            }
                        }
                        hir::Role::Representational => self.collect_canonical_relations(
                            left_argument,
                            right_argument,
                            relations,
                            path,
                        ),
                        hir::Role::Phantom => {}
                    }
                }
            }
        }
        path.remove(&key);
    }

    fn canonical_given(
        &self,
        left: &InferType,
        right: &InferType,
    ) -> Option<(InferType, InferType)> {
        let left = self.resolve_type(left.clone());
        let right = self.resolve_type(right.clone());
        match (&left, &right) {
            (InferType::Variable(left), InferType::Variable(right)) if left > right => {
                self.canonical_given(&InferType::Variable(*right), &InferType::Variable(*left))
            }
            (InferType::Variable(variable), _) if self.state.rigid.contains(variable) => {
                (!occurs_in(*variable, &right)).then_some((left, right))
            }
            (_, InferType::Variable(variable)) if self.state.rigid.contains(variable) => {
                (!occurs_in(*variable, &left)).then_some((right, left))
            }
            _ => None,
        }
    }

    fn rewrite_given_type(
        &self,
        ty: &InferType,
        variable: u32,
        replacement: &InferType,
    ) -> Option<(InferType, bool)> {
        if occurs_in(variable, replacement) {
            return None;
        }
        let ty = self.resolve_type(ty.clone());
        Some(rewrite_type_by_role(&ty, variable, replacement, self))
    }
}

fn rebuild_infer_spine(head: InferType, arguments: Vec<InferType>) -> InferType {
    arguments.into_iter().fold(head, |function, argument| {
        InferType::Application(Box::new(function), Box::new(argument))
    })
}

fn push_relation(
    relations: &mut Vec<(InferType, InferType)>,
    relation: (InferType, InferType),
    checker: &Checker,
) -> bool {
    if relations.iter().any(|(left, right)| {
        checker.infer_types_equal(left, &relation.0)
            && checker.infer_types_equal(right, &relation.1)
    }) {
        false
    } else {
        relations.push(relation);
        true
    }
}

fn add_edge(
    edges: &mut Vec<(InferType, InferType)>,
    left: InferType,
    right: InferType,
    checker: &Checker,
) {
    if !edges.iter().any(|(existing_left, existing_right)| {
        checker.infer_types_equal(existing_left, &left)
            && checker.infer_types_equal(existing_right, &right)
    }) {
        edges.push((left, right));
    }
}

fn occurs_in(variable: u32, ty: &InferType) -> bool {
    match ty {
        InferType::Variable(other) => variable == *other,
        InferType::Application(function, argument) => {
            occurs_in(variable, function) || occurs_in(variable, argument)
        }
        InferType::RowExtend { ty, tail, .. } => {
            occurs_in(variable, ty) || occurs_in(variable, tail)
        }
        InferType::ForAll { variables, body } => {
            !variables.contains(&variable) && occurs_in(variable, body)
        }
        InferType::Constrained { constraints, body } => {
            constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
                .any(|argument| occurs_in(variable, argument))
                || occurs_in(variable, body)
        }
        InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => false,
    }
}

fn rewrite_type_by_role(
    ty: &InferType,
    variable: u32,
    replacement: &InferType,
    checker: &Checker,
) -> (InferType, bool) {
    match ty {
        InferType::Variable(current) if *current == variable => (replacement.clone(), true),
        InferType::Application(..) => {
            let (head, arguments) = flatten_infer_spine(ty);
            match head {
                // A higher-kinded variable used as the head of an application
                // is rewritten at the head only. The arguments are not
                // independently rewritten by canonical application rules.
                InferType::Variable(current) => {
                    let (head, changed) = if *current == variable {
                        (replacement.clone(), true)
                    } else {
                        ((*head).clone(), false)
                    };
                    (rebuild_infer_spine(head, arguments), changed)
                }
                InferType::Constructor(constructor) => {
                    let roles = checker.roles_for_constructor(*constructor, arguments.len());
                    let mut changed = false;
                    let arguments = arguments
                        .into_iter()
                        .enumerate()
                        .map(|(index, argument)| {
                            if roles.get(index).copied() == Some(hir::Role::Nominal) {
                                return argument;
                            }
                            let (rewritten, did_rewrite) =
                                rewrite_type_by_role(&argument, variable, replacement, checker);
                            changed |= did_rewrite;
                            rewritten
                        })
                        .collect();
                    (rebuild_infer_spine((*head).clone(), arguments), changed)
                }
                _ => (ty.clone(), false),
            }
        }
        InferType::RowExtend { label, ty, tail } => {
            let (rewritten_ty, changed_ty) =
                rewrite_type_by_role(ty, variable, replacement, checker);
            let (rewritten_tail, changed_tail) =
                rewrite_type_by_role(tail, variable, replacement, checker);
            (
                InferType::RowExtend {
                    label: label.clone(),
                    ty: Box::new(rewritten_ty),
                    tail: Box::new(rewritten_tail),
                },
                changed_ty || changed_tail,
            )
        }
        InferType::ForAll { variables, body } if !variables.contains(&variable) => {
            let (body, changed) = rewrite_type_by_role(body, variable, replacement, checker);
            (
                InferType::ForAll {
                    variables: variables.clone(),
                    body: Box::new(body),
                },
                changed,
            )
        }
        InferType::ForAll { .. } => (ty.clone(), false),
        InferType::Constrained { constraints, body } => {
            let mut changed = false;
            let constraints = constraints
                .iter()
                .map(|constraint| ClassConstraint {
                    arguments: constraint
                        .arguments
                        .iter()
                        .map(|argument| {
                            let (argument, did_change) =
                                rewrite_type_by_role(argument, variable, replacement, checker);
                            changed |= did_change;
                            argument
                        })
                        .collect(),
                    ..constraint.clone()
                })
                .collect();
            let (body, body_changed) = rewrite_type_by_role(body, variable, replacement, checker);
            (
                InferType::Constrained {
                    constraints,
                    body: Box::new(body),
                },
                changed || body_changed,
            )
        }
        InferType::Variable(_)
        | InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => (ty.clone(), false),
    }
}
