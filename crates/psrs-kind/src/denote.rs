//! The one translation from a type expression to a kind.

use crate::kind::{Kind, flatten_spine, type_kind};
use crate::solve::KindState;
use psrs_hir::{self as hir, BuiltinType, TypeKind};
use std::collections::HashMap;

/// The kinds bound by name while one type expression is read, together with the
/// solver that allocates a kind for whatever the expression leaves
/// undetermined.
///
/// A binder's kind is bound for the rest of the expression it introduces, which
/// is how a `forall` binder and a declaration's own parameters reach the kinds
/// their body is read at.
pub struct KindScope<'a> {
    bound: HashMap<String, Kind>,
    state: &'a mut KindState,
}

impl<'a> KindScope<'a> {
    /// Starts a scope over `state` with the given binders already in scope.
    pub fn with_bindings(state: &'a mut KindState, bound: HashMap<String, Kind>) -> Self {
        Self { bound, state }
    }

    /// Starts a scope over `state` with no binder in scope.
    pub fn new(state: &'a mut KindState) -> Self {
        Self::with_bindings(state, HashMap::new())
    }

    /// Binds `name` to `kind` for the rest of the expression.
    pub fn bind(&mut self, name: String, kind: Kind) {
        self.bound.insert(name, kind);
    }

    /// The kind currently bound to `name`, if the name is in scope.
    pub fn lookup(&self, name: &str) -> Option<Kind> {
        self.bound.get(name).cloned()
    }

    /// Allocates a fresh kind variable.
    pub fn fresh(&mut self) -> Kind {
        self.state.fresh()
    }

    /// The solver this scope allocates from, so a caller that binds a rigid
    /// `forall` binder does it through the same state.
    pub fn state(&mut self) -> &mut KindState {
        self.state
    }

    /// The binders currently in scope, for a caller that needs to restore them
    /// after leaving a nested binder.
    pub fn bindings(&self) -> &HashMap<String, Kind> {
        &self.bound
    }

    /// Restores the binders in scope, undoing the bindings a nested binder made.
    pub fn restore(&mut self, bound: HashMap<String, Kind>) {
        self.bound = bound;
    }

    /// Takes the binders in scope, leaving the scope empty.
    pub fn into_bindings(self) -> HashMap<String, Kind> {
        self.bound
    }
}

/// Reads a resolved type expression as the kind it denotes.
///
/// This is the only translation from a type expression to a kind. A primitive
/// constructor denotes itself, a user declaration denotes its resolved identity,
/// and an application, an arrow, a `forall`, and a literal all follow the same
/// spine, so nothing here is a special case for one primitive. `None` is
/// returned for a form the kind language does not define — an unlowered type
/// operator chain — and the caller reports it.
pub fn denote_kind(ty: &hir::Type, scope: &mut KindScope<'_>) -> Option<Kind> {
    Some(match &ty.kind {
        TypeKind::Wildcard => scope.fresh(),
        TypeKind::Variable(name) => scope.lookup(name).unwrap_or_else(|| scope.fresh()),
        TypeKind::Constructor(builtin) => Kind::Builtin(*builtin),
        TypeKind::Named(id) | TypeKind::Opaque(id) => Kind::Named(*id),
        TypeKind::Application(..) => {
            let (head, arguments) = flatten_spine(ty);
            let mut kind = denote_kind(head, scope)?;
            for argument in arguments {
                kind = Kind::app(kind, denote_kind(argument, scope)?);
            }
            kind
        }
        TypeKind::OperatorChain { .. } => return None,
        TypeKind::Function { parameter, result } => Kind::Function(
            Box::new(denote_kind(parameter, scope)?),
            Box::new(denote_kind(result, scope)?),
        ),
        TypeKind::Forall { variables, body } => {
            let saved = scope.bindings().clone();
            for variable in variables {
                let kind = match &variable.kind {
                    Some(annotation) => denote_kind(annotation, scope)?,
                    None => type_kind(),
                };
                scope.bind(variable.name.clone(), kind);
            }
            let kind = denote_kind(body, scope);
            scope.restore(saved);
            kind?
        }
        TypeKind::Constrained { body, .. } => denote_kind(body, scope)?,
        // A row or record literal read as a kind is a row of types. Its entries
        // are checked at their own kind where the expression is read as a type,
        // which is the only place a row's element kind is decided.
        TypeKind::Row { .. } => Kind::row(type_kind()),
        TypeKind::Record { .. } => type_kind(),
        TypeKind::Integer(_) => Kind::Builtin(BuiltinType::Int),
        TypeKind::String(_) => Kind::Builtin(BuiltinType::Symbol),
    })
}
