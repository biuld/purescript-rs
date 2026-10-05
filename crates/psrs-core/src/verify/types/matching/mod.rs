//! The one checked type relation.
//!
//! `TypeMatcher::relate` is the single entry point, parameterized by
//! [`Variance`]: `Subsumption` checks that an actual value type may be consumed
//! where an expected one is required, and `Invariant` checks rigid equality for
//! the positions where the type system allows no variance. Every public entry
//! point and every recursive step goes through it, so acceptance and the
//! substitution evidence it solves have one owner. This module is checking's
//! (P5/P6); no backend stage recomputes the relation.

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
    if !matcher.relate(actual, expected, Variance::Subsumption, true) {
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
    matcher.relate(scheme, instance, Variance::Subsumption, true)
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
    matcher.relate(argument, parameter, Variance::Subsumption, true)
        && matcher.relate(function_result, result, Variance::Subsumption, true)
}

/// The variance mode of the single checked type relation. `Subsumption` is the
/// value-level relation a declaration or local scheme is checked against at its
/// use: function parameters are contravariant, immutable record fields are
/// covariant, and an actual universal may be instantiated. `Invariant` is the
/// rigid mode used where no variance applies — nominal and higher-kinded
/// applications, closed and rigid rows, and constructor field templates.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Variance {
    Subsumption,
    Invariant,
}

struct TypeMatcher<'a> {
    module: &'a Module,
    flexible: HashSet<psrs_hir::TypeVariableId>,
    replacements: HashMap<psrs_hir::TypeVariableId, TypeId>,
    row_forms: HashMap<psrs_hir::TypeVariableId, RowForm>,
    alpha: HashMap<psrs_hir::TypeVariableId, psrs_hir::TypeVariableId>,
    active: HashSet<(Variance, TypeId, TypeId)>,
}

impl TypeMatcher<'_> {
    /// The one checked type relation. Every entry point and every recursive
    /// position goes through it; `variance` selects the mode. Checking is the
    /// sole owner of this relation, and its consumers read only its result.
    pub(super) fn relate(
        &mut self,
        actual: TypeId,
        expected: TypeId,
        variance: Variance,
        instantiate: bool,
    ) -> bool {
        // Literal identity is by value in every mode, independent of the
        // arena IDs assigned to occurrences of the same type-level literal.
        match (
            self.module.types.get(actual.0 as usize),
            self.module.types.get(expected.0 as usize),
        ) {
            (Some(Type::TypeLevelString(left)), Some(Type::TypeLevelString(right))) => {
                return left == right;
            }
            (Some(Type::TypeLevelInt(left)), Some(Type::TypeLevelInt(right))) => {
                return left == right;
            }
            _ => {}
        }
        match variance {
            Variance::Subsumption => self.subsumption(actual, expected, instantiate),
            Variance::Invariant => self.invariant(actual, expected, instantiate),
        }
    }

    /// Checks value subsumption: `actual` may be consumed wherever `expected`
    /// is required. Function parameters are contravariant, while immutable
    /// record fields are covariant. An actual universal may be instantiated;
    /// an expected universal remains rigid and therefore requires an actual
    /// universal with alpha-equivalent binders.
    fn subsumption(&mut self, actual: TypeId, expected: TypeId, instantiate: bool) -> bool {
        if !self
            .active
            .insert((Variance::Subsumption, actual, expected))
        {
            return true;
        }
        let (Some(actual_type), Some(expected_type)) = (
            self.module.types.get(actual.0 as usize),
            self.module.types.get(expected.0 as usize),
        ) else {
            self.active
                .remove(&(Variance::Subsumption, actual, expected));
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
                .remove(&(Variance::Subsumption, actual, expected));
            return result;
        }

        if matches!(expected_type, Type::ForAll { .. })
            && !matches!(actual_type, Type::ForAll { .. })
        {
            if let Type::Variable(variable) = actual_type
                && self.flexible.contains(variable)
            {
                let result = if let Some(previous) = self.replacements.get(variable) {
                    self.relate(*previous, expected, Variance::Subsumption, true)
                } else {
                    self.bind_flexible(*variable, expected)
                };
                self.active
                    .remove(&(Variance::Subsumption, actual, expected));
                return result;
            }
            let Type::ForAll { variables, body } = expected_type else {
                unreachable!("the expected type was checked above")
            };
            let was_flexible = variables
                .iter()
                .map(|variable| (*variable, self.flexible.remove(variable)))
                .collect::<Vec<_>>();
            let result = self.relate(actual, *body, Variance::Subsumption, instantiate);
            for (variable, was_flexible) in was_flexible {
                if was_flexible {
                    self.flexible.insert(variable);
                }
            }
            self.active
                .remove(&(Variance::Subsumption, actual, expected));
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
                        .remove(&(Variance::Subsumption, actual, expected));
                    return false;
                }
                for (actual, expected) in actual_variables.iter().zip(expected_variables) {
                    self.alpha.insert(*actual, *expected);
                }
                let result = self.relate(
                    *actual_body,
                    *expected_body,
                    Variance::Subsumption,
                    instantiate,
                );
                for variable in actual_variables {
                    self.alpha.remove(variable);
                }
                self.active
                    .remove(&(Variance::Subsumption, actual, expected));
                return result;
            }
            if !instantiate {
                self.active
                    .remove(&(Variance::Subsumption, actual, expected));
                return false;
            }
            let added = actual_variables
                .iter()
                .copied()
                .filter(|variable| self.flexible.insert(*variable))
                .collect::<Vec<_>>();
            let result = self.relate(*actual_body, expected, Variance::Subsumption, true);
            for variable in added {
                self.flexible.remove(&variable);
                self.replacements.remove(&variable);
            }
            self.active
                .remove(&(Variance::Subsumption, actual, expected));
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
                .remove(&(Variance::Subsumption, actual, expected));
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
                .remove(&(Variance::Subsumption, actual, expected));
            return result;
        }

        if let Some(result) = self.closure_subsumption(actual, expected, instantiate) {
            self.active
                .remove(&(Variance::Subsumption, actual, expected));
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
                    self.relate(
                        expected_parameter,
                        actual_parameter,
                        Variance::Subsumption,
                        true,
                    ) && self.relate(
                        actual_result,
                        expected_result,
                        Variance::Subsumption,
                        instantiate,
                    )
                } else if is_record_type(self.module, actual)
                    && is_record_type(self.module, expected)
                {
                    self.record_subsumption(actual, expected)
                } else {
                    // No variance metadata is carried for nominal and
                    // higher-kinded applications, so keep them invariant.
                    self.relate(actual, expected, Variance::Invariant, false)
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
                    && self.relate(*actual_ty, *expected_ty, Variance::Subsumption, true)
                    && self.relate(*actual_tail, *expected_tail, Variance::Invariant, false)
            }
            _ => false,
        };
        self.active
            .remove(&(Variance::Subsumption, actual, expected));
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

    fn record_subsumption(&mut self, actual: TypeId, expected: TypeId) -> bool {
        self.relate_records(actual, expected, Variance::Subsumption)
    }
}
