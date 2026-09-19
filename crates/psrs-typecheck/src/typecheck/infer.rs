use super::*;

mod intrinsics;
mod pattern;
mod records;
use intrinsics::intrinsic_type;

impl Checker {
    pub(super) fn new(module: &hir::Module, imported: &HashMap<SymbolId, hir::Type>) -> Self {
        let mut checker = Self {
            globals: HashMap::new(),
            external_kinds: module
                .externals
                .iter()
                .map(|external| (external.symbol, external.kind.clone()))
                .collect(),
            external_signatures: module
                .externals
                .iter()
                .filter_map(|external| {
                    external
                        .signature
                        .clone()
                        .map(|signature| (external.symbol, signature))
                })
                .collect(),
            imported: imported.clone(),
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
                        .enumerate()
                        .map(|(tag, constructor)| {
                            (
                                constructor.symbol,
                                ConstructorInfo {
                                    symbol: constructor.symbol,
                                    name: constructor.name.clone(),
                                    type_id: declaration.id,
                                    tag: tag as u32,
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
                } else if let Some(signature) = self.imported.get(symbol).cloned() {
                    let ty = self.elaborate_signature(&signature);
                    (InferredExprKind::Global(*symbol), ty)
                } else {
                    let external = self.external_kinds.get(symbol).cloned();
                    match external {
                        Some(ExternalKind::Intrinsic(Intrinsic::BoolTrue)) => {
                            (InferredExprKind::Boolean(true), InferType::Boolean)
                        }
                        Some(ExternalKind::Intrinsic(Intrinsic::BoolFalse)) => {
                            (InferredExprKind::Boolean(false), InferType::Boolean)
                        }
                        Some(ExternalKind::Intrinsic(intrinsic)) => (
                            InferredExprKind::Global(*symbol),
                            intrinsic_type(intrinsic)?,
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
                            let ty = self.elaborate_signature(&signature);
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
            hir::ExprKind::Number(text) => match text.parse::<f64>() {
                Ok(_) => (InferredExprKind::Number(text.clone()), InferType::F64),
                Err(_) => {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::NumberOutOfRange,
                        span,
                        "number literal is not a valid Number",
                    ));
                    return None;
                }
            },
            hir::ExprKind::String(value) => {
                (InferredExprKind::String(value.clone()), InferType::String)
            }
            hir::ExprKind::Array(elements) => {
                if elements.is_empty() {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::UnsupportedExpression,
                        span,
                        "empty array literals are not supported yet",
                    ));
                    return None;
                }
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
            hir::ExprKind::Record(fields) => self.infer_record(fields, span)?,
            hir::ExprKind::RecordUpdate { expression, fields } => {
                self.infer_record_update(expression, fields, span)?
            }
            hir::ExprKind::FieldAccess { expression, field } => {
                self.infer_field_access(expression, field, span)?
            }
            hir::ExprKind::Char(value) => (InferredExprKind::Char(*value), InferType::Char),
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
