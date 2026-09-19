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
            type_names: module
                .types
                .iter()
                .map(|declaration| (declaration.id, declaration.name.clone()))
                .collect(),
            synonyms: module
                .types
                .iter()
                .filter(|declaration| declaration.kind == hir::TypeDeclarationKind::TypeSynonym)
                .filter_map(|declaration| {
                    let body = declaration.body.clone()?;
                    Some((
                        declaration.id,
                        Synonym {
                            parameters: declaration
                                .parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            body,
                        },
                    ))
                })
                .collect(),
            constructor_info: module
                .types
                .iter()
                .flat_map(|declaration| {
                    let parameters = declaration
                        .parameters
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .collect::<Vec<_>>();
                    declaration
                        .constructors
                        .iter()
                        .map(|constructor| {
                            (
                                constructor.symbol,
                                ConstructorInfo {
                                    symbol: constructor.symbol,
                                    name: constructor.name.clone(),
                                    type_id: declaration.id,
                                    parameters: parameters.clone(),
                                    fields: constructor.fields.clone(),
                                },
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect(),
            expanding: HashSet::new(),
            substitutions: HashMap::new(),
            levels: HashMap::new(),
            generic_variables: HashSet::new(),
            rigid: HashSet::new(),
            next_variable: 0,
            level: 1,
            errors: Vec::new(),
        };
        checker.register_constructors();
        checker
    }

    /// Registers each data and newtype constructor as a polymorphic value whose
    /// type is its fields followed by the declared result type.
    fn register_constructors(&mut self) {
        let constructors: Vec<ConstructorInfo> = self.constructor_info.values().cloned().collect();
        for constructor in constructors {
            let mut variables = HashMap::new();
            let mut arguments = Vec::new();
            for parameter in &constructor.parameters {
                let variable = self.fresh();
                if let InferType::Variable(id) = variable {
                    self.rigid.insert(id);
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
                ty = InferType::Function(Box::new(field), Box::new(ty));
            }
            let scheme = self.generalize(&ty, TOP_LEVEL);
            self.globals.insert(constructor.symbol, scheme);
        }
    }

    pub(super) fn infer_expr(&mut self, expression: &hir::Expr) -> Option<InferredExpr> {
        let span = expression.span;
        let (kind, ty) = match &expression.kind {
            hir::ExprKind::Local(id) => match self.locals.get(id).cloned() {
                Some(scheme) => {
                    let ty = self.instantiate(&scheme);
                    (InferredExprKind::Local(*id), ty)
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
                if let Some(scheme) = self.globals.get(symbol).cloned() {
                    let ty = self.instantiate(&scheme);
                    (InferredExprKind::Global(*symbol), ty)
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
                self.locals
                    .insert(binder.id, Scheme::monomorphic(binder_ty.clone()));
                let body = self.infer_expr(body);
                self.locals.remove(&binder.id);
                let body = body?;
                let ty =
                    InferType::Function(Box::new(binder_ty.clone()), Box::new(body.ty.clone()));
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
                // Generalize the recursive group before the body, which is where
                // uses of the bindings are instantiated.
                for (binding, inferred) in bindings.iter().zip(inferred_bindings.iter_mut()) {
                    let scheme = self.generalize(&inferred.binder.scheme.ty, outer_level);
                    inferred.binder.scheme = scheme.clone();
                    self.locals.insert(binding.binder.id, scheme);
                }
                self.level = outer_level;
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
            hir::ExprKind::Case {
                scrutinee,
                branches,
            } => return self.infer_case(scrutinee, branches, span),
        };
        Some(InferredExpr { kind, ty, span })
    }

    fn infer_case(
        &mut self,
        scrutinee: &hir::Expr,
        branches: &[hir::CaseBranch],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let scrutinee = self.infer_expr(scrutinee)?;
        let mut result_ty: Option<InferType> = None;
        let mut inferred = Vec::with_capacity(branches.len());
        for branch in branches {
            let mut inserted = Vec::new();
            let pattern = self.check_pattern(&branch.pattern, &scrutinee.ty, &mut inserted)?;
            let value = self.infer_expr(&branch.value)?;
            for id in inserted {
                self.locals.remove(&id);
            }
            match &result_ty {
                None => result_ty = Some(value.ty.clone()),
                Some(existing) => self.unify(existing.clone(), value.ty.clone(), branch.span),
            }
            inferred.push(InferredCaseBranch {
                pattern,
                value,
                span: branch.span,
            });
        }
        let ty = result_ty.unwrap_or_else(|| self.fresh());
        Some(InferredExpr {
            kind: InferredExprKind::Case {
                scrutinee: Box::new(scrutinee),
                branches: inferred,
            },
            ty,
            span,
        })
    }

    /// Checks a pattern against the scrutinee type, binding its variables in the
    /// current local scope. The returned list is the locals to remove after the
    /// branch is inferred.
    fn check_pattern(
        &mut self,
        pattern: &hir::Pattern,
        expected: &InferType,
        inserted: &mut Vec<LocalId>,
    ) -> Option<InferredPattern> {
        let span = pattern.span;
        let kind = match &pattern.kind {
            hir::PatternKind::Wildcard => InferredPatternKind::Wildcard,
            hir::PatternKind::Var(binder) => {
                self.locals
                    .insert(binder.id, Scheme::monomorphic(expected.clone()));
                inserted.push(binder.id);
                InferredPatternKind::Var {
                    binder: binder.clone(),
                    ty: expected.clone(),
                }
            }
            hir::PatternKind::Constructor {
                symbol, arguments, ..
            } => {
                let Some(info) = self.constructor_info.get(symbol).cloned() else {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::InvalidHir,
                        span,
                        "pattern constructor has no type declaration",
                    ));
                    return None;
                };
                let (result, fields) = self.instantiate_constructor(&info);
                self.unify(expected.clone(), result, span);
                if arguments.len() != fields.len() {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::TypeMismatch,
                        span,
                        format!(
                            "constructor `{}` expects {} arguments but the pattern has {}",
                            info.name,
                            fields.len(),
                            arguments.len()
                        ),
                    ));
                    return None;
                }
                let mut lowered = Vec::with_capacity(arguments.len());
                for (argument, field) in arguments.iter().zip(fields) {
                    lowered.push(self.check_pattern(argument, &field, inserted)?);
                }
                InferredPatternKind::Constructor {
                    symbol: *symbol,
                    arguments: lowered,
                }
            }
        };
        Some(InferredPattern { kind, span })
    }

    /// Instantiates a constructor's parameter variables and returns its result
    /// type together with its field types.
    fn instantiate_constructor(&mut self, info: &ConstructorInfo) -> (InferType, Vec<InferType>) {
        let mut variables = HashMap::new();
        let mut arguments = Vec::new();
        for parameter in &info.parameters {
            let variable = self.fresh();
            variables.insert(parameter.clone(), variable.clone());
            arguments.push(variable);
        }
        let mut result = InferType::Constructor(TypeConstructor::User(info.type_id));
        for argument in arguments {
            result = InferType::Application(Box::new(result), Box::new(argument));
        }
        let fields = info
            .fields
            .iter()
            .map(|field| self.elaborate_type(field, &mut variables))
            .collect();
        (result, fields)
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
