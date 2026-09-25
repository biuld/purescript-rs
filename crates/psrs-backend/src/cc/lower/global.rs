use super::super::{Assignment, AssignmentKind, RefShape, Reference, ValueId, ValueShape};
use super::FunctionLowerer;
use super::call::is_generic_function_type;
use crate::BackendError;
use psrs_core::{Expr, Type};
use psrs_hir::SymbolId;

pub(super) trait GlobalLowering {
    fn lower_global(
        &mut self,
        expression: &Expr,
        function: SymbolId,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>>;
}

impl GlobalLowering for FunctionLowerer<'_> {
    fn lower_global(
        &mut self,
        expression: &Expr,
        function: SymbolId,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(signature) = self.signatures.get(&function) else {
            return Err(global_error(
                expression,
                "global is not a local top-level function",
            ));
        };
        if !signature.parameters.is_empty()
            && matches!(
                self.module.types.get(expression.ty.0 as usize),
                Some(Type::Function { .. })
            )
        {
            let Some(source_type) = self
                .module
                .declarations
                .iter()
                .find(|declaration| declaration.symbol == function)
                .map(|declaration| declaration.ty)
            else {
                return Err(global_error(
                    expression,
                    "global function has no source declaration type",
                ));
            };
            let Some(signature_id) = self.function_types.get(&source_type).copied() else {
                return Err(global_error(
                    expression,
                    "function value has no runtime function type",
                ));
            };
            if !matches!(result_type, ValueShape::Reference(_)) {
                return Err(global_error(
                    expression,
                    "global function reference has the wrong runtime type",
                ));
            }
            let Some(&wrapper) = self.function_wrappers.get(&function) else {
                return Err(global_error(
                    expression,
                    "global function value has no closure wrapper",
                ));
            };
            // A closure allocation produces the target-neutral closure
            // representation identified by its call signature. Generic
            // function values are then widened to the erased reference type.
            let destination = self.fresh(ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Closure(signature_id),
            }));
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::FunctionRef {
                    function: wrapper,
                    signature: signature_id,
                    captures: Vec::new(),
                },
                span: expression.span,
            });
            if source_type == expression.ty {
                if is_generic_function_type(self.module, expression.ty) {
                    let erased = self.fresh(ValueShape::Reference(Reference {
                        nullable: false,
                        heap: RefShape::Erased,
                    }));
                    assignments.push(Assignment {
                        destination: erased,
                        kind: AssignmentKind::RepresentationCast {
                            destination: erased,
                            value: destination,
                            reference: Reference {
                                nullable: false,
                                heap: RefShape::Erased,
                            },
                        },
                        span: expression.span,
                    });
                    self.erased_function_types.insert(erased, expression.ty);
                    Ok(erased)
                } else {
                    Ok(destination)
                }
            } else {
                let adapted = self.adapt_erased_function_value(
                    destination,
                    source_type,
                    expression.ty,
                    expression.span,
                    assignments,
                )?;
                if is_generic_function_type(self.module, expression.ty) {
                    self.erased_function_types.insert(adapted, expression.ty);
                }
                Ok(adapted)
            }
        } else {
            if !signature.parameters.is_empty() {
                return Err(global_error(
                    expression,
                    "a function value escapes direct-call position",
                ));
            }
            if result_type != signature.result {
                return Err(global_error(
                    expression,
                    "global value type differs from its function result type",
                ));
            }
            let destination = self.fresh(result_type);
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::DirectCall {
                    function,
                    arguments: Vec::new(),
                },
                span: expression.span,
            });
            Ok(destination)
        }
    }
}

fn global_error(expression: &Expr, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new(
        "P8 closure conversion",
        expression.span,
        message,
    )]
}
