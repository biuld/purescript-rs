use super::super::FunctionLowerer;
use super::conversion_error;
use crate::{
    BackendError,
    cc::{BoxKind, ReprId, ValueConversion, ValueShape},
};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(in crate::cc) fn erase_payload(
        &mut self,
        shape: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        crate::cc::payload::PayloadPlanner::erase_payload(self, shape, span)
    }

    pub(in crate::cc) fn recover_payload(
        &mut self,
        shape: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        crate::cc::payload::PayloadPlanner::recover_payload(self, shape, span)
    }
}

impl crate::cc::payload::PayloadPlanner for FunctionLowerer<'_> {
    fn payload_table(&self) -> &crate::cc::RepresentationTable {
        self.representations
    }
    fn payload_box(&self, kind: BoxKind) -> Option<ReprId> {
        match kind {
            BoxKind::Integer => self.boxed_integer_type,
            BoxKind::Number => self.boxed_number_type,
        }
    }
    fn payload_callable(
        &mut self,
        signature: crate::cc::SignatureId,
        entering: bool,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        if entering {
            self.erase_function_slot(signature, span)
        } else {
            self.recover_function_slot(signature, span)
        }
    }
    fn payload_error(&self, span: TextRange, message: &'static str) -> Vec<BackendError> {
        conversion_error(span, message)
    }
}
