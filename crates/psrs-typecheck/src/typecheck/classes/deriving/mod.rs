use super::super::*;

mod bifunctor;
mod contravariant;
mod eq;
mod foldable;
mod functor;
mod generic;
mod mapping;
mod newtype;
mod ord;
mod profunctor;
mod registry;
mod syntax;
mod traversable;
mod types;
mod usage;

pub(in crate::typecheck) use registry::{DerivingRegistry, KnownClass};

pub(crate) use types::contains_wildcard;

use syntax::{apply_expr, boolean_literal, global_expr, local_expr};
use usage::MappingClasses;

/// Whether a structural rule produces this class method. Used only to reject a
/// method the rule does not derive.
fn handles_method(known: KnownClass, name: &str) -> bool {
    match known {
        KnownClass::Eq => name == "eq",
        KnownClass::Eq1 => name == "eq1",
        KnownClass::Ord => name == "compare",
        KnownClass::Ord1 => name == "compare1",
        KnownClass::Functor => name == "map",
        KnownClass::Bifunctor => name == "bimap",
        KnownClass::Contravariant => name == "cmap",
        KnownClass::Profunctor => name == "dimap",
        KnownClass::Foldable => matches!(name, "foldMap" | "foldr" | "foldl"),
        KnownClass::Bifoldable => matches!(name, "bifoldMap" | "bifoldr" | "bifoldl"),
        KnownClass::Traversable => matches!(name, "traverse" | "sequence"),
        KnownClass::Bitraversable => matches!(name, "bitraverse" | "bisequence"),
        KnownClass::Newtype => name == "wrap",
        KnownClass::Generic => matches!(name, "to" | "from"),
    }
}

impl Checker {
    pub(super) fn infer_derived_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        implementation: &hir::Expr,
    ) -> Option<InferredExpr> {
        let class_id = self.env.class_methods.get(&method.symbol)?.0;
        let class = self.env.classes.get(&class_id)?.clone();
        let mut variables = HashMap::new();
        for (parameter, argument) in class.parameters.iter().zip(class_arguments) {
            variables.insert(parameter.clone(), argument.clone());
        }
        let expected = self.elaborate_type(&method.signature, &mut variables);
        self.infer_expr_with_expected(implementation, Some(expected))
    }

    /// Synthesizes the methods of a compiler-supported derived class.
    pub(super) fn derive_known_class_method(
        &mut self,
        class_id: hir::TypeId,
        class: &ClassInfo,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let Some(known_class) = self.env.deriving.known_class(class_id) else {
            return self.deriving_error(
                TypeCheckErrorKind::CannotDerive,
                span,
                "the compiler has no deriving rule for this class",
            );
        };
        if known_class == KnownClass::Newtype {
            return self.deriving_error(
                TypeCheckErrorKind::CannotDerive,
                span,
                "Newtype has no derivable class methods",
            );
        }
        if known_class == KnownClass::Generic {
            if !matches!(method.name.as_str(), "to" | "from") {
                return self.deriving_error(
                    TypeCheckErrorKind::CannotDerive,
                    span,
                    "Generic derives only its `to` and `from` methods",
                );
            }
            return self.derive_generic_method(method, class_arguments, span);
        }
        if class.parameters.len() != 1 {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidDerivedInstance,
                span,
                "known-class deriving requires a unary class",
            );
        }
        if !handles_method(known_class, &method.name) {
            return self.deriving_error(
                TypeCheckErrorKind::CannotDerive,
                span,
                "the known-class deriving rule is unavailable for this class method",
            );
        }
        match known_class {
            KnownClass::Eq => self.derive_eq_method(method, class_arguments, span),
            KnownClass::Eq1 => self.derive_eq1_method(method, class_arguments, span),
            KnownClass::Ord => self.derive_ord_method(method, class_arguments, span),
            KnownClass::Ord1 => self.derive_ord1_method(method, class_arguments, span),
            KnownClass::Functor => self.derive_functor_method(method, class_arguments, span),
            KnownClass::Bifunctor => self.derive_bifunctor_method(method, class_arguments, span),
            KnownClass::Contravariant => {
                self.derive_contravariant_method(method, class_arguments, span)
            }
            KnownClass::Profunctor => self.derive_profunctor_method(method, class_arguments, span),
            KnownClass::Foldable | KnownClass::Bifoldable => {
                self.derive_foldable_method(known_class, method, class_arguments, span)
            }
            KnownClass::Traversable | KnownClass::Bitraversable => {
                self.derive_traversable_method(known_class, method, class_arguments, span)
            }
            KnownClass::Newtype | KnownClass::Generic => unreachable!("handled above"),
        }
    }

    pub(super) fn validate_known_deriving_class(
        &mut self,
        class_id: hir::TypeId,
        class: &ClassInfo,
        head: &hir::Type,
        head_arguments: &[InferType],
        span: TextRange,
    ) -> Option<()> {
        let Some(known_class) = self.env.deriving.known_class(class_id) else {
            return self.deriving_error(
                TypeCheckErrorKind::CannotDerive,
                span,
                "the compiler has no deriving rule for this class",
            );
        };
        let expected_arity = match known_class {
            KnownClass::Newtype | KnownClass::Generic => 2,
            _ => 1,
        };
        if class.parameters.len() != expected_arity || head_arguments.len() != expected_arity {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidDerivedInstance,
                span,
                "known-class deriving requires the class's supported parameter arity",
            );
        }
        if known_class.is_head_shape() {
            let (_, raw_arguments) = types::flatten_type_application(head);
            let is_wildcard = raw_arguments
                .last()
                .is_some_and(|argument| matches!(argument.kind, hir::TypeKind::Wildcard));
            if !is_wildcard {
                return self.deriving_error(
                    TypeCheckErrorKind::ExpectedWildcard,
                    span,
                    "the derived class's final type argument must be a type wildcard",
                );
            }
        }
        let has_required_methods = match known_class {
            KnownClass::Newtype => class.methods.is_empty(),
            KnownClass::Generic => ["to", "from"]
                .iter()
                .all(|name| class.methods.iter().any(|method| method.name == *name)),
            _ => class
                .methods
                .iter()
                .any(|method| handles_method(known_class, &method.name)),
        };
        if !has_required_methods {
            return self.deriving_error(
                TypeCheckErrorKind::CannotDerive,
                span,
                "the class is missing the method required by its known deriving rule",
            );
        }
        Some(())
    }

    /// A known class's method symbol by method name, read from the registry.
    fn known_method(&self, known: KnownClass, name: &str) -> Option<SymbolId> {
        self.env.deriving.method(known, name)
    }

    fn fresh_deriving_local(&mut self, prefix: &str, span: TextRange) -> hir::LocalBinder {
        let id = LocalId(self.state.next_dictionary_local);
        self.state.next_dictionary_local += 1;
        hir::LocalBinder {
            id,
            name: format!("{prefix}_{}", id.0),
            span,
        }
    }

    fn fresh_deriving_binder(&mut self, prefix: &str, span: TextRange) -> hir::LocalBinder {
        self.fresh_deriving_local(prefix, span)
    }

    pub(in crate::typecheck::classes) fn deriving_error<T>(
        &mut self,
        kind: TypeCheckErrorKind,
        span: TextRange,
        message: &str,
    ) -> Option<T> {
        self.state
            .errors
            .push(TypeCheckError::new(kind, span, message));
        None
    }
}

/// Whether the type is an application whose immediate head is a type
/// variable, such as `f a`. The official `Eq`/`Ord` deriving rules compare
/// exactly these fields through the class's higher-kinded `eq1`/`compare1`
/// counterpart; a deeper application such as `f a b` is not one, matching the
/// official `isAppliedVar` test.
fn is_applied_variable(ty: &hir::Type) -> bool {
    matches!(
        &ty.kind,
        hir::TypeKind::Application(function, _) if matches!(function.kind, hir::TypeKind::Variable(_))
    )
}

fn flatten_spine(ty: &InferType) -> (&InferType, Vec<InferType>) {
    let mut head = ty;
    let mut arguments = Vec::new();
    while let InferType::Application(function, argument) = head {
        arguments.push((**argument).clone());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}
