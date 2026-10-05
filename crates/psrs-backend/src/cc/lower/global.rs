use super::super::layout::function_type_signature;
use super::super::{Assignment, AssignmentKind, RefShape, Reference, ValueId, ValueShape};
use super::FunctionLowerer;
use super::call::is_function_type;
use crate::BackendError;
use psrs_core::Expr;
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
        if !signature.parameters.is_empty() && is_function_type(self.module, expression.ty) {
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
            let Some(signature_id) =
                function_type_signature(self.module, self.function_types, source_type)
            else {
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
            // representation identified by its normalized call signature.
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
                Ok(destination)
            } else {
                let declaration = self
                    .module
                    .declarations
                    .iter()
                    .find(|declaration| declaration.symbol == function)
                    .expect("the source declaration was found above");
                // Evidence is required only if the adaptation reaches an
                // abstract constructor boundary; `constructor_transport`
                // reports the missing binding where the boundary applies.
                let evidence = self.source.checked_instantiation(
                    source_type,
                    &declaration.quantified,
                    expression.ty,
                );
                self.adapt_erased_function_value(
                    destination,
                    source_type,
                    expression.ty,
                    expression.span,
                    assignments,
                    evidence.as_ref(),
                )
            }
        } else {
            if !signature.parameters.is_empty() {
                return Err(global_error(
                    expression,
                    "a function value escapes direct-call position",
                ));
            }
            let source_shape = signature.result;
            let destination = self.fresh(source_shape);
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::DirectCall {
                    function,
                    arguments: Vec::new(),
                },
                span: expression.span,
            });
            if source_shape == result_type {
                return Ok(destination);
            }
            let declaration = self
                .module
                .declarations
                .iter()
                .find(|declaration| declaration.symbol == function)
                .ok_or_else(|| {
                    global_error(expression, "global value has no source declaration type")
                })?;
            let source_type = declaration.ty;
            let evidence = self.source.checked_instantiation(
                source_type,
                &declaration.quantified,
                expression.ty,
            );
            let conversion = self.typed_conversion_with_instantiation(
                source_type,
                expression.ty,
                source_shape,
                result_type,
                expression.span,
                evidence.as_ref(),
            )?;
            Ok(self.emit_conversion(
                destination,
                source_shape,
                result_type,
                conversion,
                expression.span,
                assignments,
            ))
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
