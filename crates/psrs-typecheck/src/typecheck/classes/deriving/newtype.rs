use super::super::super::unify::substitute;
use super::super::super::*;
use super::super::evidence::record_field_type;
use super::flatten_spine;

struct CheckedNewtype {
    type_id: hir::TypeId,
    underlying: InferType,
}

impl Checker {
    /// Builds the method implementation for a `derive newtype` instance.
    /// The source method is selected from the wrapped type's dictionary, then
    /// adapted through the trusted representation-transparent newtype contract.
    /// This is dictionary reuse, not a claim of ordinary Coercible entailment.
    pub(in crate::typecheck::classes) fn derive_newtype_method(
        &mut self,
        class_id: hir::TypeId,
        class: &ClassInfo,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        if class.parameters.len() != class_arguments.len() {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidNewtypeInstance,
                span,
                "derive newtype class head has the wrong arity",
            );
        }
        let checked = self.checked_newtype(class_arguments, span, false)?;
        let origin = thir::UncheckedCoercionOrigin::NewtypeDeriving {
            class_id,
            newtype_id: checked.type_id,
        };
        let underlying = checked.underlying;

        let mut underlying_arguments = class_arguments.to_vec();
        *underlying_arguments.last_mut()? = underlying;
        let underlying_constraint = ClassConstraint {
            class_id,
            arguments: underlying_arguments.clone(),
            span,
        };
        let underlying_dictionary_type = self.dictionary_type(&underlying_constraint);
        let dictionary =
            self.push_wanted(underlying_constraint, underlying_dictionary_type.clone());

        // Elaborate the method signature once with stable placeholders for the
        // class parameters. Then instantiate those placeholders separately for
        // the derived and wrapped class heads. Method-level forall variables
        // therefore have identical identities on both sides of the adapter,
        // while a forall binder that shadows a class parameter remains scoped
        // to the method body.
        let mut method_variables = HashMap::new();
        let mut derived_substitutions = HashMap::new();
        let mut underlying_substitutions = HashMap::new();
        for ((parameter, derived), wrapped) in class
            .parameters
            .iter()
            .zip(class_arguments)
            .zip(&underlying_arguments)
        {
            let InferType::Variable(placeholder) = self.fresh() else {
                unreachable!("fresh type variables are variables")
            };
            method_variables.insert(parameter.clone(), InferType::Variable(placeholder));
            derived_substitutions.insert(placeholder, derived.clone());
            underlying_substitutions.insert(placeholder, wrapped.clone());
        }
        let method_template =
            self.elaborate_type_mode(&method.signature, &mut method_variables, false);
        let derived_method_type = substitute(&method_template, &derived_substitutions);
        let underlying_method_type = substitute(&method_template, &underlying_substitutions);
        if let Some(dictionary_method_type) =
            record_field_type(&underlying_dictionary_type, &method.name)
        {
            self.unify(underlying_method_type.clone(), dictionary_method_type, span);
        }
        let selected_method = InferredExpr {
            kind: InferredExprKind::FieldAccess {
                expression: Box::new(InferredExpr {
                    kind: InferredExprKind::Evidence(dictionary),
                    ty: underlying_dictionary_type,
                    span,
                }),
                field: method.name.clone(),
            },
            ty: underlying_method_type.clone(),
            span,
        };
        let mut method_foralls = Vec::new();
        leading_forall_variables(&derived_method_type, &mut method_foralls);
        self.with_skolem_scope(&method_foralls, |checker| {
            checker.adapt_newtype_method(
                selected_method,
                underlying_method_type,
                derived_method_type,
                span,
                origin,
            )
        })
    }

    pub(in crate::typecheck::classes) fn validate_newtype_deriving_instance(
        &mut self,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferType> {
        self.newtype_underlying_type(class_arguments, span, false)
    }

    pub(in crate::typecheck::classes) fn newtype_underlying_type(
        &mut self,
        class_arguments: &[InferType],
        span: TextRange,
        deriving_newtype_class: bool,
    ) -> Option<InferType> {
        self.checked_newtype(class_arguments, span, deriving_newtype_class)
            .map(|checked| checked.underlying)
    }

    fn checked_newtype(
        &mut self,
        class_arguments: &[InferType],
        span: TextRange,
        deriving_newtype_class: bool,
    ) -> Option<CheckedNewtype> {
        let Some(newtype) = class_arguments.last() else {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidNewtypeInstance,
                span,
                "derive newtype requires a class with a final type parameter",
            );
        };
        let resolved_newtype = self.resolve_type(newtype.clone());
        let (head, arguments) = flatten_spine(&resolved_newtype);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidNewtypeInstance,
                span,
                "derive newtype requires its final class argument to be a newtype constructor",
            );
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(
                TypeCheckErrorKind::CannotFindDerivingType,
                span,
                "cannot find the newtype declaration to derive",
            );
        };
        if declaration.kind != hir::TypeDeclarationKind::Newtype
            || type_id.module != self.env.module_id
            || arguments.len() > declaration.parameters.len()
        {
            let kind = if deriving_newtype_class
                && declaration.kind == hir::TypeDeclarationKind::Data
                && type_id.module == self.env.module_id
            {
                TypeCheckErrorKind::CannotDeriveNewtypeForData
            } else {
                TypeCheckErrorKind::InvalidNewtypeInstance
            };
            return self.deriving_error(
                kind,
                span,
                "derive newtype requires a locally declared newtype constructor",
            );
        }
        let Some(constructor) = declaration.constructors.first() else {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidNewtypeInstance,
                span,
                "the newtype has no data constructor",
            );
        };
        let [field] = constructor.fields.as_slice() else {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidNewtypeInstance,
                span,
                "derive newtype requires a constructor with exactly one field",
            );
        };

        let supplied = arguments.len();
        let mut newtype_arguments = arguments;
        for _ in supplied..declaration.parameters.len() {
            newtype_arguments.push(self.fresh());
        }
        let omitted_arguments = newtype_arguments[supplied..].to_vec();
        let mut newtype_variables = declaration
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .zip(newtype_arguments)
            .collect::<HashMap<_, _>>();
        let underlying = self.elaborate_type(field, &mut newtype_variables);
        let Some(underlying) = strip_newtype_arguments(underlying, &omitted_arguments, self) else {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidNewtypeInstance,
                span,
                "the wrapped type must end in every unapplied newtype parameter",
            );
        };
        Some(CheckedNewtype {
            type_id: *type_id,
            underlying,
        })
    }

    fn adapt_newtype_method(
        &mut self,
        value: InferredExpr,
        source: InferType,
        target: InferType,
        span: TextRange,
        origin: thir::UncheckedCoercionOrigin,
    ) -> Option<InferredExpr> {
        let source = self.resolve_type(source);
        let target = self.resolve_type(target);
        // Method quantifiers are elaborated once and then instantiated at the
        // two class heads, so both sides share those binders. Peel them before
        // adapting arrows; coercing the quantified type itself is not a
        // newtype representation change.
        if let (
            InferType::ForAll {
                variables: source_variables,
                body: source_body,
            },
            InferType::ForAll {
                variables: target_variables,
                body: target_body,
            },
        ) = (&source, &target)
            && source_variables == target_variables
        {
            let instantiated = InferredExpr {
                ty: source_body.as_ref().clone(),
                ..value
            };
            let adapted = self.adapt_newtype_method(
                instantiated,
                source_body.as_ref().clone(),
                target_body.as_ref().clone(),
                span,
                origin,
            )?;
            return Some(InferredExpr {
                ty: target,
                ..adapted
            });
        }
        if self.infer_types_equal(&source, &target) {
            return Some(InferredExpr {
                ty: target,
                ..value
            });
        }
        if let (Some((source_parameter, source_result)), Some((target_parameter, target_result))) =
            (arrow_parts(&source), arrow_parts(&target))
        {
            let local = LocalId(self.state.next_dictionary_local);
            self.state.next_dictionary_local += 1;
            let binder = LocalBinder {
                id: local,
                name: format!("__derived_arg_{}", local.0),
                span,
            };
            let argument = InferredExpr {
                kind: InferredExprKind::Local(local),
                ty: target_parameter.clone(),
                span,
            };
            let converted_argument = self.apply_newtype_coercion(
                argument,
                target_parameter.clone(),
                source_parameter,
                span,
                origin,
            );
            let applied = InferredExpr {
                kind: InferredExprKind::Application(Box::new(value), Box::new(converted_argument)),
                ty: source_result.clone(),
                span,
            };
            let body = self.adapt_newtype_method(
                applied,
                source_result.clone(),
                target_result.clone(),
                span,
                origin,
            )?;
            return Some(InferredExpr {
                kind: InferredExprKind::Lambda {
                    binder: InferredBinder {
                        binder,
                        scheme: Scheme::monomorphic(target_parameter.clone()),
                    },
                    body: Box::new(body),
                },
                ty: arrow(target_parameter, target_result),
                span,
            });
        }
        Some(self.apply_newtype_coercion(value, source, target, span, origin))
    }

    fn apply_newtype_coercion(
        &mut self,
        value: InferredExpr,
        source: InferType,
        target: InferType,
        span: TextRange,
        origin: thir::UncheckedCoercionOrigin,
    ) -> InferredExpr {
        let function_type = arrow(source.clone(), target.clone());
        let function = InferredExpr {
            kind: InferredExprKind::UnsafeCoerceFunction {
                source,
                target: target.clone(),
                origin,
            },
            ty: function_type,
            span,
        };
        InferredExpr {
            kind: InferredExprKind::Application(Box::new(function), Box::new(value)),
            ty: target,
            span,
        }
    }
}

fn arrow_parts(ty: &InferType) -> Option<(InferType, InferType)> {
    match ty {
        InferType::Application(function, result) => {
            let InferType::Application(head, parameter) = function.as_ref() else {
                return None;
            };
            matches!(
                head.as_ref(),
                InferType::Constructor(TypeConstructor::Function)
            )
            .then(|| ((**parameter).clone(), (**result).clone()))
        }
        _ => None,
    }
}

fn strip_newtype_arguments(
    mut ty: InferType,
    omitted_arguments: &[InferType],
    checker: &Checker,
) -> Option<InferType> {
    for expected in omitted_arguments.iter().rev() {
        let InferType::Application(function, argument) = ty else {
            return None;
        };
        if !checker.infer_types_equal(&argument, expected) {
            return None;
        }
        ty = *function;
    }
    Some(ty)
}

fn leading_forall_variables(ty: &InferType, variables: &mut Vec<u32>) {
    match ty {
        InferType::ForAll {
            variables: binders,
            body,
        } => {
            variables.extend(binders);
            leading_forall_variables(body, variables);
        }
        InferType::Constrained { body, .. } => leading_forall_variables(body, variables),
        _ => {}
    }
}
