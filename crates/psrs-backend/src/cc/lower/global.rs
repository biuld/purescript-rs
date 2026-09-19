use super::super::{Assignment, AssignmentKind, ValueId, ValueType};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::{Expr, Type};
use psrs_hir::SymbolId;

pub(super) trait GlobalLowering {
    fn lower_global(
        &mut self,
        expression: &Expr,
        function: SymbolId,
        result_type: ValueType,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>>;
}

impl GlobalLowering for FunctionLowerer<'_> {
    fn lower_global(
        &mut self,
        expression: &Expr,
        function: SymbolId,
        result_type: ValueType,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(signature) = self.signatures.get(&function) else {
            return Err(global_error(
                expression,
                "global is not a local top-level function",
            ));
        };
        if matches!(
            self.module.types.get(expression.ty.0 as usize),
            Some(Type::Function { .. })
        ) {
            let Some(type_index) = self.function_types.get(&expression.ty).copied() else {
                return Err(global_error(
                    expression,
                    "function value has no runtime function type",
                ));
            };
            if !matches!(result_type, ValueType::Ref(_)) {
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
            let (Some(closure_type), Some(capture_array_type)) =
                (self.closure_type, self.capture_array_type)
            else {
                return Err(global_error(
                    expression,
                    "function value has no closure layout",
                ));
            };
            let destination = self.fresh(result_type);
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::FunctionRef {
                    function: wrapper,
                    type_index,
                    closure_type,
                    capture_array_type,
                    boxed_f64_type: self.boxed_f64_type,
                    captures: Vec::new(),
                },
                span: expression.span,
            });
            Ok(destination)
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
