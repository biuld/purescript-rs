//! Invariant type matching. Flexible variables are solved from either side.
//! Constructor identity comes from an explicit binding, never from a closure's
//! parameter count.

use super::{Relation, TypeMatcher};
use crate::Type;

impl TypeMatcher<'_> {
    pub(super) fn matches(
        &mut self,
        source: crate::TypeId,
        target: crate::TypeId,
        instantiate: bool,
    ) -> bool {
        if !self.active.insert((Relation::Invariant, source, target)) {
            return true;
        }
        if let (
            Some((source_parameters, source_result)),
            Some((target_parameters, target_result)),
        ) = (
            crate::closure_parts(&self.module.types, source),
            crate::closure_parts(&self.module.types, target),
        ) {
            let source_parameters = source_parameters.to_vec();
            let target_parameters = target_parameters.to_vec();
            let result = source_parameters.len() == target_parameters.len()
                && source_parameters
                    .into_iter()
                    .zip(target_parameters)
                    .all(|(source, target)| self.matches(source, target, false))
                && self.matches(source_result, target_result, instantiate);
            self.active.remove(&(Relation::Invariant, source, target));
            return result;
        }
        let (Some(source_type), Some(target_type)) = (
            self.module.types.get(source.0 as usize),
            self.module.types.get(target.0 as usize),
        ) else {
            self.active.remove(&(Relation::Invariant, source, target));
            return false;
        };
        if let Type::Variable(variable) = source_type {
            let result = if self.alpha.contains_key(variable)
                || self.alpha.values().any(|bound| bound == variable)
                || matches!(target_type, Type::Variable(other) if self.alpha.contains_key(other) || self.alpha.values().any(|bound| bound == other))
            {
                matches!(target_type, Type::Variable(other) if self.alpha_variables_match(*variable, *other))
            } else if self.flexible.contains(variable) {
                if matches!(target_type, Type::ForAll { .. }) {
                    false
                } else {
                    self.bind_flexible(*variable, target)
                }
            } else {
                match target_type {
                    Type::Variable(actual) if self.flexible.contains(actual) => {
                        self.bind_flexible(*actual, source)
                    }
                    Type::Variable(actual) => actual == variable,
                    _ => false,
                }
            };
            self.active.remove(&(Relation::Invariant, source, target));
            return result;
        }
        if let Type::Variable(variable) = target_type {
            let result = !self.alpha.values().any(|bound| bound == variable)
                && self.flexible.contains(variable)
                && !matches!(source_type, Type::ForAll { .. })
                && self.bind_flexible(*variable, source);
            self.active.remove(&(Relation::Invariant, source, target));
            return result;
        }
        let result = match (source_type, target_type) {
            (
                Type::ForAll {
                    variables: source_variables,
                    body: source_body,
                },
                Type::ForAll {
                    variables: target_variables,
                    body: target_body,
                },
            ) if source_variables.len() == target_variables.len() => {
                if source_variables
                    .iter()
                    .any(|variable| self.alpha.contains_key(variable))
                {
                    false
                } else {
                    for (source, target) in source_variables.iter().zip(target_variables) {
                        self.alpha.insert(*source, *target);
                    }
                    let matches = self.matches(*source_body, *target_body, instantiate);
                    for variable in source_variables {
                        self.alpha.remove(variable);
                    }
                    matches
                }
            }
            (Type::ForAll { variables, body }, _) if instantiate => {
                let added = variables
                    .iter()
                    .copied()
                    .filter(|variable| self.flexible.insert(*variable))
                    .collect::<Vec<_>>();
                let matches = self.matches(*body, target, true);
                for variable in added {
                    self.flexible.remove(&variable);
                    self.replacements.remove(&variable);
                }
                matches
            }
            (Type::ForAll { .. }, _) | (_, Type::ForAll { .. }) => false,
            (Type::Constructor(left), Type::Constructor(right)) => left == right,
            (Type::Application(_, _), Type::Application(_, _)) => {
                if let (
                    Some((source_parameter, source_result)),
                    Some((target_parameter, target_result)),
                ) = (
                    crate::arrow_parts(&self.module.types, source),
                    crate::arrow_parts(&self.module.types, target),
                ) {
                    self.matches(source_parameter, target_parameter, false)
                        && self.matches(source_result, target_result, instantiate)
                } else if super::is_record_type(self.module, source)
                    && super::is_record_type(self.module, target)
                {
                    self.matches_record(source, target)
                } else {
                    let (
                        Type::Application(source_function, source_argument),
                        Type::Application(target_function, target_argument),
                    ) = (source_type, target_type)
                    else {
                        unreachable!()
                    };
                    self.matches(*source_function, *target_function, false)
                        && self.matches(*source_argument, *target_argument, false)
                }
            }
            (Type::RowEmpty, Type::RowEmpty) => true,
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
                    && self.matches(*left_ty, *right_ty, false)
                    && self.matches(*left_tail, *right_tail, false)
            }
            _ => false,
        };
        self.active.remove(&(Relation::Invariant, source, target));
        result
    }

    pub(super) fn matches_record(&mut self, source: crate::TypeId, target: crate::TypeId) -> bool {
        self.relate_records(source, target, false)
    }
}
