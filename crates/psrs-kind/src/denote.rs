//! The one translation from a type expression to a kind.

use crate::kind::{Kind, flatten_spine};
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
    /// A free name in a kind annotation is that type variable. Kind signatures
    /// leave this off, so an unbound name there is a fresh kind variable.
    free_names_are_type_variables: bool,
}

impl<'a> KindScope<'a> {
    /// Starts a scope over `state` with the given binders already in scope.
    pub fn with_bindings(state: &'a mut KindState, bound: HashMap<String, Kind>) -> Self {
        Self {
            bound,
            state,
            free_names_are_type_variables: false,
        }
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

/// The kind a type-position annotation denotes.
///
/// A name bound by `forall` inside the annotation is a kind variable. Any
/// other name is the type variable itself, so `KProxy k` stays an application
/// of `KProxy` to `k` and does not collapse to `Type`.
pub fn denote_kind_annotation(ty: &hir::Type, state: &mut KindState) -> Option<Kind> {
    let mut scope = KindScope {
        bound: HashMap::new(),
        state,
        free_names_are_type_variables: true,
    };
    denote_kind(ty, &mut scope)
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
        TypeKind::Wildcard | TypeKind::Hole(_) => scope.fresh(),
        TypeKind::Variable(name) => scope.lookup(name).unwrap_or_else(|| {
            if scope.free_names_are_type_variables {
                Kind::TypeVariable(name.clone())
            } else {
                scope.fresh()
            }
        }),
        // `(t :: k)` read as a kind is `k`. The inner type is checked by the
        // kind of a type, not by this translation.
        TypeKind::Kinded { kind, .. } => denote_kind(kind, scope)?,
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
            // A binder in a kind is a variable. Its annotation is the kind of
            // that variable, not a type the variable is replaced by, so
            // `forall k. k -> Type` stays quantified instead of collapsing to
            // `Type -> Type`.
            let saved = scope.bindings().clone();
            let mut quantified = Vec::with_capacity(variables.len());
            for variable in variables {
                if let Some(annotation) = &variable.kind {
                    let _ = denote_kind(annotation, scope)?;
                }
                let Kind::Variable(id) = scope.fresh() else {
                    unreachable!("a fresh kind is a variable")
                };
                scope.state().make_rigid(id);
                scope.bind(variable.name.clone(), Kind::Variable(id));
                quantified.push(id);
            }
            let mut kind = denote_kind(body, scope)?;
            scope.restore(saved);
            for id in quantified.into_iter().rev() {
                kind = Kind::ForAll {
                    variable: id,
                    body: Box::new(kind),
                };
            }
            kind
        }
        TypeKind::Constrained { body, .. } => denote_kind(body, scope)?,
        // A row literal is that row. A record literal is `Record` applied to
        // it. Collapsing either one to `Row Type` makes `{a :: Int | r}` and
        // `{b :: Int | r}` the same kind.
        TypeKind::Row { fields, tail } => denote_row(fields, tail.as_deref(), scope)?,
        TypeKind::Record { fields, tail } => Kind::app(
            Kind::Builtin(BuiltinType::Record),
            denote_row(fields, tail.as_deref(), scope)?,
        ),
        TypeKind::Integer(_) => Kind::Builtin(BuiltinType::Int),
        TypeKind::String(_) => Kind::Builtin(BuiltinType::Symbol),
    })
}

/// The row a row or record literal denotes, labels sorted, fields read as kinds.
fn denote_row(
    fields: &[hir::TypeField],
    tail: Option<&hir::Type>,
    scope: &mut KindScope<'_>,
) -> Option<Kind> {
    let mut denoted = Vec::with_capacity(fields.len());
    for field in fields {
        denoted.push((field.label.clone(), denote_kind(&field.ty, scope)?));
    }
    denoted.sort_by(|left, right| left.0.cmp(&right.0));
    let mut kind = match tail {
        Some(tail) => denote_kind(tail, scope)?,
        None => Kind::RowEmpty,
    };
    for (label, field) in denoted.into_iter().rev() {
        kind = Kind::RowExtend {
            label,
            field: Box::new(field),
            tail: Box::new(kind),
        };
    }
    Some(kind)
}
