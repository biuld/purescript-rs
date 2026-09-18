use super::*;
impl Checker {
    pub(super) fn new(module: &hir::Module) -> Self {
        let mut checker = Self {
            globals: HashMap::new(),
            external_kinds: module
                .externals
                .iter()
                .map(|external| (external.symbol, external.kind))
                .collect(),
            locals: HashMap::new(),
            substitutions: HashMap::new(),
            next_variable: 0,
            errors: Vec::new(),
        };
        for declaration in &module.declarations {
            let variable = checker.fresh();
            checker.globals.insert(declaration.symbol, variable);
        }
        checker
    }

    pub(super) fn infer_expr(&mut self, expression: &hir::Expr) -> Option<InferredExpr> {
        let span = expression.span;
        let (kind, ty) = match &expression.kind {
            hir::ExprKind::Local(id) => match self.locals.get(id) {
                Some(ty) => (InferredExprKind::Local(*id), ty.clone()),
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
                if let Some(ty) = self.globals.get(symbol) {
                    (InferredExprKind::Global(*symbol), ty.clone())
                } else {
                    match self.external_kinds.get(symbol) {
                        Some(ExternalKind::Intrinsic(Intrinsic::BoolTrue)) => {
                            (InferredExprKind::Boolean(true), InferType::Boolean)
                        }
                        Some(ExternalKind::Intrinsic(Intrinsic::BoolFalse)) => {
                            (InferredExprKind::Boolean(false), InferType::Boolean)
                        }
                        Some(ExternalKind::Intrinsic(intrinsic)) => (
                            InferredExprKind::Global(*symbol),
                            intrinsic_type(*intrinsic)?,
                        ),
                        Some(ExternalKind::Runtime(function)) => {
                            (InferredExprKind::Global(*symbol), runtime_type(*function)?)
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
                Ok(value) => (InferredExprKind::Integer(value), InferType::I32),
                Err(_) => {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::IntegerOutOfRange,
                        span,
                        format!("integer literal `{text}` is outside the signed 32-bit range"),
                    ));
                    return None;
                }
            },
            hir::ExprKind::String(value) => {
                (InferredExprKind::String(value.clone()), InferType::String)
            }
            hir::ExprKind::Char(_) => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedExpression,
                    span,
                    "character literals are not supported yet",
                ));
                return None;
            }
            hir::ExprKind::Application(function, argument) => {
                let function = self.infer_expr(function);
                let argument = self.infer_expr(argument);
                let result_ty = self.fresh();
                if let (Some(function), Some(argument)) = (&function, &argument) {
                    self.unify(
                        function.ty.clone(),
                        InferType::Function(
                            Box::new(argument.ty.clone()),
                            Box::new(result_ty.clone()),
                        ),
                        span,
                    );
                }
                (
                    InferredExprKind::Application(Box::new(function?), Box::new(argument?)),
                    result_ty,
                )
            }
            hir::ExprKind::Operator { .. } => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnloweredOperator,
                    span,
                    "operator syntax must be lowered before type checking",
                ));
                return None;
            }
            hir::ExprKind::Lambda { binder, body } => {
                let binder_ty = self.fresh();
                self.locals.insert(binder.id, binder_ty.clone());
                let body = self.infer_expr(body);
                self.locals.remove(&binder.id);
                let body = body?;
                let ty =
                    InferType::Function(Box::new(binder_ty.clone()), Box::new(body.ty.clone()));
                (
                    InferredExprKind::Lambda {
                        binder: InferredBinder {
                            binder: binder.clone(),
                            ty: binder_ty,
                        },
                        body: Box::new(body),
                    },
                    ty,
                )
            }
            hir::ExprKind::Let { bindings, body } => {
                let mut binders = Vec::with_capacity(bindings.len());
                for binding in bindings {
                    let ty = self.fresh();
                    self.locals.insert(binding.binder.id, ty.clone());
                    binders.push(InferredBinder {
                        binder: binding.binder.clone(),
                        ty,
                    });
                }
                let mut inferred_bindings = Vec::with_capacity(bindings.len());
                for (binding, binder) in bindings.iter().zip(binders) {
                    if let Some(value) = self.infer_expr(&binding.value) {
                        self.unify(binder.ty.clone(), value.ty.clone(), binding.span);
                        inferred_bindings.push(InferredBinding {
                            binder,
                            value,
                            span: binding.span,
                        });
                    }
                }
                let body = self.infer_expr(body);
                for binding in bindings {
                    self.locals.remove(&binding.binder.id);
                }
                let body = body?;
                let ty = body.ty.clone();
                (
                    InferredExprKind::Let {
                        bindings: inferred_bindings,
                        body: Box::new(body),
                    },
                    ty,
                )
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
                    self.unify(condition.ty.clone(), InferType::Boolean, condition.span);
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
        };
        Some(InferredExpr { kind, ty, span })
    }

    fn fresh(&mut self) -> InferType {
        let variable = InferType::Variable(self.next_variable);
        self.next_variable += 1;
        variable
    }

    pub(super) fn unify(&mut self, left: InferType, right: InferType, span: TextRange) {
        let left = self.resolve_type(left);
        let right = self.resolve_type(right);
        match (left, right) {
            (InferType::Variable(a), InferType::Variable(b)) if a == b => {}
            (InferType::Variable(variable), ty) | (ty, InferType::Variable(variable)) => {
                if occurs(variable, &ty) {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::OccursCheck,
                        span,
                        format!("infinite type: _T{variable} occurs in {ty}"),
                    ));
                } else {
                    self.substitutions.insert(variable, ty);
                }
            }
            (InferType::I32, InferType::I32)
            | (InferType::Boolean, InferType::Boolean)
            | (InferType::String, InferType::String)
            | (InferType::Unit, InferType::Unit) => {}
            (InferType::Function(a1, r1), InferType::Function(a2, r2)) => {
                self.unify(*a1, *a2, span);
                self.unify(*r1, *r2, span);
            }
            (expected, actual) => self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                span,
                format!("type mismatch: expected {expected}, found {actual}"),
            )),
        }
    }

    fn resolve_type(&self, ty: InferType) -> InferType {
        match ty {
            InferType::Variable(variable) => self
                .substitutions
                .get(&variable)
                .map(|ty| self.resolve_type(ty.clone()))
                .unwrap_or(InferType::Variable(variable)),
            InferType::Function(parameter, result) => InferType::Function(
                Box::new(self.resolve_type(*parameter)),
                Box::new(self.resolve_type(*result)),
            ),
            primitive => primitive,
        }
    }

    pub(super) fn finalize_type(
        &mut self,
        ty: &InferType,
        span: TextRange,
        interner: &mut TypeInterner,
    ) -> Option<TypeId> {
        match self.resolve_type(ty.clone()) {
            InferType::I32 => Some(interner.intern(Type::I32)),
            InferType::Boolean => Some(interner.intern(Type::Boolean)),
            InferType::String => Some(interner.intern(Type::String)),
            InferType::Unit => Some(interner.intern(Type::Unit)),
            InferType::Function(parameter, result) => {
                let parameter = self.finalize_type(&parameter, span, interner);
                let result = self.finalize_type(&result, span, interner);
                Some(interner.intern(Type::Function {
                    parameter: parameter?,
                    result: result?,
                }))
            }
            InferType::Variable(variable) => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnconstrainedType,
                    span,
                    format!("cannot infer a monomorphic type for _T{variable}"),
                ));
                None
            }
        }
    }

    pub(super) fn finalize_expr(
        &mut self,
        expression: InferredExpr,
        interner: &mut TypeInterner,
    ) -> Option<thir::Expr> {
        let ty = self.finalize_type(&expression.ty, expression.span, interner);
        let kind = match expression.kind {
            InferredExprKind::Local(id) => thir::ExprKind::Local(id),
            InferredExprKind::Global(id) => thir::ExprKind::Global(id),
            InferredExprKind::Integer(value) => thir::ExprKind::Integer(value),
            InferredExprKind::Boolean(value) => thir::ExprKind::Boolean(value),
            InferredExprKind::String(value) => thir::ExprKind::String(value),
            InferredExprKind::Application(function, argument) => {
                let function = self.finalize_expr(*function, interner);
                let argument = self.finalize_expr(*argument, interner);
                thir::ExprKind::Application(Box::new(function?), Box::new(argument?))
            }
            InferredExprKind::Lambda { binder, body } => {
                let binder_ty = self.finalize_type(&binder.ty, binder.binder.span, interner);
                let body = self.finalize_expr(*body, interner);
                thir::ExprKind::Lambda {
                    binder: thir::Binder {
                        id: binder.binder.id,
                        name: binder.binder.name,
                        ty: binder_ty?,
                        span: binder.binder.span,
                    },
                    body: Box::new(body?),
                }
            }
            InferredExprKind::Let { bindings, body } => {
                let bindings = bindings
                    .into_iter()
                    .filter_map(|binding| {
                        let binder_ty = self.finalize_type(
                            &binding.binder.ty,
                            binding.binder.binder.span,
                            interner,
                        );
                        let value = self.finalize_expr(binding.value, interner);
                        Some(thir::Binding {
                            binder: thir::Binder {
                                id: binding.binder.binder.id,
                                name: binding.binder.binder.name,
                                ty: binder_ty?,
                                span: binding.binder.binder.span,
                            },
                            value: value?,
                            span: binding.span,
                        })
                    })
                    .collect();
                thir::ExprKind::Let {
                    bindings,
                    body: Box::new(self.finalize_expr(*body, interner)?),
                }
            }
            InferredExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.finalize_expr(*condition, interner);
                let then_branch = self.finalize_expr(*then_branch, interner);
                let else_branch = self.finalize_expr(*else_branch, interner);
                thir::ExprKind::If {
                    condition: Box::new(condition?),
                    then_branch: Box::new(then_branch?),
                    else_branch: Box::new(else_branch?),
                }
            }
        };
        Some(thir::Expr {
            kind,
            ty: ty?,
            span: expression.span,
        })
    }
}

fn intrinsic_type(intrinsic: Intrinsic) -> Option<InferType> {
    let result = match intrinsic {
        Intrinsic::I32Add
        | Intrinsic::I32Sub
        | Intrinsic::I32Mul
        | Intrinsic::I32DivS
        | Intrinsic::I32RemS => InferType::I32,
        Intrinsic::I32Eq
        | Intrinsic::I32Ne
        | Intrinsic::I32LtS
        | Intrinsic::I32LeS
        | Intrinsic::I32GtS
        | Intrinsic::I32GeS => InferType::Boolean,
        Intrinsic::BoolTrue | Intrinsic::BoolFalse => return None,
    };
    Some(InferType::Function(
        Box::new(InferType::I32),
        Box::new(InferType::Function(
            Box::new(InferType::I32),
            Box::new(result),
        )),
    ))
}

fn runtime_type(function: RuntimeFunction) -> Option<InferType> {
    match function {
        RuntimeFunction::ConsoleLog => Some(InferType::Function(
            Box::new(InferType::String),
            Box::new(InferType::Unit),
        )),
    }
}
