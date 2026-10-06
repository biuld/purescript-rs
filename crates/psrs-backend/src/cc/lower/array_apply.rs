//! A checked unary invocation through the common application/partial-call path.
use super::FunctionLowerer;
use super::lambda::LambdaLowering;
use crate::BackendError;
use crate::cc::Function;
use psrs_core::{Expr, ExprKind, TypeId, arrow_parts};
use psrs_hir::{LocalId, SymbolId};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(super) fn array_apply_invoker(
        &mut self,
        functions: TypeId,
        values: TypeId,
        span: TextRange,
    ) -> Result<SymbolId, Vec<BackendError>> {
        let error = || {
            vec![BackendError::invalid_ir(
                "P8 closure conversion",
                span,
                "arrayApply has an invalid checked source function type",
            )]
        };
        let function_type =
            super::super::layout::array_element_type(self.module, functions).ok_or_else(error)?;
        let argument_type =
            super::super::layout::array_element_type(self.module, values).ok_or_else(error)?;
        let (_, result_type) = arrow_parts(&self.module.types, function_type).ok_or_else(error)?;
        let mut nested = self.child_lowerer();
        let callback_shape = nested.value_shape(function_type, span)?;
        let argument_shape = nested.value_shape(argument_type, span)?;
        let result_shape = nested.value_shape(result_type, span)?;
        let callback = nested.fresh(callback_shape);
        let argument = nested.fresh(argument_shape);
        nested.locals.insert(LocalId(0), callback);
        nested.locals.insert(LocalId(1), argument);
        nested.local_types.insert(LocalId(0), function_type);
        nested.local_types.insert(LocalId(1), argument_type);
        let call = Expr {
            kind: ExprKind::Application(
                Box::new(Expr {
                    kind: ExprKind::Local(LocalId(0)),
                    ty: function_type,
                    span,
                }),
                Box::new(Expr {
                    kind: ExprKind::Local(LocalId(1)),
                    ty: argument_type,
                    span,
                }),
            ),
            ty: result_type,
            span,
        };
        let mut assignments = Vec::new();
        let result = nested.lower_value(&call, &mut assignments)?;
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let function = Function {
            symbol,
            name: "array_apply_invoke".into(),
            parameters: vec![callback, argument],
            values: nested.values,
            assignments,
            result,
            result_type: result_shape,
            span,
        };
        super::super::verify::verify_function(&function, self.signatures, self.representations)?;
        let FunctionLowerer {
            warnings,
            generated,
            ..
        } = nested;
        self.warnings.extend(warnings);
        self.generated.extend(generated);
        self.generated.push(function);
        Ok(symbol)
    }
}
