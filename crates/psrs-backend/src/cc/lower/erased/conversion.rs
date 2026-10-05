//! Function adaptation as a leaf of the recursive representation conversion.

use super::super::super::{Function, ValueConversion, ValueShape};
use super::super::{FunctionLowerer, LambdaLowering};
use crate::BackendError;
use psrs_core::TypeId;
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(in crate::cc::lower) fn function_adapter_plan(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        source: ValueShape,
        destination: ValueShape,
        span: TextRange,
        instantiation: Option<&psrs_core::Instantiation<'_>>,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        let mut factory = self.child_lowerer();
        let value = factory.fresh(source);
        let mut assignments = Vec::new();
        let result = factory.adapt_erased_function_value(
            value,
            source_type,
            target_type,
            span,
            &mut assignments,
            instantiation,
        )?;
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let function = Function {
            symbol,
            name: format!("function_adapter_factory_{}", span.start),
            parameters: vec![value],
            values: factory.values,
            assignments,
            result,
            result_type: destination,
            span,
        };
        super::super::super::verify::verify_function(
            &function,
            self.signatures,
            self.representations,
        )?;
        self.generated.extend(factory.generated);
        self.generated.push(function);
        Ok(ValueConversion::FunctionAdapter {
            function: symbol,
            source,
            destination,
        })
    }
}
