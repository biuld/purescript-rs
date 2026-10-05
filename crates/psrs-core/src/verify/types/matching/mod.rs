use super::error;
use crate::{Module, Type, TypeId, VerifyError};
use psrs_hir::ModuleId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod closure;
mod constructors;
mod evidence;
mod invariant;
pub(crate) use evidence::instantiation;
mod helpers;
mod rows;
pub(in crate::verify) use constructors::constructor_fields_match;
pub(crate) use helpers::equivalent_types;
use helpers::{collect_free_variables, is_record_type, same_type};
use rows::RowForm;

pub(in crate::verify) fn compatible(
    actual: TypeId,
    expected: TypeId,
    module: &Module,
    owner: ModuleId,
    span: TextRange,
    errors: &mut Vec<VerifyError>,
) {
    let mut matcher = TypeMatcher {
        module,
        flexible: HashSet::new(),
        replacements: HashMap::new(),
        row_forms: HashMap::new(),
        alpha: HashMap::new(),
        active: HashSet::new(),
    };
    if !matcher.subsumes(actual, expected, true) {
        errors.push(error(
            owner,
            span,
            "Core expression type is inconsistent with its context",
        ));
    }
}

/// Checks whether `instance` is a legal use of a declaration or local scheme.
/// Every quantified variable receives one consistent replacement for the full
/// type, while nested `ForAll` binders remain rigid where the type is consumed.
pub(in crate::verify) fn scheme_instance(
    scheme: TypeId,
    quantified: &[psrs_hir::TypeVariableId],
    instance: TypeId,
    module: &Module,
) -> bool {
    let mut matcher = TypeMatcher {
        module,
        flexible: quantified.iter().copied().collect(),
        replacements: HashMap::new(),
        row_forms: HashMap::new(),
        alpha: HashMap::new(),
        active: HashSet::new(),
    };
    matcher.subsumes(scheme, instance, true)
}

/// Checks an application after transparently instantiating leading type-level
/// quantifiers on its callee. The same substitutions must satisfy its argument
/// and result, which rejects inconsistent instances such as `Int -> Boolean`
/// for `forall a. a -> a`.
pub(in crate::verify) fn application_matches(
    function: TypeId,
    argument: TypeId,
    result: TypeId,
    module: &Module,
) -> bool {
    let mut matcher = TypeMatcher {
        module,
        flexible: HashSet::new(),
        replacements: HashMap::new(),
        row_forms: HashMap::new(),
        alpha: HashMap::new(),
        active: HashSet::new(),
    };
    let mut function_body = function;
    let mut seen = HashSet::new();
    while let Some((variables, body)) = crate::forall_parts(&module.types, function_body) {
        if !seen.insert(function_body) {
            return false;
        }
        matcher.flexible.extend(variables.iter().copied());
        function_body = body;
    }
    let Some((parameter, function_result)) = crate::arrow_parts(&module.types, function_body)
    else {
        return false;
    };
    matcher.subsumes(argument, parameter, true) && matcher.subsumes(function_result, result, true)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Relation {
    Subsumption,
    Invariant,
}

struct TypeMatcher<'a> {
    module: &'a Module,
    flexible: HashSet<psrs_hir::TypeVariableId>,
    replacements: HashMap<psrs_hir::TypeVariableId, TypeId>,
    row_forms: HashMap<psrs_hir::TypeVariableId, RowForm>,
    alpha: HashMap<psrs_hir::TypeVariableId, psrs_hir::TypeVariableId>,
    active: HashSet<(Relation, TypeId, TypeId)>,
}

impl TypeMatcher<'_> {
    /// Checks value subsumption: `actual` may be consumed wherever `expected`
    /// is required. Function parameters are contravariant, while immutable
    /// record fields are covariant. An actual universal may be instantiated;
    /// an expected universal remains rigid and therefore requires an actual
    /// universal with alpha-equivalent binders.
    fn subsumes(&mut self, actual: TypeId, expected: TypeId, instantiate: bool) -> bool {
        if !self
            .active
            .insert((Relation::Subsumption, actual, expected))
        {
            return true;
        }
        let (Some(actual_type), Some(expected_type)) = (
            self.module.types.get(actual.0 as usize),
            self.module.types.get(expected.0 as usize),
        ) else {
            self.active
                .remove(&(Relation::Subsumption, actual, expected));
            return false;
        };

        if let Type::Variable(variable) = expected_type
            && self.flexible.contains(variable)
        {
            let result = if matches!(actual_type, Type::Variable(actual) if actual == variable) {
                true
            } else {
                self.bind_flexible(*variable, actual)
            };
            self.active
                .remove(&(Relation::Subsumption, actual, expected));
            return result;
        }

        if matches!(expected_type, Type::ForAll { .. })
            && !matches!(actual_type, Type::ForAll { .. })
        {
            if let Type::Variable(variable) = actual_type
                && self.flexible.contains(variable)
            {
                let result = if let Some(previous) = self.replacements.get(variable) {
                    self.subsumes(*previous, expected, true)
                } else {
                    self.bind_flexible(*variable, expected)
                };
                self.active
                    .remove(&(Relation::Subsumption, actual, expected));
                return result;
            }
            let Type::ForAll { variables, body } = expected_type else {
                unreachable!("the expected type was checked above")
            };
            let was_flexible = variables
                .iter()
                .map(|variable| (*variable, self.flexible.remove(variable)))
                .collect::<Vec<_>>();
            let result = self.subsumes(actual, *body, instantiate);
            for (variable, was_flexible) in was_flexible {
                if was_flexible {
                    self.flexible.insert(variable);
                }
            }
            self.active
                .remove(&(Relation::Subsumption, actual, expected));
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
                    self.active
                        .remove(&(Relation::Subsumption, actual, expected));
                    return false;
                }
                for (actual, expected) in actual_variables.iter().zip(expected_variables) {
                    self.alpha.insert(*actual, *expected);
                }
                let result = self.subsumes(*actual_body, *expected_body, instantiate);
                for variable in actual_variables {
                    self.alpha.remove(variable);
                }
                self.active
                    .remove(&(Relation::Subsumption, actual, expected));
                return result;
            }
            if !instantiate {
                self.active
                    .remove(&(Relation::Subsumption, actual, expected));
                return false;
            }
            let added = actual_variables
                .iter()
                .copied()
                .filter(|variable| self.flexible.insert(*variable))
                .collect::<Vec<_>>();
            let result = self.subsumes(*actual_body, expected, true);
            for variable in added {
                self.flexible.remove(&variable);
                self.replacements.remove(&variable);
            }
            self.active
                .remove(&(Relation::Subsumption, actual, expected));
            return result;
        }
        if let Type::Variable(variable) = actual_type {
            let result = if let Type::Variable(other) = expected_type {
                if self.alpha.contains_key(variable)
                    || self.alpha.values().any(|bound| bound == variable)
                    || self.alpha.contains_key(other)
                    || self.alpha.values().any(|bound| bound == other)
                {
                    self.alpha_variables_match(*variable, *other)
                } else if self.flexible.contains(variable) {
                    self.bind_flexible(*variable, expected)
                } else if self.flexible.contains(other) {
                    self.bind_flexible(*other, actual)
                } else {
                    *variable == *other
                }
            } else if self.alpha.contains_key(variable)
                || self.alpha.values().any(|bound| bound == variable)
            {
                false
            } else if self.flexible.contains(variable) {
                self.bind_flexible(*variable, expected)
            } else {
                false
            };
            self.active
                .remove(&(Relation::Subsumption, actual, expected));
            return result;
        }
        if let Type::Variable(variable) = expected_type {
            let result = if self.alpha.values().any(|bound| bound == variable) {
                false
            } else if self.flexible.contains(variable) {
                self.bind_flexible(*variable, actual)
            } else {
                false
            };
            self.active
                .remove(&(Relation::Subsumption, actual, expected));
            return result;
        }

        if let Some(result) = self.subsumes_closure(actual, expected, instantiate) {
            self.active
                .remove(&(Relation::Subsumption, actual, expected));
            return result;
        }

        let result = match (actual_type, expected_type) {
            (Type::Constructor(left), Type::Constructor(right)) => left == right,
            (Type::Application(_, _), Type::Application(_, _)) => {
                if let (
                    Some((actual_parameter, actual_result)),
                    Some((expected_parameter, expected_result)),
                ) = (
                    crate::arrow_parts(&self.module.types, actual),
                    crate::arrow_parts(&self.module.types, expected),
                ) {
                    self.subsumes(expected_parameter, actual_parameter, true)
                        && self.subsumes(actual_result, expected_result, instantiate)
                } else if is_record_type(self.module, actual)
                    && is_record_type(self.module, expected)
                {
                    self.subsumes_record(actual, expected)
                } else {
                    // No variance metadata is carried for nominal and
                    // higher-kinded applications, so keep them invariant.
                    self.matches(actual, expected, false)
                }
            }
            (Type::RowEmpty, Type::RowEmpty) => true,
            (
                Type::RowExtend {
                    label: actual_label,
                    ty: actual_ty,
                    tail: actual_tail,
                },
                Type::RowExtend {
                    label: expected_label,
                    ty: expected_ty,
                    tail: expected_tail,
                },
            ) => {
                actual_label == expected_label
                    && self.subsumes(*actual_ty, *expected_ty, true)
                    && self.matches(*actual_tail, *expected_tail, false)
            }
            _ => false,
        };
        self.active
            .remove(&(Relation::Subsumption, actual, expected));
        result
    }

    fn alpha_variables_match(
        &self,
        left: psrs_hir::TypeVariableId,
        right: psrs_hir::TypeVariableId,
    ) -> bool {
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

    fn bind_flexible(&mut self, variable: psrs_hir::TypeVariableId, replacement: TypeId) -> bool {
        if let Some(form) = self.row_forms.get(&variable).cloned() {
            return self.row_form_matches(&form, replacement);
        }
        if let Some(previous) = self.replacements.get(&variable) {
            return same_type(*previous, replacement, self.module);
        }
        // A variable is identical to itself. That binding is not an occurrence
        // inside a larger type, and later uses still have to agree with it.
        if matches!(
            self.module.types.get(replacement.0 as usize),
            Some(Type::Variable(found)) if *found == variable
        ) {
            self.replacements.insert(variable, replacement);
            return true;
        }
        let mut free = HashSet::new();
        collect_free_variables(
            replacement,
            &self.module.types,
            &mut HashMap::new(),
            &mut HashSet::new(),
            &mut free,
        );
        if free.contains(&variable) {
            return false;
        }
        self.replacements.insert(variable, replacement);
        true
    }

    fn subsumes_record(&mut self, actual: TypeId, expected: TypeId) -> bool {
        self.relate_records(actual, expected, true)
    }
}
