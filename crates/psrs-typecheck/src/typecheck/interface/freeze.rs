use super::*;

impl Checker {
    /// Builds candidates before erasure. The entry point publishes them only
    /// after checked-IR validation, alongside the successful module result.
    pub(in crate::typecheck) fn freeze_value_interfaces(
        &mut self,
        module: &hir::Module,
        declarations: &[InferredDeclaration],
    ) -> HashMap<SymbolId, CheckedValueSignature> {
        let exported = module.exports.as_ref().map(|exports| {
            exports
                .values
                .iter()
                .map(|value| value.symbol)
                .collect::<HashSet<_>>()
        });
        let source_values = module
            .declarations
            .iter()
            .map(|value| value.symbol)
            .collect::<HashSet<_>>();
        let mut result = HashMap::new();
        for declaration in declarations {
            if !source_values.contains(&declaration.symbol)
                || exported
                    .as_ref()
                    .is_some_and(|symbols| !symbols.contains(&declaration.symbol))
            {
                continue;
            }
            let scheme = &declaration.scheme;
            let mut ty = scheme.ty.clone();
            if !scheme.constraints.is_empty() {
                ty = InferType::Constrained {
                    constraints: scheme.constraints.clone(),
                    body: Box::new(ty),
                };
            }
            if !scheme.variables.is_empty() {
                ty = InferType::ForAll {
                    variables: scheme.variables.clone(),
                    visible: scheme.visible.clone(),
                    body: Box::new(ty),
                };
            }
            match self.freeze_value_type(ty, declaration.span, Some(declaration.name_span)) {
                Ok(signature) => {
                    result.insert(declaration.symbol, signature);
                }
                Err(error) => self.state.errors.push(error),
            }
        }
        for external in &module.externals {
            if external.source_module != Some(module.id)
                || !external.kind.requires_checked_signature()
                || exported
                    .as_ref()
                    .is_some_and(|symbols| !symbols.contains(&external.symbol))
            {
                continue;
            }
            let Some(signature) = &external.signature else {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::InvalidHir,
                    module.span,
                    "source foreign value has no signature for its checked interface",
                ));
                continue;
            };
            let ty = self.elaborate_type_mode(signature, &mut HashMap::new(), true);
            self.note_elaborated_kinds(&[], &ty, signature.span);
            match self.freeze_value_type(ty, signature.span, None) {
                Ok(signature) => {
                    result.insert(external.symbol, signature);
                }
                Err(error) => self.state.errors.push(error),
            }
        }
        result
    }
    fn freeze_value_type(
        &mut self,
        ty: InferType,
        span: TextRange,
        name_span: Option<TextRange>,
    ) -> Result<CheckedValueSignature, TypeCheckError> {
        let mut freezer = Freezer {
            checker: self,
            next: 0,
            bound: HashMap::new(),
            kinds: Vec::new(),
            span,
        };
        let ty = freezer.ty(ty)?;
        let kinds = freezer.checker.state.kinds.close_kinds(&freezer.kinds);
        Ok(CheckedValueSignature {
            source_span: span,
            name_span,
            kinds,
            ty,
        })
    }
}

struct Freezer<'a> {
    checker: &'a mut Checker,
    next: u32,
    bound: HashMap<u32, u32>,
    kinds: Vec<Kind>,
    span: TextRange,
}

impl Freezer<'_> {
    fn invalid(&self, message: &str) -> TypeCheckError {
        TypeCheckError::new(TypeCheckErrorKind::InvalidHir, self.span, message)
    }

    fn ty(&mut self, ty: InferType) -> Result<ClosedType, TypeCheckError> {
        psrs_span::with_sufficient_stack(|| self.ty_inner(ty))
    }

    fn ty_inner(&mut self, ty: InferType) -> Result<ClosedType, TypeCheckError> {
        Ok(match self.checker.resolve_type(ty) {
            InferType::Variable(id) => {
                ClosedType::Variable(*self.bound.get(&id).ok_or_else(|| {
                    self.invalid("checked value interface contains an unbound inference variable")
                })?)
            }
            InferType::Constructor(head) => ClosedType::Constructor(head),
            InferType::Application(head, argument) => {
                ClosedType::Application(Box::new(self.ty(*head)?), Box::new(self.ty(*argument)?))
            }
            InferType::ForAll {
                variables,
                visible,
                body,
            } => {
                let previous = self.bound.clone();
                let mut binders = Vec::with_capacity(variables.len());
                for (index, variable) in variables.iter().enumerate() {
                    let kind = self.checker.recorded_kind(*variable).ok_or_else(|| {
                        self.invalid("checked value interface lacks a quantified variable kind")
                    })?;
                    let id = self.next;
                    self.next += 1;
                    let kind_index = self.kinds.len();
                    self.kinds.push(kind);
                    self.bound.insert(*variable, id);
                    binders.push(ClosedBinder {
                        variable: id,
                        visible: visible.get(index).copied().unwrap_or(false),
                        kind: kind_index,
                    });
                }
                let body = self.ty(*body)?;
                self.bound = previous;
                ClosedType::ForAll {
                    binders,
                    body: Box::new(body),
                }
            }
            InferType::Constrained { constraints, body } => ClosedType::Constrained {
                constraints: constraints
                    .into_iter()
                    .map(|constraint| {
                        Ok(ClosedConstraint {
                            class_id: constraint.class_id,
                            span: constraint.span,
                            arguments: constraint
                                .arguments
                                .into_iter()
                                .map(|argument| self.ty(argument))
                                .collect::<Result<_, TypeCheckError>>()?,
                        })
                    })
                    .collect::<Result<_, TypeCheckError>>()?,
                body: Box::new(self.ty(*body)?),
            },
            InferType::RowEmpty => ClosedType::RowEmpty,
            InferType::RowExtend { label, ty, tail } => ClosedType::RowExtend {
                label,
                ty: Box::new(self.ty(*ty)?),
                tail: Box::new(self.ty(*tail)?),
            },
            InferType::TypeLevelString(value) => ClosedType::String(value),
            InferType::TypeLevelInt(value) => ClosedType::Integer(value),
        })
    }
}
