use super::*;

impl Checker {
    /// Imports lexical binders into this checker's fresh scope, then uses the
    /// same synthesis operation as every other value reference.
    pub(in crate::typecheck) fn synthesize_checked_use(
        &mut self,
        symbol: SymbolId,
        signature: &CheckedValueSignature,
        span: TextRange,
    ) -> Option<InferredExpr> {
        let kinds = self.state.kinds.instantiate_closed_kinds(&signature.kinds);
        let ty = self.open_interface_type(&signature.ty, &kinds, &mut HashMap::new(), span)?;
        let base = InferredExpr {
            kind: InferredExprKind::Global(symbol),
            ty: ty.clone(),
            span,
        };
        Some(self.synthesize_expression_use(base, &Scheme::monomorphic(ty), span))
    }

    fn open_interface_type(
        &mut self,
        ty: &ClosedType,
        kinds: &[Kind],
        bound: &mut HashMap<u32, InferType>,
        span: TextRange,
    ) -> Option<InferType> {
        psrs_span::with_sufficient_stack(|| self.open_interface_type_inner(ty, kinds, bound, span))
    }

    fn open_interface_type_inner(
        &mut self,
        ty: &ClosedType,
        kinds: &[Kind],
        bound: &mut HashMap<u32, InferType>,
        span: TextRange,
    ) -> Option<InferType> {
        Some(match ty {
            ClosedType::Variable(id) => match bound.get(id) {
                Some(ty) => ty.clone(),
                None => {
                    return self.invalid_interface(
                        span,
                        "checked value interface has an unbound type variable",
                    );
                }
            },
            ClosedType::Constructor(head) => InferType::Constructor(*head),
            ClosedType::Application(head, argument) => InferType::Application(
                Box::new(self.open_interface_type(head, kinds, bound, span)?),
                Box::new(self.open_interface_type(argument, kinds, bound, span)?),
            ),
            ClosedType::ForAll { binders, body } => {
                let previous = bound.clone();
                let mut variables = Vec::with_capacity(binders.len());
                let mut visible = Vec::with_capacity(binders.len());
                for binder in binders {
                    let Some(kind) = kinds.get(binder.kind) else {
                        return self
                            .invalid_interface(span, "checked value interface has no binder kind");
                    };
                    let variable = self.fresh();
                    let InferType::Variable(id) = variable else {
                        unreachable!("fresh type variable")
                    };
                    self.record_variable_kind(id, kind.clone());
                    self.state.rigid.insert(id);
                    bound.insert(binder.variable, variable);
                    variables.push(id);
                    visible.push(binder.visible);
                }
                let body = self.open_interface_type(body, kinds, bound, span)?;
                *bound = previous;
                InferType::ForAll {
                    variables,
                    visible,
                    body: Box::new(body),
                }
            }
            ClosedType::Constrained { constraints, body } => InferType::Constrained {
                constraints: constraints
                    .iter()
                    .map(|constraint| {
                        Some(ClassConstraint {
                            class_id: constraint.class_id,
                            span: constraint.span,
                            arguments: constraint
                                .arguments
                                .iter()
                                .map(|argument| {
                                    self.open_interface_type(argument, kinds, bound, span)
                                })
                                .collect::<Option<_>>()?,
                        })
                    })
                    .collect::<Option<_>>()?,
                body: Box::new(self.open_interface_type(body, kinds, bound, span)?),
            },
            ClosedType::RowEmpty => InferType::RowEmpty,
            ClosedType::RowExtend { label, ty, tail } => InferType::RowExtend {
                label: label.clone(),
                ty: Box::new(self.open_interface_type(ty, kinds, bound, span)?),
                tail: Box::new(self.open_interface_type(tail, kinds, bound, span)?),
            },
            ClosedType::String(value) => InferType::TypeLevelString(value.clone()),
            ClosedType::Integer(value) => InferType::TypeLevelInt(*value),
        })
    }

    fn invalid_interface<T>(&mut self, span: TextRange, message: &str) -> Option<T> {
        self.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::InvalidHir,
            span,
            message,
        ));
        None
    }
}
