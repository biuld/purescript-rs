use super::super::*;

pub(super) type SelectedInstance = (InstanceInfo, HashMap<u32, InferType>);

/// Whether an instance head agrees with the currently known wanted types.
/// `Unknown` means equality is possible but not established.
#[derive(Clone, Debug)]
pub(super) enum HeadMatch {
    Match(HashMap<u32, InferType>),
    Apart,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchState {
    Match,
    Apart,
    Unknown,
}

impl MatchState {
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Apart, _) | (_, Self::Apart) => Self::Apart,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::Match,
        }
    }
}

impl Checker {
    /// Selects the first matching branch in each visible instance group.
    /// An unknown non-final branch blocks only its own chain; it is not an
    /// overlap with a unique match from an unrelated chain.
    pub(super) fn selected_instances(
        &self,
        class_id: hir::TypeId,
        arguments: &[InferType],
    ) -> Vec<SelectedInstance> {
        self.select_instance_groups(class_id, arguments)
    }

    /// Returns the first matching branch from each visible group. An unknown
    /// branch stops searching that group. A caller diagnoses overlap only
    /// when more than one unrelated group has a definite match.
    pub(super) fn select_instance_groups(
        &self,
        class_id: hir::TypeId,
        arguments: &[InferType],
    ) -> Vec<SelectedInstance> {
        let visible = self.instance_candidate_modules(class_id, arguments);
        let mut instances = self
            .env
            .instances
            .iter()
            .filter(|instance| {
                instance.class_id == class_id && visible.contains(&instance.symbol.module)
            })
            .cloned()
            .collect::<Vec<_>>();
        instances.sort_by_key(|instance| {
            (
                instance.symbol.module.0,
                instance.chain_id,
                instance.chain_position,
            )
        });

        let fundeps = self
            .env
            .classes
            .get(&class_id)
            .map(|class| class.fundeps.as_slice())
            .unwrap_or_default();
        let mut selected = Vec::new();
        let mut start = 0;
        while start < instances.len() {
            let key = (instances[start].symbol.module, instances[start].chain_id);
            let mut end = start + 1;
            while end < instances.len()
                && (instances[end].symbol.module, instances[end].chain_id) == key
            {
                end += 1;
            }
            for instance in &instances[start..end] {
                match self.match_instance_head(&instance.head_arguments, arguments, fundeps) {
                    HeadMatch::Match(mapping) => {
                        selected.push((instance.clone(), mapping));
                        break;
                    }
                    HeadMatch::Apart => continue,
                    // PureScript ignores Unknown on singleton and final
                    // branches; on a non-final branch it blocks only this
                    // chain. In either case it must not suppress a definite
                    // match from an unrelated chain.
                    HeadMatch::Unknown => break,
                }
            }
            start = end;
        }
        selected
    }

    /// Matches every class argument together, using the transitive closure of
    /// all functional dependencies to decide which unknown positions can be
    /// inferred. Independent parameters still participate in apartness.
    pub(super) fn match_instance_head(
        &self,
        head: &[InferType],
        actual: &[InferType],
        fundeps: &[FundepInfo],
    ) -> HeadMatch {
        if head.len() != actual.len()
            || fundeps.iter().any(|fundep| {
                fundep
                    .determining
                    .iter()
                    .chain(&fundep.determined)
                    .any(|index| *index >= head.len())
            })
        {
            return HeadMatch::Apart;
        }

        // Retain each argument's substitution separately until the FD closure
        // tells us which positions are independently observable.
        let mut matched = Vec::with_capacity(head.len());
        for (pattern, wanted) in head.iter().zip(actual) {
            let mut substitutions = HashMap::<u32, Vec<InferType>>::new();
            let state = self.match_type_state(pattern, wanted, &mut substitutions);
            matched.push((state, substitutions));
        }

        let mut covered = matched
            .iter()
            .enumerate()
            .filter_map(|(index, (state, _))| (*state == MatchState::Match).then_some(index))
            .collect::<HashSet<_>>();
        loop {
            let before = covered.len();
            for fundep in fundeps {
                if fundep
                    .determining
                    .iter()
                    .all(|index| covered.contains(index))
                {
                    covered.extend(fundep.determined.iter().copied());
                }
            }
            if covered.len() == before {
                break;
            }
        }

        if covered.len() != head.len() {
            return if matched.iter().any(|(state, _)| *state == MatchState::Apart) {
                HeadMatch::Apart
            } else {
                HeadMatch::Unknown
            };
        }

        let determined = fundeps
            .iter()
            .flat_map(|fundep| fundep.determined.iter().copied())
            .collect::<HashSet<_>>();
        let mut substitutions = HashMap::<u32, Vec<InferType>>::new();
        for (index, (_, position_substitutions)) in matched.into_iter().enumerate() {
            if determined.contains(&index) {
                continue;
            }
            for (variable, values) in position_substitutions {
                substitutions.entry(variable).or_default().extend(values);
            }
        }

        let mut mapping = HashMap::new();
        let mut state = MatchState::Match;
        for (variable, values) in substitutions {
            let Some(first) = values.first().cloned() else {
                continue;
            };
            for other in values.iter().skip(1) {
                state = state.combine(self.equal_type_state(&first, other));
                if state == MatchState::Apart {
                    return HeadMatch::Apart;
                }
            }
            mapping.insert(variable, first);
        }
        match state {
            MatchState::Match => HeadMatch::Match(mapping),
            MatchState::Apart => HeadMatch::Apart,
            MatchState::Unknown => HeadMatch::Unknown,
        }
    }

    fn match_type_state(
        &self,
        pattern: &InferType,
        actual: &InferType,
        mapping: &mut HashMap<u32, Vec<InferType>>,
    ) -> MatchState {
        let actual = self.resolve_type(actual.clone());
        match pattern {
            InferType::Variable(variable) => {
                mapping.entry(*variable).or_default().push(actual);
                MatchState::Match
            }
            InferType::Constructor(constructor) => match actual {
                InferType::Constructor(other) if *constructor == other => MatchState::Match,
                InferType::Variable(_) => MatchState::Unknown,
                _ => MatchState::Apart,
            },
            InferType::Application(function, argument) => match actual {
                InferType::Application(actual_function, actual_argument) => self
                    .match_type_state(function, &actual_function, mapping)
                    .combine(self.match_type_state(argument, &actual_argument, mapping)),
                InferType::Variable(_) => MatchState::Unknown,
                _ => MatchState::Apart,
            },
            InferType::RowEmpty => match actual {
                InferType::RowEmpty => MatchState::Match,
                InferType::Variable(_) => MatchState::Unknown,
                _ => MatchState::Apart,
            },
            InferType::RowExtend { label, ty, tail } => match actual {
                InferType::RowExtend {
                    label: actual_label,
                    ty: actual_ty,
                    tail: actual_tail,
                } if *label == actual_label => self
                    .match_type_state(ty, &actual_ty, mapping)
                    .combine(self.match_type_state(tail, &actual_tail, mapping)),
                InferType::Variable(_) => MatchState::Unknown,
                _ => MatchState::Apart,
            },
            InferType::ForAll { variables, body } => match actual {
                InferType::ForAll {
                    variables: actual_variables,
                    body: actual_body,
                } if variables.len() == actual_variables.len() => {
                    let mapping = actual_variables
                        .into_iter()
                        .zip(variables.iter().copied())
                        .map(|(actual, pattern)| (actual, InferType::Variable(pattern)))
                        .collect();
                    self.equal_type_state(
                        body,
                        &super::super::unify::substitute(&actual_body, &mapping),
                    )
                }
                InferType::Variable(_) => MatchState::Unknown,
                _ => MatchState::Apart,
            },
            InferType::Constrained { constraints, body } => match actual {
                InferType::Constrained {
                    constraints: actual_constraints,
                    body: actual_body,
                } if constraints.len() == actual_constraints.len() => {
                    let mut state = MatchState::Match;
                    for (pattern, actual) in constraints.iter().zip(&actual_constraints) {
                        if pattern.class_id != actual.class_id
                            || pattern.arguments.len() != actual.arguments.len()
                        {
                            return MatchState::Apart;
                        }
                        for (pattern, actual) in pattern.arguments.iter().zip(&actual.arguments) {
                            state = state.combine(self.match_type_state(pattern, actual, mapping));
                        }
                    }
                    state.combine(self.match_type_state(body, &actual_body, mapping))
                }
                InferType::Variable(_) => MatchState::Unknown,
                _ => MatchState::Apart,
            },
            // A literal is decided, so it matches only the equal literal and is
            // apart from every other known shape.
            InferType::TypeLevelString(_) | InferType::TypeLevelInt(_) => match actual {
                InferType::Variable(_) => MatchState::Unknown,
                actual if pattern == &actual => MatchState::Match,
                _ => MatchState::Apart,
            },
        }
    }

    fn equal_type_state(&self, left: &InferType, right: &InferType) -> MatchState {
        let left = self.resolve_type(left.clone());
        let right = self.resolve_type(right.clone());
        if left == right {
            return MatchState::Match;
        }
        match (&left, &right) {
            (InferType::Variable(variable), other) if contains_variable(other, *variable) => {
                MatchState::Apart
            }
            (other, InferType::Variable(variable)) if contains_variable(other, *variable) => {
                MatchState::Apart
            }
            (InferType::Variable(_), _) | (_, InferType::Variable(_)) => MatchState::Unknown,
            (InferType::Constructor(a), InferType::Constructor(b)) => {
                if a == b {
                    MatchState::Match
                } else {
                    MatchState::Apart
                }
            }
            (InferType::Application(lf, la), InferType::Application(rf, ra)) => self
                .equal_type_state(lf, rf)
                .combine(self.equal_type_state(la, ra)),
            (
                InferType::RowExtend {
                    label: ll,
                    ty: lt,
                    tail: ltail,
                },
                InferType::RowExtend {
                    label: rl,
                    ty: rt,
                    tail: rtail,
                },
            ) if ll == rl => self
                .equal_type_state(lt, rt)
                .combine(self.equal_type_state(ltail, rtail)),
            (
                InferType::ForAll {
                    variables: left_variables,
                    body: left_body,
                },
                InferType::ForAll {
                    variables: right_variables,
                    body: right_body,
                },
            ) if left_variables.len() == right_variables.len() => {
                let mapping = right_variables
                    .iter()
                    .copied()
                    .zip(left_variables.iter().copied())
                    .map(|(right, left)| (right, InferType::Variable(left)))
                    .collect();
                self.equal_type_state(
                    left_body,
                    &super::super::unify::substitute(right_body, &mapping),
                )
            }
            (
                InferType::Constrained {
                    constraints: left_constraints,
                    body: left_body,
                },
                InferType::Constrained {
                    constraints: right_constraints,
                    body: right_body,
                },
            ) if left_constraints.len() == right_constraints.len() => {
                let mut state = MatchState::Match;
                for (left, right) in left_constraints.iter().zip(right_constraints) {
                    if left.class_id != right.class_id
                        || left.arguments.len() != right.arguments.len()
                    {
                        return MatchState::Apart;
                    }
                    for (left, right) in left.arguments.iter().zip(&right.arguments) {
                        state = state.combine(self.equal_type_state(left, right));
                    }
                }
                state.combine(self.equal_type_state(left_body, right_body))
            }
            _ => MatchState::Apart,
        }
    }
}

fn contains_variable(ty: &InferType, variable: u32) -> bool {
    match ty {
        InferType::Variable(found) => *found == variable,
        InferType::Application(function, argument) => {
            contains_variable(function, variable) || contains_variable(argument, variable)
        }
        InferType::RowExtend { ty, tail, .. } => {
            contains_variable(ty, variable) || contains_variable(tail, variable)
        }
        InferType::ForAll { variables, body } => {
            !variables.contains(&variable) && contains_variable(body, variable)
        }
        InferType::Constrained { constraints, body } => {
            constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
                .any(|argument| contains_variable(argument, variable))
                || contains_variable(body, variable)
        }
        InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => false,
    }
}
