//! The kind owner inference reads through.
//!
//! Every type unknown carries a kind, and binding one maintains it. This module
//! owns that: `kind_of_type` is the one way the kind of a resolved inference
//! type is read, `denote_kind` is the one way a type expression is read as a
//! kind, `primitive_kind` is the one primitive table, and `unify_kind` is the
//! one equation solver. A coercion, a row tail, an ordinary binding, and a
//! `forall` binder's annotation all reach the same substitution, so a kind that
//! is accepted in one position cannot be rejected in another.
//!
//! The recorded kinds live in [`InferState`](super::state::InferState) next to
//! the type substitutions they belong to, so `speculate` rolls them back with
//! the bindings that produced them.

use super::*;
use psrs_kind::{KindScheme, KindScope, type_kind};

impl Checker {
    /// Allocates a fresh kind variable from the one kind solver.
    pub(in crate::typecheck) fn fresh_kind(&mut self) -> Kind {
        self.state.kinds.fresh()
    }

    /// The kind recorded for an inference type variable. A variable that has no
    /// recorded kind gets a fresh one, so every unknown carries a kind even
    /// when the declaration that would have recorded one is missing.
    pub(in crate::typecheck) fn kind_of_variable(&mut self, variable: u32) -> Kind {
        if let Some(kind) = self.recorded_kind(variable) {
            return kind;
        }
        let kind = self.fresh_kind();
        self.record_variable_kind(variable, kind.clone());
        kind
    }

    /// The kind recorded for an inference type variable, if it has one. A fresh
    /// unknown always does, so this only answers for a variable that reached
    /// inference from somewhere else.
    pub(in crate::typecheck) fn recorded_kind(&self, variable: u32) -> Option<Kind> {
        self.state.variable_kinds.get(&variable).cloned()
    }

    /// Records the kind of an inference type variable. This is the only writer
    /// of the table: a fresh unknown, a `forall` binder, a constructor
    /// parameter, and a quantified variable's fresh instance all record through
    /// it, so the kind of one variable is established the same way wherever it
    /// came from.
    pub(in crate::typecheck) fn record_variable_kind(&mut self, variable: u32, kind: Kind) {
        self.state.variable_kinds.insert(variable, kind);
    }

    /// Reads a type expression as the kind it denotes.
    ///
    /// This is a thin owner over [`psrs_kind::denote_kind`]: the type checker
    /// keeps no second denotation and no private primitive table. A form the
    /// kind language does not define — an unlowered type operator chain — is
    /// reported against its own range and answered with a fresh kind variable,
    /// so elaboration continues and the diagnostic is not lost.
    pub(in crate::typecheck) fn kind_from_hir(
        &mut self,
        ty: &hir::Type,
        bound: &HashMap<String, Kind>,
    ) -> Kind {
        let state = &mut self.state.kinds;
        let mut scope = KindScope::with_bindings(state, bound.clone());
        match psrs_kind::denote_kind(ty, &mut scope) {
            Some(kind) => kind,
            None => {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedType,
                    ty.span,
                    "type operator chain reached type checking before P4",
                ));
                self.fresh_kind()
            }
        }
    }

    /// The kinds a type constructor's parameters have, one per parameter.
    ///
    /// The scheme comes from the checked kind environment, so an imported
    /// declaration's parameter kinds are the ones its declaring module checked.
    /// A constructor the program produced no scheme for gets fresh kinds: the
    /// missing scheme is the kind pass's diagnostic to report, and inference
    /// must not reject the module a second time for it.
    pub(in crate::typecheck) fn type_argument_kinds(
        &mut self,
        id: hir::TypeId,
        arity: usize,
    ) -> Vec<Kind> {
        let Some(scheme) = self.env.checked_kinds.kind_scheme(id).cloned() else {
            return (0..arity).map(|_| self.fresh_kind()).collect();
        };
        let kind = self.instantiate_kind_scheme(&scheme);
        let mut arguments = Vec::with_capacity(arity);
        let mut current = kind;
        while let Kind::Function(argument, result) = current {
            arguments.push(*argument);
            current = *result;
        }
        while arguments.len() < arity {
            arguments.push(self.fresh_kind());
        }
        arguments.truncate(arity);
        arguments
    }

    /// Instantiates a polymorphic kind: one fresh kind variable per quantified
    /// variable, substituted into the scheme's kind. This is
    /// [`psrs_kind::substitute`] over a mapping the one solver allocates.
    pub(in crate::typecheck) fn instantiate_kind_scheme(&mut self, scheme: &KindScheme) -> Kind {
        let mapping = scheme
            .variables
            .iter()
            .map(|variable| (*variable, self.fresh_kind()))
            .collect::<HashMap<_, _>>();
        psrs_kind::substitute(&scheme.kind, &mapping)
    }

    /// The kind of a resolved inference type, read through the one kind
    /// substitution.
    ///
    /// `span` is the range of the operation that reached this type:
    /// `InferType` carries no ranges of its own, so the same convention the row
    /// normalizer uses applies. `None` means the type names something the
    /// program produced no checked kind for; that absence is the kind pass's
    /// diagnostic, and a caller here treats it as "no kind constraint to
    /// enforce" rather than inventing one.
    pub(in crate::typecheck) fn kind_of_type(
        &mut self,
        ty: &InferType,
        span: TextRange,
    ) -> Option<Kind> {
        let ty = self.resolve_type(ty.clone());
        Some(match ty {
            InferType::Variable(variable) => self.kind_of_variable(variable),
            InferType::Constructor(constructor) => self.kind_of_constructor(&constructor)?,
            InferType::Application(function, argument) => {
                // Applying a type constructor consumes an argument and produces a
                // result, so the head's kind is an arrow. A head whose kind is
                // not an arrow is a kind error at this application, and the
                // smallest offending node is the application itself.
                let function_kind = self.kind_of_type(&function, span)?;
                let argument_kind = self.kind_of_type(&argument, span)?;
                let result_kind = self.fresh_kind();
                let arrow = Kind::Function(Box::new(argument_kind), Box::new(result_kind.clone()));
                let applied = self.unify_kind(function_kind, arrow, span);
                if !applied {
                    return None;
                }
                self.state.kinds.resolve(result_kind)
            }
            // A `forall` or a constraint denotes the kind of its body: the
            // binder introduces a variable the body names, and the constraint is
            // discharged into a dictionary arrow before the body is reached.
            InferType::ForAll { body, .. } | InferType::Constrained { body, .. } => {
                self.kind_of_type(&body, span)?
            }
            InferType::RowEmpty => Kind::row(type_kind()),
            // A row's entries have the kind its tail admits, so the entry's kind
            // is unified with the row's element kind and the row itself has kind
            // `Row k`.
            InferType::RowExtend { ty, tail, .. } => {
                let element = self.fresh_kind();
                let row = Kind::row(element.clone());
                let field_kind = self.kind_of_type(&ty, span)?;
                let tail_kind = self.kind_of_type(&tail, span)?;
                if !self.unify_kind(element, field_kind, span) {
                    return None;
                }
                if !self.unify_kind(row.clone(), tail_kind, span) {
                    return None;
                }
                self.state.kinds.resolve(row)
            }
            InferType::TypeLevelString(_) => Kind::Builtin(hir::BuiltinType::Symbol),
            InferType::TypeLevelInt(_) => Kind::Builtin(hir::BuiltinType::Int),
        })
    }

    fn kind_of_constructor(&mut self, constructor: &TypeConstructor) -> Option<Kind> {
        // A user constructor's kind is the scheme its declaring module checked,
        // never one rebuilt from surface syntax.
        let TypeConstructor::User(id) = constructor else {
            return Some(psrs_kind::primitive_kind(primitive_builtin(constructor)));
        };
        let scheme = self.env.checked_kinds.kind_scheme(*id)?.clone();
        Some(self.instantiate_kind_scheme(&scheme))
    }

    /// Solves a kind equation through the one kind substitution, reporting
    /// `KindsDoNotUnify` at `span` when the two kinds have no common solution.
    ///
    /// This is the only way inference solves a kind equation.
    pub(in crate::typecheck) fn unify_kind(
        &mut self,
        left: Kind,
        right: Kind,
        span: TextRange,
    ) -> bool {
        match psrs_kind::unify_kind(&mut self.state.kinds, left, right, span) {
            Ok(()) => true,
            Err(error) => {
                self.report_kind_error(&error);
                false
            }
        }
    }

    /// Whether a representation conversion may be attempted between `source`
    /// and `target` at all, which is the question their kinds answer.
    ///
    /// This adds no machinery of its own: it reads both kinds through
    /// [`Self::kind_of_type`] and solves the equation through the one kind
    /// solver, exactly as a binding does. It is a trial, because a coercion that
    /// turns out not to hold — a phantom argument, an invisible newtype — must
    /// not leave a kind binding behind for a later one to read.
    ///
    /// A type with no kind, because the program produced no checked scheme for
    /// it, is not treated as compatible. The missing scheme is a kind-pass
    /// diagnostic; until it is reported, these kinds say nothing about the
    /// conversion.
    pub(super) fn coercion_kinds_compatible(
        &mut self,
        source: &InferType,
        target: &InferType,
        span: TextRange,
    ) -> bool {
        self.speculate(|checker| {
            let source = checker.kind_of_type(source, span)?;
            let target = checker.kind_of_type(target, span)?;
            checker.unify_kind(source, target, span).then_some(())
        })
        .is_some()
    }

    fn report_kind_error(&mut self, error: &psrs_kind::KindError) {
        self.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::KindsDoNotUnify,
            error.span,
            error.message,
        ));
    }
}

/// The primitive a primitive type constructor denotes. `User` has no primitive:
/// its kind is the checked scheme, which the caller reads from the environment.
fn primitive_builtin(constructor: &TypeConstructor) -> hir::BuiltinType {
    match constructor {
        TypeConstructor::Function => hir::BuiltinType::Function,
        TypeConstructor::Record => hir::BuiltinType::Record,
        TypeConstructor::Row => hir::BuiltinType::Row,
        TypeConstructor::Array => hir::BuiltinType::Array,
        TypeConstructor::Int => hir::BuiltinType::Int,
        TypeConstructor::Number => hir::BuiltinType::Number,
        TypeConstructor::Boolean => hir::BuiltinType::Boolean,
        TypeConstructor::String => hir::BuiltinType::String,
        TypeConstructor::Char => hir::BuiltinType::Char,
        TypeConstructor::Unit => hir::BuiltinType::Unit,
        TypeConstructor::User(_) => {
            unreachable!("a user constructor's kind is its checked scheme")
        }
    }
}
