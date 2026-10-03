use super::super::*;

mod bifunctor;
mod contravariant;
mod functor;
mod newtype;
mod ord;
mod types;

pub(crate) use types::contains_wildcard;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KnownDerivingClass {
    Eq,
    Ord,
    Functor,
    Bifunctor,
    Contravariant,
}

impl KnownDerivingClass {
    fn identity(self) -> (&'static str, &'static str) {
        match self {
            Self::Eq => ("Data.Eq", "Eq"),
            Self::Ord => ("Data.Ord", "Ord"),
            Self::Functor => ("Data.Functor", "Functor"),
            Self::Bifunctor => ("Data.Bifunctor", "Bifunctor"),
            Self::Contravariant => ("Data.Functor.Contravariant", "Contravariant"),
        }
    }

    fn method(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ord => "compare",
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

    /// Generates the structural `Eq` method for a local data or newtype type.
    /// Field comparisons remain ordinary class-method selections, so explicit
    /// instance-context dictionaries and imported instances use the existing
    /// evidence solver.
    pub(super) fn derive_known_class_method(
        &mut self,
        class_id: hir::TypeId,
        class: &ClassInfo,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        if class.parameters.len() != 1 {
            return self.deriving_error(span, "known-class deriving requires a unary class");
        }
        let Some(known_class) = self.known_deriving_class(class_id) else {
            return self.deriving_error(
                span,
                "the known-class deriving rule is unavailable for this class",
            );
        };
        if method.name != known_class.method() {
            return self.deriving_error(
                span,
                "the known-class deriving rule is unavailable for this class method",
            );
        }
        match known_class {
            KnownDerivingClass::Functor => {
                return self.derive_functor_method(method, class_arguments, span);
            }
            KnownDerivingClass::Bifunctor => {
                return self.derive_bifunctor_method(method, class_arguments, span);
            }
            KnownDerivingClass::Contravariant => {
                return self.derive_contravariant_method(method, class_arguments, span);
            }
            KnownDerivingClass::Ord => {
                return self.derive_ord_method(method, class_arguments, span);
            }
            KnownDerivingClass::Eq => {}
        }
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(span, "Eq deriving requires one type argument");
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(
                span,
                "Eq deriving requires a local data or newtype constructor",
            );
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(span, "cannot find the data declaration to derive Eq");
        };
        if type_id.module != self.env.module_id
            || !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            )
            || arguments.len() != declaration.parameters.len()
        {
            return self.deriving_error(
                span,
                "Eq deriving requires a locally declared, fully applied data type",
            );
        }

        let left = self.fresh_deriving_binder("__derived_left", span);
        let right = self.fresh_deriving_binder("__derived_right", span);
        let mut left_case_branches = Vec::new();
        for constructor in &declaration.constructors {
            let left_fields = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_l", span))
                .collect::<Vec<_>>();
            let right_fields = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_r", span))
                .collect::<Vec<_>>();
            let body = Self::derive_eq_field_tests(
                constructor,
                &left_fields,
                &right_fields,
                method.symbol,
                span,
            );
            let same_constructor = hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: hir::Pattern {
                    kind: hir::PatternKind::Constructor {
                        symbol: constructor.symbol,
                        name_span: constructor.name_span,
                        arguments: right_fields
                            .iter()
                            .cloned()
                            .map(|binder| hir::Pattern {
                                kind: hir::PatternKind::Var(binder),
                                span,
                            })
                            .collect(),
                    },
                    span,
                },
                value: body,
                span,
            };
            let mismatch = hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: hir::Pattern {
                    kind: hir::PatternKind::Wildcard,
                    span,
                },
                value: boolean_literal(false, span),
                span,
            };
            let right_case = hir::Expr {
                kind: hir::ExprKind::Case {
                    scrutinee: Box::new(local_expr(right.id, span)),
                    branches: vec![same_constructor, mismatch],
                },
                span,
            };
            left_case_branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: hir::Pattern {
                    kind: hir::PatternKind::Constructor {
                        symbol: constructor.symbol,
                        name_span: constructor.name_span,
                        arguments: left_fields
                            .iter()
                            .cloned()
                            .map(|binder| hir::Pattern {
                                kind: hir::PatternKind::Var(binder),
                                span,
                            })
                            .collect(),
                    },
                    span,
                },
                value: right_case,
                span,
            });
        }
        if left_case_branches.is_empty() {
            left_case_branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: hir::Pattern {
                    kind: hir::PatternKind::Wildcard,
                    span,
                },
                value: boolean_literal(true, span),
                span,
            });
        }
        let implementation = hir::Expr {
            kind: hir::ExprKind::Lambda {
                binder: left.clone(),
                body: Box::new(hir::Expr {
                    kind: hir::ExprKind::Lambda {
                        binder: right.clone(),
                        body: Box::new(hir::Expr {
                            kind: hir::ExprKind::Case {
                                scrutinee: Box::new(local_expr(left.id, span)),
                                branches: left_case_branches,
                            },
                            span,
                        }),
                    },
                    span,
                }),
            },
            span,
        };
        self.infer_derived_method(method, class_arguments, &implementation)
    }

    pub(super) fn validate_known_deriving_class(
        &mut self,
        class_id: hir::TypeId,
        class: &ClassInfo,
        span: TextRange,
    ) -> Option<()> {
        let Some(known_class) = self.known_deriving_class(class_id) else {
            return self.deriving_error(
                span,
                "the known-class deriving rule is unavailable for this class",
            );
        };
        if class.parameters.len() != 1 {
            return self.deriving_error(span, "known-class deriving requires a unary class");
        }
        if !class
            .methods
            .iter()
            .any(|method| method.name == known_class.method())
        {
            return self.deriving_error(
                span,
                "the class is missing the method required by its known deriving rule",
            );
        }
        Some(())
    }

    fn known_deriving_class(&self, class_id: hir::TypeId) -> Option<KnownDerivingClass> {
        let module = self.env.type_modules.get(&class_id)?.as_str();
        let name = self.env.type_names.get(&class_id)?.as_str();
        [
            KnownDerivingClass::Eq,
            KnownDerivingClass::Ord,
            KnownDerivingClass::Functor,
            KnownDerivingClass::Bifunctor,
            KnownDerivingClass::Contravariant,
        ]
        .into_iter()
        .find(|known| known.identity() == (module, name))
    }

    fn derive_eq_field_tests(
        constructor: &hir::Constructor,
        left_fields: &[hir::LocalBinder],
        right_fields: &[hir::LocalBinder],
        eq_method: SymbolId,
        span: TextRange,
    ) -> hir::Expr {
        let tests = constructor
            .fields
            .iter()
            .zip(left_fields)
            .zip(right_fields)
            .map(|((_field, left), right)| {
                let method = global_expr(eq_method, span);
                apply_expr(
                    apply_expr(method, local_expr(left.id, span), span),
                    local_expr(right.id, span),
                    span,
                )
            })
            .collect::<Vec<_>>();
        tests
            .into_iter()
            .rev()
            .fold(boolean_literal(true, span), |rest, test| hir::Expr {
                kind: hir::ExprKind::If {
                    condition: Box::new(test),
                    then_branch: Box::new(rest),
                    else_branch: Box::new(boolean_literal(false, span)),
                },
                span,
            })
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
