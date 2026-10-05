//! Read-only instantiation evidence from the Core checking relation.

use crate::{Module, Type, TypeConstructor, TypeId};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

/// A successful declaration-scheme/use check. Borrowing the source module keeps
/// its type arena immutable for the lifetime of the evidence.
pub struct Instantiation<'a> {
    pub(crate) module: &'a Module,
    pub(crate) replacements: HashMap<TypeVariableId, TypeId>,
}

impl std::fmt::Debug for Instantiation<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Instantiation")
            .field("replacements", &self.replacements)
            .finish()
    }
}

impl Instantiation<'_> {
    /// Resolves an abstract constructor to its checked constructor application.
    /// Fixed arguments belong to the borrowed source arena. An unsolved head,
    /// row solution or substitution cycle is not a constructor identity.
    pub fn constructor(&self, variable: TypeVariableId) -> Option<(TypeConstructor, Vec<TypeId>)> {
        let mut current = *self.replacements.get(&variable)?;
        let mut arguments = Vec::new();
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(current) {
                return None;
            }
            match self.module.types.get(current.0 as usize)? {
                Type::Variable(variable) => current = *self.replacements.get(variable)?,
                Type::Application(function, argument) => {
                    arguments.push(*argument);
                    current = *function;
                }
                Type::Constructor(constructor) => {
                    arguments.reverse();
                    return Some((*constructor, arguments));
                }
                _ => return None,
            }
        }
    }
}
