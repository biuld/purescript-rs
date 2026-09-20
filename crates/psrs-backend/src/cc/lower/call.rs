use super::super::layout::depends_on_type_variable;
use super::super::layout::function_signature;
use super::super::{Assignment, AssignmentKind, RefShape, Reference, ValueId};
use super::{FunctionLowerer, Signature, ValueShape};
use crate::BackendError;
use psrs_core::{Expr, ExprKind, Module as CoreModule, Type};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

pub(super) trait CallShape {
    fn check_call_shape(
        &self,
        signature: &Signature,
        argument_count: usize,
        result: ValueShape,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>>;
}

pub(super) trait ApplicationLowering {
    fn lower_application(
        &mut self,
        expression: &Expr,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>>;
}

impl ApplicationLowering for FunctionLowerer<'_> {
    fn lower_application(
        &mut self,
        expression: &Expr,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let (head, arguments) = collect_application(expression);
        if let ExprKind::Global(function) = head.kind {
            let signature = self.signatures.get(&function).cloned().ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    head.span,
                    "call target is not a local top-level function",
                )]
            })?;
            self.check_call_shape(&signature, arguments.len(), result_type, expression.span)?;
            let source_parameters = declaration_parameter_types(self.module, function);
            let returned_erased_function_type = if is_erased_value_type(signature.result)
                && is_function_type(self.module, expression.ty)
            {
                source_parameters
                    .iter()
                    .zip(arguments.iter())
                    .find(|(source_type, argument)| {
                        is_generic_function_type(self.module, **source_type)
                            && is_function_type(self.module, argument.ty)
                    })
                    .map(|(_, argument)| argument.ty)
                    .or_else(|| {
                        depends_on_type_variable(self.module, expression.ty)
                            .then_some(expression.ty)
                    })
            } else {
                None
            };
            let values = arguments
                .into_iter()
                .zip(&signature.parameters)
                .enumerate()
                .map(|(index, (argument, expected))| {
                    let value = self.lower_value(argument, assignments)?;
                    if let Some(source_type) = source_parameters.get(index).copied()
                        && is_generic_function_type(self.module, source_type)
                        && is_function_type(self.module, argument.ty)
                    {
                        self.adapt_erased_function_value(
                            value,
                            argument.ty,
                            source_type,
                            expression.span,
                            assignments,
                        )
                    } else if is_erased_value_type(*expected) {
                        self.box_erased_value(value, expression.span, assignments)
                    } else {
                        Ok(value)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let call_result = if is_erased_value_type(signature.result) {
                self.fresh(signature.result)
            } else {
                self.fresh(result_type)
            };
            assignments.push(Assignment {
                destination: call_result,
                kind: AssignmentKind::DirectCall {
                    function,
                    arguments: values,
                },
                span: expression.span,
            });
            if is_erased_value_type(signature.result) {
                let result = if result_type != signature.result {
                    self.unbox_erased_value(call_result, result_type, expression.span, assignments)?
                } else {
                    call_result
                };
                if let Some(function_type) = returned_erased_function_type.or_else(|| {
                    is_function_type(self.module, expression.ty).then_some(expression.ty)
                }) {
                    self.erased_function_types.insert(result, function_type);
                }
                Ok(result)
            } else {
                Ok(call_result)
            }
        } else {
            let signature = function_signature(
                self.module,
                head.ty,
                self.enum_types,
                self.aggregate_types,
                self.newtype_ids,
                self.array_types,
                self.record_types,
                self.function_types,
            )?;
            self.check_call_shape(&signature, arguments.len(), result_type, expression.span)?;
            let Some(signature_id) = self.function_types.get(&head.ty).copied() else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "higher-order call has no runtime function type",
                )]);
            };
            let function = self.lower_value(head, assignments)?;
            let function = if let Some(source_type) =
                self.erased_function_types.get(&function).copied()
                && source_type != head.ty
            {
                self.adapt_erased_function_value(
                    function,
                    source_type,
                    head.ty,
                    expression.span,
                    assignments,
                )?
            } else {
                function
            };
            let function = if is_generic_function_type(self.module, head.ty) {
                let cast = self.fresh(ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Closure(signature_id),
                }));
                assignments.push(Assignment {
                    destination: cast,
                    kind: AssignmentKind::RepresentationCast {
                        destination: cast,
                        value: function,
                        reference: Reference {
                            nullable: false,
                            heap: RefShape::Closure(signature_id),
                        },
                    },
                    span: expression.span,
                });
                cast
            } else {
                function
            };
            let values = arguments
                .into_iter()
                .map(|argument| self.lower_value(argument, assignments))
                .collect::<Result<Vec<_>, _>>()?;
            let destination = self.fresh(result_type);
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::IndirectCall {
                    function,
                    signature: signature_id,
                    arguments: values,
                },
                span: expression.span,
            });
            Ok(destination)
        }
    }
}

impl CallShape for FunctionLowerer<'_> {
    fn check_call_shape(
        &self,
        signature: &Signature,
        argument_count: usize,
        result: ValueShape,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if signature.parameters.len() != argument_count {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                format!(
                    "call expects {} arguments but received {}",
                    signature.parameters.len(),
                    argument_count
                ),
            )]);
        }
        if result != signature.result
            && !matches!(
                signature.result,
                ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Erased,
                })
            )
        {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "call result type differs from the declared function type",
            )]);
        }
        Ok(())
    }
}

pub(super) fn collect_application(expression: &Expr) -> (&Expr, Vec<&Expr>) {
    let mut arguments = Vec::new();
    let mut head = expression;
    while let ExprKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}

pub(super) fn is_erased_value_type(value_type: ValueShape) -> bool {
    matches!(
        value_type,
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        })
    )
}

pub(super) fn is_function_type(module: &CoreModule, type_id: psrs_core::TypeId) -> bool {
    matches!(
        module.types.get(type_id.0 as usize),
        Some(Type::Function { .. })
    )
}

pub(super) fn is_generic_function_type(module: &CoreModule, type_id: psrs_core::TypeId) -> bool {
    is_function_type(module, type_id) && depends_on_type_variable(module, type_id)
}

pub(super) fn declaration_parameter_types(
    module: &CoreModule,
    symbol: SymbolId,
) -> Vec<psrs_core::TypeId> {
    let Some(declaration) = module
        .declarations
        .iter()
        .find(|item| item.symbol == symbol)
    else {
        return Vec::new();
    };
    let mut parameters = Vec::new();
    let mut type_id = declaration.ty;
    while let Some(Type::Function { parameter, result }) = module.types.get(type_id.0 as usize) {
        parameters.push(*parameter);
        type_id = *result;
    }
    parameters
}
