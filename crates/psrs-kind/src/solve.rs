//! The one kind solver: one substitution over kind variables, one occurs check,
//! and the two equation operations every kind check in the compiler goes
//! through. A kind that is accepted in one position cannot be rejected in
//! another, because there is only one solver.

use crate::check::{INFINITE_KIND, KINDS_DO_NOT_UNIFY};
use crate::kind::Kind;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

/// A kind equation with no solution, carrying the official PureScript
/// `errorCode` reported for it and the span of the offending application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindError {
    pub code: &'static str,
    pub span: TextRange,
    pub message: &'static str,
}

fn kinds_do_not_unify(span: TextRange) -> KindError {
    KindError {
        code: KINDS_DO_NOT_UNIFY,
        span,
        message: "kinds do not unify",
    }
}

fn infinite_kind(span: TextRange) -> KindError {
    KindError {
        code: INFINITE_KIND,
        span,
        message: "the kind of this declaration is infinite",
    }
}

/// The mutable state kind inference maintains: the substitution over kind
/// variables, the next variable to allocate, and the rigid variables that came
/// from `forall` binders. Every kind equation solves through one of these, so a
/// type and its recorded kind cannot disagree after a binding.
#[derive(Clone, Debug, Default)]
pub struct KindState {
    substitution: HashMap<u32, Kind>,
    next_variable: u32,
    rigid: HashSet<u32>,
}

impl KindState {
    /// A state whose first allocated variable is `next`. This keeps a variable
    /// reserved for a compiler-owned scheme that other allocations must not
    /// reuse.
    pub fn starting_at(next: u32) -> Self {
        Self {
            next_variable: next,
            ..Self::default()
        }
    }

    /// Allocates a fresh kind variable.
    pub fn fresh(&mut self) -> Kind {
        let variable = self.next_variable;
        self.next_variable += 1;
        Kind::Variable(variable)
    }

    /// The next variable id this state will allocate.
    pub(crate) fn peek_next(&self) -> u32 {
        self.next_variable
    }

    /// Reserves every id below `next`. A scratch elaboration allocates from a
    /// clone and then keeps those ids from being reused by the live state.
    pub(crate) fn set_next(&mut self, next: u32) {
        self.next_variable = next;
    }

    /// Marks `variable` rigid, so no use can bind it. A `forall` binder is rigid
    /// for as long as the binder is in scope, which is what makes a skolem escape
    /// an error rather than a silent binding.
    pub fn make_rigid(&mut self, variable: u32) {
        self.rigid.insert(variable);
    }

    /// Whether `variable` is a rigid `forall` binder.
    pub fn is_rigid(&self, variable: u32) -> bool {
        self.rigid.contains(&variable)
    }

    /// Resolves a variable chain without descending beneath binders. Scoped
    /// conversions supply the lexical identities that must remain untouched.
    pub(crate) fn resolve_head_with(&self, mut kind: Kind, bound: impl Fn(u32) -> bool) -> Kind {
        while let Kind::Variable(variable) = kind {
            if bound(variable) {
                return Kind::Variable(variable);
            }
            match self.substitution.get(&variable) {
                Some(value) => kind = value.clone(),
                None => return Kind::Variable(variable),
            }
        }
        kind
    }

    /// Reads solver solutions while respecting lexical forall scopes.
    pub fn resolve(&self, kind: Kind) -> Kind {
        self.resolve_scoped(kind, &mut HashSet::new())
    }

    fn resolve_scoped(&self, kind: Kind, bound: &mut HashSet<u32>) -> Kind {
        match self.resolve_head_with(kind, |id| bound.contains(&id)) {
            Kind::App(function, argument) => Kind::App(
                Box::new(self.resolve_scoped(*function, bound)),
                Box::new(self.resolve_scoped(*argument, bound)),
            ),
            Kind::Function(parameter, result) => Kind::Function(
                Box::new(self.resolve_scoped(*parameter, bound)),
                Box::new(self.resolve_scoped(*result, bound)),
            ),
            Kind::RowExtend { label, field, tail } => Kind::RowExtend {
                label,
                field: Box::new(self.resolve_scoped(*field, bound)),
                tail: Box::new(self.resolve_scoped(*tail, bound)),
            },
            Kind::ForAll { variable, body } => {
                let introduced = bound.insert(variable);
                let body = self.resolve_scoped(*body, bound);
                if introduced {
                    bound.remove(&variable);
                }
                Kind::ForAll {
                    variable,
                    body: Box::new(body),
                }
            }
            atomic => atomic,
        }
    }

    /// Solves `left = right` through this state. See [`unify_kind`].
    pub fn unify(&mut self, left: Kind, right: Kind, span: TextRange) -> Result<(), KindError> {
        unify_kind(self, left, right, span)
    }
}

/// Solves the kind equation `left = right`.
///
/// Both sides are resolved through the one substitution first, so a binding
/// made earlier decides what the equation means. A rigid `forall` binder never
/// binds, which is what rejects a kind annotation that would capture one.
pub fn unify_kind(
    state: &mut KindState,
    left: Kind,
    right: Kind,
    span: TextRange,
) -> Result<(), KindError> {
    let left = state.resolve(left);
    let right = state.resolve(right);
    match (left, right) {
        (Kind::Variable(a), Kind::Variable(b)) if a == b => Ok(()),
        (Kind::Variable(a), Kind::Variable(b)) => match (state.is_rigid(a), state.is_rigid(b)) {
            (true, true) => Err(kinds_do_not_unify(span)),
            (true, false) => bind_kind_variable(state, b, Kind::Variable(a), span),
            (false, _) => bind_kind_variable(state, a, Kind::Variable(b), span),
        },
        (Kind::Variable(a), _) if state.is_rigid(a) => Err(kinds_do_not_unify(span)),
        (_, Kind::Variable(b)) if state.is_rigid(b) => Err(kinds_do_not_unify(span)),
        (Kind::Variable(a), other) | (other, Kind::Variable(a)) => {
            bind_kind_variable(state, a, other, span)
        }
        (Kind::Builtin(a), Kind::Builtin(b)) if a == b => Ok(()),
        (Kind::Named(a), Kind::Named(b)) if a == b => Ok(()),
        (Kind::TypeVariable(left), Kind::TypeVariable(right)) if left == right => Ok(()),
        (Kind::App(f1, a1), Kind::App(f2, a2)) => {
            unify_kind(state, *f1, *f2, span)?;
            unify_kind(state, *a1, *a2, span)
        }
        (Kind::Function(p1, r1), Kind::Function(p2, r2)) => {
            unify_kind(state, *p1, *p2, span)?;
            unify_kind(state, *r1, *r2, span)
        }
        (
            Kind::ForAll {
                variable: left_variable,
                body: left_body,
            },
            Kind::ForAll {
                variable: right_variable,
                body: right_body,
            },
        ) => {
            let right_body = substitute(
                &right_body,
                &HashMap::from([(right_variable, Kind::Variable(left_variable))]),
            );
            unify_kind(state, *left_body, right_body, span)
        }
        (Kind::RowEmpty, Kind::RowEmpty) => Ok(()),
        (
            Kind::RowExtend {
                label: left_label,
                field: left_field,
                tail: left_tail,
            },
            Kind::RowExtend {
                label: right_label,
                field: right_field,
                tail: right_tail,
            },
        ) if left_label == right_label => {
            unify_kind(state, *left_field, *right_field, span)?;
            unify_kind(state, *left_tail, *right_tail, span)
        }
        _ => Err(kinds_do_not_unify(span)),
    }
}

/// Binds a kind variable, rejecting a binding that would make the kind
/// infinite. The occurs check is the only escape rule: a kind that mentions the
/// variable it is bound to has no solution.
pub fn bind_kind_variable(
    state: &mut KindState,
    variable: u32,
    kind: Kind,
    span: TextRange,
) -> Result<(), KindError> {
    if occurs(variable, &kind) {
        return Err(infinite_kind(span));
    }
    state.substitution.insert(variable, kind);
    Ok(())
}

/// Whether `variable` occurs in `kind`.
pub fn occurs(variable: u32, kind: &Kind) -> bool {
    match kind {
        Kind::Variable(other) => variable == *other,
        Kind::App(function, argument) => occurs(variable, function) || occurs(variable, argument),
        Kind::Function(parameter, result) => {
            occurs(variable, parameter) || occurs(variable, result)
        }
        Kind::RowExtend { field, tail, .. } => occurs(variable, field) || occurs(variable, tail),
        Kind::ForAll {
            variable: bound,
            body,
        } => *bound != variable && occurs(variable, body),
        Kind::Builtin(_) | Kind::Named(_) | Kind::RowEmpty | Kind::TypeVariable(_) => false,
    }
}

/// Replaces the kind variables in `mapping` inside `kind`. Instantiating a
/// scheme is exactly this: one fresh kind per quantified variable, applied to
/// the scheme's kind.
pub fn substitute(kind: &Kind, mapping: &HashMap<u32, Kind>) -> Kind {
    match kind {
        Kind::Variable(variable) => mapping
            .get(variable)
            .cloned()
            .unwrap_or(Kind::Variable(*variable)),
        Kind::App(function, argument) => Kind::App(
            Box::new(substitute(function, mapping)),
            Box::new(substitute(argument, mapping)),
        ),
        Kind::Function(parameter, result) => Kind::Function(
            Box::new(substitute(parameter, mapping)),
            Box::new(substitute(result, mapping)),
        ),
        Kind::RowExtend { label, field, tail } => Kind::RowExtend {
            label: label.clone(),
            field: Box::new(substitute(field, mapping)),
            tail: Box::new(substitute(tail, mapping)),
        },
        Kind::ForAll { variable, body } => {
            if mapping.contains_key(variable) {
                // The binder is not free. A mapping that names it was built
                // for a different variable that happens to share the id, so
                // the quantifier stays and only the free variables of the
                // body are replaced.
                let mut inner = mapping.clone();
                inner.remove(variable);
                Kind::ForAll {
                    variable: *variable,
                    body: Box::new(substitute(body, &inner)),
                }
            } else {
                Kind::ForAll {
                    variable: *variable,
                    body: Box::new(substitute(body, mapping)),
                }
            }
        }
        atomic => atomic.clone(),
    }
}
