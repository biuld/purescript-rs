use super::*;

mod application;
mod case;
mod construct;
mod expected;
mod intrinsics;
mod pattern;
mod records;

impl Checker {
    /// Registers data and newtype constructors declared anywhere in the
    /// program. Constructors this module already declared keep their entry.
    fn import_known_types(&mut self, known_types: &[hir::TypeDeclaration]) {
        for declaration in known_types {
            self.type_names
                .entry(declaration.id)
                .or_insert_with(|| declaration.name.clone());
            if !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            ) {
                continue;
            }
            let parameters = declaration
                .parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            for (tag, constructor) in declaration.constructors.iter().enumerate() {
                self.constructor_info
                    .entry(constructor.symbol)
                    .or_insert_with(|| ConstructorInfo {
                        symbol: constructor.symbol,
                        name: constructor.name.clone(),
                        type_id: declaration.id,
                        tag: tag as u32,
                        parameters: parameters.clone(),
                        fields: constructor.fields.clone(),
                    });
            }
        }
    }

    /// Registers each data and newtype constructor as a polymorphic value whose
    /// type is its fields followed by the declared result type.
    fn register_constructors(&mut self) {
        let constructors: Vec<ConstructorInfo> = self.constructor_info.values().cloned().collect();
        for constructor in constructors {
            let mut variables = HashMap::new();
            let mut arguments = Vec::new();
            let parameter_kinds =
                self.type_argument_kinds(constructor.type_id, constructor.parameters.len());
            for (index, parameter) in constructor.parameters.iter().enumerate() {
                let variable = self.fresh();
                if let InferType::Variable(id) = variable {
                    self.rigid.insert(id);
                    if let Some(kind) = parameter_kinds.get(index) {
                        self.infer_variable_kinds.insert(id, kind.clone());
                    }
                }
                variables.insert(parameter.clone(), variable.clone());
                arguments.push(variable);
            }
            let mut result = InferType::Constructor(TypeConstructor::User(constructor.type_id));
            for argument in arguments {
                result = InferType::Application(Box::new(result), Box::new(argument));
            }
            let mut ty = result;
            for field in constructor.fields.iter().rev() {
                let field = self.elaborate_type(field, &mut variables);
                ty = arrow(field, ty);
            }
            let scheme = self.generalize(&ty, &[], TOP_LEVEL);
            self.globals.insert(constructor.symbol, scheme);
        }
    }

    pub(super) fn infer_expr(&mut self, expression: &hir::Expr) -> Option<InferredExpr> {
        let span = expression.span;
        let (kind, ty) = match &expression.kind {
            hir::ExprKind::Local(id) => match self.locals.get(id).cloned() {
                Some(scheme) => {
                    let (constraints, ty) = self.instantiate_use(&scheme);
                    let base = InferredExpr {
                        kind: InferredExprKind::Local(*id),
                        ty: ty.clone(),
                        span,
                    };
                    let applied = self.apply_constraints(base, constraints, span);
                    (applied.kind, applied.ty)
                }
                None => {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::InvalidHir,
                        span,
                        "local has no type environment entry",
                    ));
                    return None;
                }
            },
            hir::ExprKind::Global(symbol) => {
                if let Some((class_id, method)) = self.class_methods.get(symbol).cloned() {
                    return self
                        .infer_method_use(class_id, method, span)
                        .map(|(kind, ty)| InferredExpr { kind, ty, span });
                }
                if let Some(scheme) = self.globals.get(symbol).cloned() {
                    let (constraints, ty) = self.instantiate_use(&scheme);
                    let base = InferredExpr {
                        kind: InferredExprKind::Global(*symbol),
                        ty: ty.clone(),
                        span,
                    };
                    let applied = self.apply_constraints(base, constraints, span);
                    (applied.kind, applied.ty)
                } else if let Some(signature) = self.imported.get(symbol).cloned() {
                    let (constraints, ty) = self.elaborate_imported_constraints(&signature);
                    let base = InferredExpr {
                        kind: InferredExprKind::Global(*symbol),
                        ty: ty.clone(),
                        span,
                    };
                    let applied = self.apply_constraints(base, constraints, span);
                    (applied.kind, applied.ty)
                } else {
                    let external = self.external_kinds.get(symbol).cloned();
                    match external {
                        Some(ExternalKind::Intrinsic(Intrinsic::BoolTrue)) => (
                            InferredExprKind::Boolean(true),
                            InferType::Constructor(TypeConstructor::Boolean),
                        ),
                        Some(ExternalKind::Intrinsic(Intrinsic::BoolFalse)) => (
                            InferredExprKind::Boolean(false),
                            InferType::Constructor(TypeConstructor::Boolean),
                        ),
                        Some(ExternalKind::Intrinsic(Intrinsic::Coerce)) => {
                            self.coercion_function(span)
                        }
                        Some(ExternalKind::Intrinsic(intrinsic)) => (
                            InferredExprKind::Global(*symbol),
                            self.intrinsic_type(intrinsic)?,
                        ),
                        Some(ExternalKind::Wit { .. }) => {
                            let Some(signature) = self.external_signatures.get(symbol).cloned()
                            else {
                                self.errors.push(TypeCheckError::new(
                                    TypeCheckErrorKind::InvalidHir,
                                    span,
                                    "WIT import has no declared type",
                                ));
                                return None;
                            };
                            let ty = self.elaborate_imported_signature(&signature);
                            (InferredExprKind::Global(*symbol), ty)
                        }
                        None => {
                            self.errors.push(TypeCheckError::new(
                                TypeCheckErrorKind::InvalidHir,
                                span,
                                "global has no type or intrinsic declaration",
                            ));
                            return None;
                        }
                    }
                }
            }
            hir::ExprKind::Integer(text) => match text.parse::<i32>() {
                Ok(value) => (
                    InferredExprKind::Integer(value),
                    InferType::Constructor(TypeConstructor::Int),
                ),
                Err(_) => {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::IntegerOutOfRange,
                        span,
                        format!("integer literal `{text}` is outside the signed 32-bit range"),
                    ));
                    return None;
                }
            },
            hir::ExprKind::Number(text) => match text.parse::<f64>() {
                Ok(value) if value.is_finite() => (
                    InferredExprKind::Number(text.clone()),
                    InferType::Constructor(TypeConstructor::Number),
                ),
                Ok(_) | Err(_) => {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::NumberOutOfRange,
                        span,
                        "number literal is not a valid Number",
                    ));
                    return None;
                }
            },
            hir::ExprKind::String(value) => (
                InferredExprKind::String(value.clone()),
                InferType::Constructor(TypeConstructor::String),
            ),
            hir::ExprKind::Array(elements) => {
                if elements.is_empty() {
                    // The element type is a fresh variable. A signature or a
                    // use site unifies it; an ambiguous element stays
                    // unconstrained and is reported at finalization.
                    let element_ty = self.fresh();
                    (
                        InferredExprKind::Array(Vec::new()),
                        InferType::Application(
                            Box::new(InferType::Constructor(TypeConstructor::Array)),
                            Box::new(element_ty),
                        ),
                    )
                } else {
                    let mut inferred = Vec::with_capacity(elements.len());
                    let mut element_ty: Option<InferType> = None;
                    for element in elements {
                        let element = self.infer_expr(element)?;
                        if let Some(expected) = &element_ty {
                            self.unify(expected.clone(), element.ty.clone(), element.span);
                        } else {
                            element_ty = Some(element.ty.clone());
                        }
                        inferred.push(element);
                    }
                    let element_ty = element_ty?;
                    (
                        InferredExprKind::Array(inferred),
                        InferType::Application(
                            Box::new(InferType::Constructor(TypeConstructor::Array)),
                            Box::new(element_ty),
                        ),
                    )
                }
            }
            hir::ExprKind::Record(fields) => self.infer_record(fields, span)?,
            hir::ExprKind::RecordUpdate { expression, fields } => {
                self.infer_record_update(expression, fields, span)?
            }
            hir::ExprKind::FieldAccess { expression, field } => {
                self.infer_field_access(expression, field, span)?
            }
            hir::ExprKind::Char(value) => (
                InferredExprKind::Char(*value),
                InferType::Constructor(TypeConstructor::Char),
            ),
            hir::ExprKind::Application(function, argument) => {
                self.infer_application(function, argument, span)?
            }
            hir::ExprKind::Typed { expression, ty } => {
                return self.infer_ascription(expression, ty, span);
            }
            hir::ExprKind::Operator { .. }
            | hir::ExprKind::OperatorChain { .. }
            | hir::ExprKind::OperatorSection { .. } => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnloweredOperator,
                    span,
                    "operator syntax must be lowered before type checking",
                ));
                return None;
            }
            hir::ExprKind::Negate { .. } => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnloweredOperator,
                    span,
                    "unary minus syntax must be lowered before type checking",
                ));
                return None;
            }
            hir::ExprKind::Lambda { binder, body } => {
                let binder_ty = self.fresh();
                self.locals
                    .insert(binder.id, Scheme::monomorphic(binder_ty.clone()));
                let body = self.infer_expr(body);
                self.locals.remove(&binder.id);
                let body = body?;
                let ty = arrow(binder_ty.clone(), body.ty.clone());
                (
                    InferredExprKind::Lambda {
                        binder: InferredBinder {
                            binder: binder.clone(),
                            scheme: Scheme::monomorphic(binder_ty),
                        },
                        body: Box::new(body),
                    },
                    ty,
                )
            }
            hir::ExprKind::Let { bindings, body } => {
                self.infer_let_expression(bindings, body, None)?
            }
            hir::ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.infer_expr(condition);
                let then_branch = self.infer_expr(then_branch);
                let else_branch = self.infer_expr(else_branch);
                if let Some(condition) = &condition {
                    self.unify(
                        condition.ty.clone(),
                        InferType::Constructor(TypeConstructor::Boolean),
                        condition.span,
                    );
                }
                if let (Some(then_branch), Some(else_branch)) = (&then_branch, &else_branch) {
                    self.unify(then_branch.ty.clone(), else_branch.ty.clone(), span);
                }
                let (Some(condition), Some(then_branch), Some(else_branch)) =
                    (condition, then_branch, else_branch)
                else {
                    return None;
                };
                let ty = then_branch.ty.clone();
                (
                    InferredExprKind::If {
                        condition: Box::new(condition),
                        then_branch: Box::new(then_branch),
                        else_branch: Box::new(else_branch),
                    },
                    ty,
                )
            }
            hir::ExprKind::Case {
                scrutinee,
                branches,
            } => return self.infer_case(scrutinee, branches, span),
            hir::ExprKind::Guarded(_) => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::InvalidHir,
                    span,
                    "guarded expression survived P4 desugaring",
                ));
                return None;
            }
        };
        Some(InferredExpr { kind, ty, span })
    }

    pub(super) fn infer_let_expression(
        &mut self,
        bindings: &[hir::LocalBinding],
        body: &hir::Expr,
        expected: Option<InferType>,
    ) -> Option<(InferredExprKind, InferType)> {
        let outer_level = self.level;
        self.level += 1;
        let mut binders = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let ty = self.fresh();
            self.locals
                .insert(binding.binder.id, Scheme::monomorphic(ty.clone()));
            binders.push(InferredBinder {
                binder: binding.binder.clone(),
                scheme: Scheme::monomorphic(ty),
            });
        }
        let mut inferred_bindings = Vec::with_capacity(bindings.len());
        for (binding, binder) in bindings.iter().zip(binders) {
            if let Some(value) = self.infer_expr(&binding.value) {
                self.unify(binder.scheme.ty.clone(), value.ty.clone(), binding.span);
                inferred_bindings.push(InferredBinding {
                    binder,
                    value,
                    span: binding.span,
                });
            }
        }
        for (binding, inferred) in bindings.iter().zip(inferred_bindings.iter_mut()) {
            let scheme = self.generalize(&inferred.binder.scheme.ty, &[], outer_level);
            inferred.binder.scheme = scheme.clone();
            self.locals.insert(binding.binder.id, scheme);
        }
        self.level = outer_level;
        let body = self.infer_expr_with_expected(body, expected);
        for binding in bindings {
            self.locals.remove(&binding.binder.id);
        }
        let body = body?;
        let ty = body.ty.clone();
        Some((
            InferredExprKind::Let {
                bindings: inferred_bindings,
                body: Box::new(body),
            },
            ty,
        ))
    }
}
