//! Read-only instantiation evidence and checked materialization.
mod local_rows;
pub(crate) mod substitution;
pub use local_rows::instantiate_local_rows;

use crate::{Module, Type, TypeConstructor, TypeId};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

/// A successful declaration-scheme/use check. Borrowing the source module keeps
/// its type arena immutable for the lifetime of the evidence.
pub struct Instantiation<'a> {
    pub(crate) module: &'a Module,
    pub(crate) replacements: HashMap<TypeVariableId, TypeId>,
    pub(crate) rows: HashMap<TypeVariableId, RowInstantiation>,
}

/// A checked row residual, including solutions with no existing arena node.
/// Field and tail ids refer to the immutable source module of the evidence.
#[derive(Clone, Debug)]
pub struct RowInstantiation {
    pub fields: Vec<(String, TypeId)>,
    pub tail: Option<TypeId>,
}

impl std::fmt::Debug for Instantiation<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Instantiation")
            .field("replacements", &self.replacements)
            .field("rows", &self.rows)
            .finish()
    }
}

impl Instantiation<'_> {
    /// Returns the checking owner's solution for a quantified row variable.
    /// A residual is retained even when its fields have no arena row node.
    pub fn row(&self, variable: TypeVariableId) -> Option<RowInstantiation> {
        if let Some(row) = self.rows.get(&variable) {
            return Some(row.clone());
        }
        let mut current = *self.replacements.get(&variable)?;
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(current) {
                return None;
            }
            if let Some(Type::Variable(variable)) = self.module.types.get(current.0 as usize) {
                if let Some(row) = self.rows.get(variable) {
                    return Some(row.clone());
                }
                current = *self.replacements.get(variable)?;
            } else {
                let (fields, tail) = crate::row_fields(&self.module.types, current)?;
                return Some(RowInstantiation { fields, tail });
            }
        }
    }

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
