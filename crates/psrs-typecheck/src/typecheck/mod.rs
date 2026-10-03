//! Type inference and checked THIR construction for one module.
//!
//! Inference has three state owners with distinct lifetimes — the read-only
//! [`SemanticEnv`](state::SemanticEnv), the mutable
//! [`InferState`](state::InferState), and the lexical
//! [`Scope`](state::Scope) — and [`Checker`](state::Checker) owns all three.
//! `state` declares them and the operations that enter, leave, or roll each
//! one back; `unify` owns equality and the binding rule; `group` owns the
//! per-binding-group sequence of solve, retain, check, and generalize; `infer`
//! owns synthesis and expected-type propagation; `rank_n` owns subsumption and
//! quantified instantiation; `classes` owns the adjacent constraint solver; `prim`
//! owns the `Prim` rule table that constraint solving dispatches to; and `kind`
//! reads the kind of an inference type through the one kind solver.

use psrs_hir::{
    self as hir, ExternalKind, Intrinsic, LocalBinder, LocalId, SymbolId, TypeVariableId,
};
use psrs_kind::{CheckedKindEnv, Kind};
use psrs_span::TextRange;
use psrs_thir::{self as thir, Type, TypeId};
use std::collections::{HashMap, HashSet};

mod checked_exports;
mod error;
pub use error::{TypeCheckError, TypeCheckErrorKind};

/// Program-wide semantic inputs needed when checking a module.
#[derive(Clone, Copy)]
pub struct TypecheckContext<'a> {
    pub known_types: &'a [hir::TypeDeclaration],
    pub imported_instances: &'a [hir::InstanceDeclaration],
    pub module_names: &'a HashMap<hir::ModuleId, String>,
    pub checked_kinds: &'a CheckedKindEnv,
}

mod entry;

pub use entry::{
    typecheck_module, typecheck_module_with_checked_kinds,
    typecheck_module_with_checked_kinds_and_module_names, typecheck_module_with_imports,
    typecheck_module_with_imports_and_effect_context,
    typecheck_module_with_imports_and_effect_representation,
};

/// The level of the empty top-level environment. All declaration variables are
/// created at a higher level, so top-level generalization quantifies them.
const TOP_LEVEL: u32 = 0;

#[derive(Default)]
struct TypeInterner {
    values: Vec<Type>,
    ids: HashMap<Type, TypeId>,
}

impl TypeInterner {
    fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(id) = self.ids.get(&ty) {
            return *id;
        }
        let id = TypeId(self.values.len() as u32);
        self.values.push(ty.clone());
        self.ids.insert(ty, id);
        id
    }
}

fn occurs(variable: u32, ty: &InferType) -> bool {
    match ty {
        InferType::Variable(other) => variable == *other,
        InferType::Application(function, argument) => {
            occurs(variable, function) || occurs(variable, argument)
        }
        InferType::RowExtend { ty, tail, .. } => occurs(variable, ty) || occurs(variable, tail),
        InferType::ForAll { variables, body } => {
            !variables.contains(&variable) && occurs(variable, body)
        }
        InferType::Constrained { constraints, body } => {
            constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
                .any(|argument| occurs(variable, argument))
                || occurs(variable, body)
        }
        InferType::RowEmpty => false,
        InferType::Constructor(_) => false,
        // A type-level literal contains no variable, so it can never be the
        // type a variable is solved to recursively.
        InferType::TypeLevelString(_) | InferType::TypeLevelInt(_) => false,
    }
}

#[cfg(test)]
mod tests;

mod classes;
mod finalize;
mod generalize;
mod group;
mod infer;
mod kind;
mod order;
mod prim;
mod rank_n;
mod result;
mod rows;
mod signature;
mod state;
mod type_model;
mod unify;
mod vocabulary;

use result::*;
use state::*;
use type_model::*;
use vocabulary::*;
