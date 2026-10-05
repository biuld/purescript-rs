use super::super::*;

mod bifunctor;
mod contravariant;
mod eq;
mod functor;
mod generic;
mod newtype;
mod ord;
mod types;

pub(crate) use types::contains_wildcard;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KnownDerivingClass {
    Eq,
    Eq1,
    Ord,
    Ord1,
    Newtype,
    Generic,
    Functor,
    Bifunctor,
    Contravariant,
}

impl KnownDerivingClass {
    fn identity(self) -> (&'static str, &'static str) {
        match self {
            Self::Eq => ("Data.Eq", "Eq"),
            Self::Eq1 => ("Data.Eq", "Eq1"),
            Self::Ord => ("Data.Ord", "Ord"),
            Self::Ord1 => ("Data.Ord", "Ord1"),
            Self::Newtype => ("Data.Newtype", "Newtype"),
            Self::Generic => ("Data.Generic.Rep", "Generic"),
            Self::Functor => ("Data.Functor", "Functor"),
            Self::Bifunctor => ("Data.Bifunctor", "Bifunctor"),
            Self::Contravariant => ("Data.Functor.Contravariant", "Contravariant"),
        }
    }

    fn method(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Eq1 => "eq1",
            Self::Ord => "compare",
            Self::Ord1 => "compare1",
            Self::Newtype => "wrap",
            Self::Generic => "to",
            Self::Functor => "map",
            Self::Bifunctor => "bimap",
            Self::Contravariant => "cmap",
        }
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
        let Some(known_class) = self.known_deriving_class(class_id) else {
            return self.deriving_error(
                span,
                "the known-class deriving rule is unavailable for this class",
            );
        };
        if known_class == KnownDerivingClass::Newtype {
            return self.deriving_error(span, "Newtype has no derivable class methods");
        }
        if known_class == KnownDerivingClass::Generic {
            if !matches!(method.name.as_str(), "to" | "from") {
                return self
                    .deriving_error(span, "Generic derives only its `to` and `from` methods");
            }
            return self.derive_generic_method(method, class_arguments, span);
        }
        if class.parameters.len() != 1 {
            return self.deriving_error(span, "known-class deriving requires a unary class");
        }
        if method.name != known_class.method() {
            return self.deriving_error(
                span,
                "the known-class deriving rule is unavailable for this class method",
            );
        }
        match known_class {
            KnownDerivingClass::Eq => self.derive_eq_method(method, class_arguments, span),
            KnownDerivingClass::Eq1 => self.derive_eq1_method(method, class_arguments, span),
            KnownDerivingClass::Ord => self.derive_ord_method(method, class_arguments, span),
            KnownDerivingClass::Ord1 => self.derive_ord1_method(method, class_arguments, span),
            KnownDerivingClass::Functor => {
                self.derive_functor_method(method, class_arguments, span)
            }
            KnownDerivingClass::Bifunctor => {
                self.derive_bifunctor_method(method, class_arguments, span)
            }
            KnownDerivingClass::Contravariant => {
                self.derive_contravariant_method(method, class_arguments, span)
            }
            KnownDerivingClass::Newtype => unreachable!("handled above"),
            KnownDerivingClass::Generic => unreachable!("handled above"),
        }
    }

    pub(super) fn validate_known_deriving_class(
        &mut self,
        class_id: hir::TypeId,
        class: &ClassInfo,
        head_arguments: &[InferType],
        span: TextRange,
    ) -> Option<()> {
        let Some(known_class) = self.known_deriving_class(class_id) else {
            return self.deriving_error(
                span,
                "the known-class deriving rule is unavailable for this class",
            );
        };
        let expected_arity = match known_class {
            KnownDerivingClass::Newtype | KnownDerivingClass::Generic => 2,
            _ => 1,
        };
        if class.parameters.len() != expected_arity || head_arguments.len() != expected_arity {
            return self.deriving_error(
                span,
                "known-class deriving requires the class's supported parameter arity",
            );
        }
        let has_required_methods = match known_class {
            KnownDerivingClass::Newtype => class.methods.is_empty(),
            KnownDerivingClass::Generic => ["to", "from"]
                .iter()
                .all(|name| class.methods.iter().any(|method| method.name == *name)),
            _ => class
                .methods
                .iter()
                .any(|method| method.name == known_class.method()),
        };
        if !has_required_methods {
            return self.deriving_error(
                span,
                "the class is missing the method required by its known deriving rule",
            );
        }
        if known_class == KnownDerivingClass::Newtype {
            let underlying = self.newtype_underlying_type(&head_arguments[..1], span)?;
            let errors_before = self.state.errors.len();
            self.unify(head_arguments[1].clone(), underlying, span);
            if self.state.errors.len() != errors_before {
                return None;
            }
        }
        if known_class == KnownDerivingClass::Generic {
            let representation = self.generic_representation(&head_arguments[0], span)?;
            let errors_before = self.state.errors.len();
            self.unify(head_arguments[1].clone(), representation, span);
            if self.state.errors.len() != errors_before {
                return None;
            }
        }
        Some(())
    }

    fn known_method_symbol(
        &self,
        module_name: &str,
        class_name: &str,
        method_name: &str,
    ) -> Option<SymbolId> {
        let class_id = self.env.type_names.iter().find_map(|(class_id, name)| {
            (name == class_name
                && self
                    .env
                    .type_modules
                    .get(class_id)
                    .is_some_and(|module| module == module_name))
            .then_some(*class_id)
        })?;
        self.env
            .classes
            .get(&class_id)?
            .methods
            .iter()
            .find(|method| method.name == method_name)
            .map(|method| method.symbol)
    }

    fn known_deriving_class(&self, class_id: hir::TypeId) -> Option<KnownDerivingClass> {
        let module = self.env.type_modules.get(&class_id)?.as_str();
        let name = self.env.type_names.get(&class_id)?.as_str();
        [
            KnownDerivingClass::Eq,
            KnownDerivingClass::Eq1,
            KnownDerivingClass::Ord,
            KnownDerivingClass::Ord1,
            KnownDerivingClass::Newtype,
            KnownDerivingClass::Generic,
            KnownDerivingClass::Functor,
            KnownDerivingClass::Bifunctor,
            KnownDerivingClass::Contravariant,
        ]
        .into_iter()
        .find(|known| known.identity() == (module, name))
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
        span: TextRange,
        message: &str,
    ) -> Option<T> {
        self.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::UnsupportedClass,
            span,
            message,
        ));
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

fn local_expr(local: LocalId, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Local(local),
        span,
    }
}

fn global_expr(symbol: SymbolId, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Global(symbol),
        span,
    }
}

fn apply_expr(function: hir::Expr, argument: hir::Expr, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Application(Box::new(function), Box::new(argument)),
        span,
    }
}

fn boolean_literal(value: bool, span: TextRange) -> hir::Expr {
    global_expr(
        if value {
            hir::Intrinsic::BoolTrue.symbol()
        } else {
            hir::Intrinsic::BoolFalse.symbol()
        },
        span,
    )
}
