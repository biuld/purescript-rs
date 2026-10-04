use super::super::super::*;
use super::{apply_expr, flatten_spine, global_expr, local_expr};

impl Checker {
    pub(super) fn generic_representation(
        &mut self,
        instance_type: &InferType,
        span: TextRange,
    ) -> Option<InferType> {
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(span, "Generic deriving requires a local data type");
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(span, "cannot find the data declaration for Generic");
        };
        if type_id.module != self.env.module_id
            || declaration.kind != hir::TypeDeclarationKind::Data
            || declaration.parameters.len() != arguments.len()
        {
            return self.deriving_error(
                span,
                "Generic deriving requires a locally declared, fully applied data type",
            );
        }
        if declaration.constructors.is_empty() {
            return self.generic_type("NoConstructors", Vec::new()).or_else(|| {
                self.deriving_error(span, "cannot find Data.Generic.Rep.NoConstructors")
            });
        }

        let parameter_types = declaration
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .zip(arguments)
            .collect::<HashMap<_, _>>();
        let mut constructor_representations = Vec::new();
        for constructor in &declaration.constructors {
            let mut fields = Vec::new();
            for field in &constructor.fields {
                let mut variables = parameter_types.clone();
                let field_type = self.elaborate_type(field, &mut variables);
                let Some(argument) = self.generic_type("Argument", vec![field_type]) else {
                    return self.deriving_error(span, "cannot find Data.Generic.Rep.Argument");
                };
                fields.push(argument);
            }
            let product = if fields.is_empty() {
                let Some(no_arguments) = self.generic_type("NoArguments", Vec::new()) else {
                    return self.deriving_error(span, "cannot find Data.Generic.Rep.NoArguments");
                };
                no_arguments
            } else {
                let Some(product) = self.generic_product_type(fields) else {
                    return self.deriving_error(span, "cannot find Data.Generic.Rep.Product");
                };
                product
            };
            let Some(representation) = self.generic_type(
                "Constructor",
                vec![
                    InferType::TypeLevelString(constructor.name.clone()),
                    product,
                ],
            ) else {
                return self.deriving_error(span, "cannot find Data.Generic.Rep.Constructor");
            };
            constructor_representations.push(representation);
        }
        if constructor_representations.len() == 1 {
            return constructor_representations.pop();
        }
        self.generic_sum_type(constructor_representations)
            .or_else(|| self.deriving_error(span, "cannot find Data.Generic.Rep.Sum"))
    }

    pub(super) fn derive_generic_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(span, "Generic deriving requires its data type argument");
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(span, "Generic deriving requires a local data type");
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(span, "cannot find the data declaration for Generic");
        };
        if type_id.module != self.env.module_id
            || declaration.kind != hir::TypeDeclarationKind::Data
            || declaration.parameters.len() != arguments.len()
        {
            return self.deriving_error(
                span,
                "Generic deriving requires a locally declared, fully applied data type",
            );
        }

        let value = self.fresh_deriving_binder("__generic_value", span);
        let implementation = match method.name.as_str() {
            "from" => self.derive_generic_from(&declaration, &value, span)?,
            "to" => self.derive_generic_to(&declaration, &value, span)?,
            _ => {
                return self
                    .deriving_error(span, "Generic derives only its `to` and `from` methods");
            }
        };
        self.infer_derived_method(method, class_arguments, &implementation)
    }

    fn derive_generic_from(
        &mut self,
        declaration: &hir::TypeDeclaration,
        value: &hir::LocalBinder,
        span: TextRange,
    ) -> Option<hir::Expr> {
        let mut branches = Vec::with_capacity(declaration.constructors.len());
        let count = declaration.constructors.len();
        for (index, constructor) in declaration.constructors.iter().enumerate() {
            let fields = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__generic_field", span))
                .collect::<Vec<_>>();
            let product = self.generic_product_expression(&fields, span)?;
            let wrapped = self.generic_constructor_expression("Constructor", product, span)?;
            let encoded = self.generic_sum_expression(wrapped, index, count, span)?;
            branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: data_constructor_pattern(constructor, &fields, span),
                value: encoded,
                span,
            });
        }
        if branches.is_empty() {
            let recursive = apply_expr(
                global_expr(self.current_generic_method("from")?, span),
                local_expr(value.id, span),
                span,
            );
            return Some(hir::Expr {
                kind: hir::ExprKind::Lambda {
                    binder: value.clone(),
                    body: Box::new(recursive),
                },
                span,
            });
        }
        Some(hir::Expr {
            kind: hir::ExprKind::Lambda {
                binder: value.clone(),
                body: Box::new(hir::Expr {
                    kind: hir::ExprKind::Case {
                        scrutinee: Box::new(local_expr(value.id, span)),
                        branches,
                    },
                    span,
                }),
            },
            span,
        })
    }

    fn derive_generic_to(
        &mut self,
        declaration: &hir::TypeDeclaration,
        value: &hir::LocalBinder,
        span: TextRange,
    ) -> Option<hir::Expr> {
        let count = declaration.constructors.len();
        let mut branches = Vec::with_capacity(count);
        for (index, constructor) in declaration.constructors.iter().enumerate() {
            let fields = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__generic_field", span))
                .collect::<Vec<_>>();
            let product = self.generic_product_pattern(&fields, span)?;
            let wrapped = self.generic_constructor_pattern("Constructor", product, span)?;
            let encoded = self.generic_sum_pattern(wrapped, index, count, span)?;
            let value = fields
                .iter()
                .fold(global_expr(constructor.symbol, span), |function, field| {
                    apply_expr(function, local_expr(field.id, span), span)
                });
            branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: encoded,
                value,
                span,
            });
        }
        if branches.is_empty() {
            let recursive = apply_expr(
                global_expr(self.current_generic_method("to")?, span),
                local_expr(value.id, span),
                span,
            );
            return Some(hir::Expr {
                kind: hir::ExprKind::Lambda {
                    binder: value.clone(),
                    body: Box::new(recursive),
                },
                span,
            });
        }
        Some(hir::Expr {
            kind: hir::ExprKind::Lambda {
                binder: value.clone(),
                body: Box::new(hir::Expr {
                    kind: hir::ExprKind::Case {
                        scrutinee: Box::new(local_expr(value.id, span)),
                        branches,
                    },
                    span,
                }),
            },
            span,
        })
    }

    fn generic_product_expression(
        &self,
        fields: &[hir::LocalBinder],
        span: TextRange,
    ) -> Option<hir::Expr> {
        if fields.is_empty() {
            return Some(global_expr(
                self.generic_rep_constructor("NoArguments")?,
                span,
            ));
        }
        let argument = self.generic_rep_constructor("Argument")?;
        let product = self.generic_rep_constructor("Product")?;
        let mut values = fields
            .iter()
            .map(|field| {
                apply_expr(
                    global_expr(argument, span),
                    local_expr(field.id, span),
                    span,
                )
            })
            .rev();
        let mut result = values.next()?;
        for value in values {
            result = apply_expr(
                apply_expr(global_expr(product, span), value, span),
                result,
                span,
            );
        }
        Some(result)
    }

    fn generic_product_pattern(
        &self,
        fields: &[hir::LocalBinder],
        span: TextRange,
    ) -> Option<hir::Pattern> {
        match fields {
            [] => Some(wildcard_pattern(span)),
            [field] => {
                self.generic_constructor_pattern("Argument", variable_pattern(field, span), span)
            }
            [first, rest @ ..] => Some(constructor_pattern(
                self.generic_rep_constructor("Product")?,
                vec![
                    self.generic_constructor_pattern(
                        "Argument",
                        variable_pattern(first, span),
                        span,
                    )?,
                    self.generic_product_pattern(rest, span)?,
                ],
                span,
            )),
        }
    }

    fn generic_constructor_expression(
        &self,
        name: &str,
        argument: hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        Some(apply_expr(
            global_expr(self.generic_rep_constructor(name)?, span),
            argument,
            span,
        ))
    }

    fn generic_constructor_pattern(
        &self,
        name: &str,
        argument: hir::Pattern,
        span: TextRange,
    ) -> Option<hir::Pattern> {
        Some(constructor_pattern(
            self.generic_rep_constructor(name)?,
            vec![argument],
            span,
        ))
    }

    fn generic_sum_expression(
        &self,
        expression: hir::Expr,
        index: usize,
        count: usize,
        span: TextRange,
    ) -> Option<hir::Expr> {
        if count <= 1 {
            return Some(expression);
        }
        let name = if index == 0 { "Inl" } else { "Inr" };
        let nested = if index == 0 {
            expression
        } else {
            self.generic_sum_expression(expression, index - 1, count - 1, span)?
        };
        Some(apply_expr(
            global_expr(self.generic_rep_constructor(name)?, span),
            nested,
            span,
        ))
    }

    fn generic_sum_pattern(
        &self,
        pattern: hir::Pattern,
        index: usize,
        count: usize,
        span: TextRange,
    ) -> Option<hir::Pattern> {
        if count <= 1 {
            return Some(pattern);
        }
        let name = if index == 0 { "Inl" } else { "Inr" };
        let nested = if index == 0 {
            pattern
        } else {
            self.generic_sum_pattern(pattern, index - 1, count - 1, span)?
        };
        Some(constructor_pattern(
            self.generic_rep_constructor(name)?,
            vec![nested],
            span,
        ))
    }

    fn generic_product_type(&self, fields: Vec<InferType>) -> Option<InferType> {
        let mut fields = fields.into_iter().rev();
        let mut result = fields.next()?;
        for field in fields {
            result = self.generic_type("Product", vec![field, result])?;
        }
        Some(result)
    }

    fn generic_sum_type(&self, constructors: Vec<InferType>) -> Option<InferType> {
        let mut constructors = constructors.into_iter().rev();
        let mut result = constructors.next()?;
        for constructor in constructors {
            result = self.generic_type("Sum", vec![constructor, result])?;
        }
        Some(result)
    }

    fn generic_type(&self, name: &str, arguments: Vec<InferType>) -> Option<InferType> {
        let type_id = self.generic_rep_type_id(name)?;
        Some(arguments.into_iter().fold(
            InferType::Constructor(TypeConstructor::User(type_id)),
            |function, argument| InferType::Application(Box::new(function), Box::new(argument)),
        ))
    }

    fn generic_rep_type_id(&self, name: &str) -> Option<hir::TypeId> {
        self.env.type_names.iter().find_map(|(type_id, type_name)| {
            (type_name == name
                && self
                    .env
                    .type_modules
                    .get(type_id)
                    .is_some_and(|module| module == "Data.Generic.Rep"))
            .then_some(*type_id)
        })
    }

    fn generic_rep_constructor(&self, name: &str) -> Option<SymbolId> {
        let type_id = self.generic_rep_type_id(match name {
            "Inl" | "Inr" => "Sum",
            _ => name,
        })?;
        self.env
            .type_declarations
            .get(&type_id)?
            .constructors
            .iter()
            .find(|constructor| constructor.name == name)
            .map(|constructor| constructor.symbol)
    }

    fn current_generic_method(&self, name: &str) -> Option<SymbolId> {
        self.known_method_symbol("Data.Generic.Rep", "Generic", name)
    }
}

fn data_constructor_pattern(
    constructor: &hir::Constructor,
    fields: &[hir::LocalBinder],
    span: TextRange,
) -> hir::Pattern {
    constructor_pattern(
        constructor.symbol,
        fields
            .iter()
            .map(|field| variable_pattern(field, span))
            .collect(),
        span,
    )
}

fn constructor_pattern(
    symbol: SymbolId,
    arguments: Vec<hir::Pattern>,
    span: TextRange,
) -> hir::Pattern {
    hir::Pattern {
        kind: hir::PatternKind::Constructor {
            symbol,
            name_span: span,
            arguments,
        },
        span,
    }
}

fn variable_pattern(binder: &hir::LocalBinder, span: TextRange) -> hir::Pattern {
    hir::Pattern {
        kind: hir::PatternKind::Var(binder.clone()),
        span,
    }
}

fn wildcard_pattern(span: TextRange) -> hir::Pattern {
    hir::Pattern {
        kind: hir::PatternKind::Wildcard,
        span,
    }
}
