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

    /// Applies the substitution as far as it reaches. This is how a recorded
    /// kind is read back, so the answer accounts for every binding made since.
    pub fn resolve(&self, kind: Kind) -> Kind {
        match kind {
            Kind::Variable(variable) => match self.substitution.get(&variable) {
                Some(kind) => self.resolve(kind.clone()),
                None => Kind::Variable(variable),
            },
            Kind::App(function, argument) => Kind::App(
                Box::new(self.resolve(*function)),
                Box::new(self.resolve(*argument)),
            ),
            Kind::Function(parameter, result) => Kind::Function(
                Box::new(self.resolve(*parameter)),
                Box::new(self.resolve(*result)),
            ),
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
        (Kind::App(f1, a1), Kind::App(f2, a2)) => {
            unify_kind(state, *f1, *f2, span)?;
            unify_kind(state, *a1, *a2, span)
        }
        (Kind::Function(p1, r1), Kind::Function(p2, r2)) => {
            unify_kind(state, *p1, *p2, span)?;
            unify_kind(state, *r1, *r2, span)
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
        Kind::Builtin(_) | Kind::Named(_) => false,
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
        atomic => atomic.clone(),
    }
}
