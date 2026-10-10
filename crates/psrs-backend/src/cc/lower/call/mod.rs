mod application;
mod helpers;
mod partial;

use super::{Assignment, Signature, ValueId, ValueShape};
use crate::BackendError;
use psrs_core::Expr;
use psrs_span::TextRange;

pub(super) use helpers::{is_function_type, persist_reference, restore_reference};

impl super::FunctionLowerer<'_> {
    pub(super) fn lower_external_function_value(
        &mut self,
        expression: &Expr,
        function: psrs_hir::SymbolId,
        signature: &Signature,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        self.lower_partial_global_application(
            partial::PartialApplication {
                expression,
                function,
                source_signature: signature,
                arguments: Vec::new(),
                result_type,
                callable_type: expression.ty,
            },
            assignments,
        )
    }
}

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
