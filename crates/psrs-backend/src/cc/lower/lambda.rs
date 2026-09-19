use super::super::layout::scalar_type;
use super::super::{Assignment, AssignmentKind, Function, ValueId, ValueType};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::{Expr, ExprKind};
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(super) trait LambdaLowering {
    fn lower_non_capturing_lambda(
        &mut self,
        expression: &Expr,
        result_type: ValueType,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>>;

    fn child_lowerer(&self) -> FunctionLowerer<'_>;
}

impl LambdaLowering for FunctionLowerer<'_> {
    fn lower_non_capturing_lambda(
        &mut self,
        expression: &Expr,
        result_type: ValueType,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let ExprKind::Lambda { binder, body } = &expression.kind else {
            unreachable!("lambda lowering received another expression");
        };
        let Some(type_index) = self.function_types.get(&expression.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "lambda has no runtime function type",
            )]);
        };
        let mut nested = self.child_lowerer();
        let parameter_type = scalar_type(
            self.module,
            binder.ty,
            binder.span,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
        let parameter = nested.fresh(parameter_type);
        nested.locals.insert(binder.id, parameter);
        let mut nested_assignments = Vec::new();
        let result = nested
            .lower_value(body, &mut nested_assignments)
            .map_err(|_| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "capturing lambdas require closure conversion and are not in this slice",
                )]
            })?;
        let nested_result_type = scalar_type(
            self.module,
            body.ty,
            body.span,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
        let symbol = SymbolId::new(self.module.id, u32::MAX - expression.span.start);
        let nested_function = Function {
            symbol,
            name: format!("lambda_{}", expression.span.start),
            parameters: vec![parameter],
            values: nested.values,
            assignments: nested_assignments,
            result,
            result_type: nested_result_type,
            span: expression.span,
        };
        super::super::verify::verify_function(&nested_function, self.signatures)?;
        self.generated.extend(nested.generated);
        self.generated.push(nested_function);
        let destination = self.fresh(result_type);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::FunctionRef {
                function: symbol,
                type_index,
            },
            span: expression.span,
        });
        Ok(destination)
    }

    fn child_lowerer(&self) -> FunctionLowerer<'_> {
        FunctionLowerer {
            next_value: 0,
            values: Vec::new(),
            locals: HashMap::new(),
            signatures: self.signatures,
            module: self.module,
            enum_types: self.enum_types,
            aggregate_types: self.aggregate_types,
            newtype_ids: self.newtype_ids,
            boxed_i32_type: self.boxed_i32_type,
            array_types: self.array_types,
            record_types: self.record_types,
            constructor_tags: self.constructor_tags,
            constructors_by_type: self.constructors_by_type,
            constructor_types: self.constructor_types,
            function_types: self.function_types,
            generated: Vec::new(),
        }
    }
}
