use super::super::layout::scalar_type;
use super::super::{
    Assignment, AssignmentKind, Function, RefShape, Reference, ValueId, ValueShape,
};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::{Expr, ExprKind};
use std::collections::HashMap;

mod captures;
mod wrapper;

pub(super) use captures::collect_captures;
use captures::lambda_captures;
pub(super) use wrapper::make_wrapper;

pub(super) trait LambdaLowering {
    fn lower_lambda(
        &mut self,
        expression: &Expr,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>>;

    fn child_lowerer(&self) -> FunctionLowerer<'_>;
}

impl LambdaLowering for FunctionLowerer<'_> {
    fn lower_lambda(
        &mut self,
        expression: &Expr,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let ExprKind::Lambda { binder, body } = &expression.kind else {
            unreachable!("lambda lowering received another expression");
        };
        let Some(signature) = self.function_types.get(&expression.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "lambda has no runtime function type",
            )]);
        };
        let captures = lambda_captures(body, binder.id);
        let mut capture_values = Vec::with_capacity(captures.len());
        for capture in &captures {
            let Some(value) = self.locals.get(capture).copied() else {
                return Err(capture_error(expression));
            };
            capture_values.push(value);
        }
        // A lambda chain `\x -> \y -> e` is one callable value. Peel every
        // binder so the lifted function's arity matches its flattened runtime
        // signature, exactly as a top-level declaration is peeled.
        let mut binders = vec![binder];
        let mut body = body;
        while let ExprKind::Lambda { binder, body: rest } = &body.kind {
            binders.push(binder);
            body = rest;
        }
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
        // When the lambda returns a function value rather than binding every
        // arrow syntactically (`\x -> g x`), expose the full flattened arity by
        // applying the remaining parameters to the returned closure.
        let signature_definition = self.representations.signature(signature).cloned();
        let body_signature = self.function_types.get(&body.ty).copied();
        let eta_expand = matches!(
            (&signature_definition, body_signature),
            (Some(signature_definition), Some(body_signature))
                if signature_definition.parameters.len() > binders.len()
                    && nested_result_type == closure_value_type_for(body_signature)
        );
        let mut nested = self.child_lowerer();
        let closure_parameter = nested.fresh(closure_value_type());
        let mut parameters = vec![closure_parameter];
        for binder in &binders {
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
            parameters.push(parameter);
        }
        let mut extra_parameters = Vec::new();
        if eta_expand {
            let signature_definition = signature_definition
                .as_ref()
                .expect("an eta-expanded lambda has an interned signature");
            for parameter_type in &signature_definition.parameters[binders.len()..] {
                let parameter = nested.fresh(*parameter_type);
                parameters.push(parameter);
                extra_parameters.push(parameter);
            }
        }
        let mut nested_assignments = Vec::new();
        for (index, capture) in captures.into_iter().enumerate() {
            let Some(capture_type) = self
                .locals
                .get(&capture)
                .and_then(|value| self.values.iter().find(|decl| decl.id == *value))
                .map(|decl| decl.ty)
            else {
                return Err(capture_error(expression));
            };
            let destination = nested.fresh(capture_type);
            nested_assignments.push(Assignment {
                destination,
                kind: AssignmentKind::ClosureGetCapture {
                    closure: closure_parameter,
                    index: index as u32,
                },
                span: expression.span,
            });
            nested.locals.insert(capture, destination);
        }
        let result = nested
            .lower_value(body, &mut nested_assignments)
            .map_err(|_| capture_error(expression))?;
        let (final_result, final_result_type) = if eta_expand {
            let signature_definition = signature_definition
                .as_ref()
                .expect("an eta-expanded lambda has an interned signature");
            let body_signature =
                body_signature.expect("an eta-expanded lambda has a body signature");
            let final_result = nested.fresh(signature_definition.result);
            nested_assignments.push(Assignment {
                destination: final_result,
                kind: AssignmentKind::IndirectCall {
                    function: result,
                    signature: body_signature,
                    arguments: extra_parameters,
                },
                span: expression.span,
            });
            (final_result, signature_definition.result)
        } else {
            (result, nested_result_type)
        };
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let nested_function = Function {
            symbol,
            name: format!("lambda_{}", expression.span.start),
            parameters,
            values: nested.values,
            assignments: nested_assignments,
            result: final_result,
            result_type: final_result_type,
            span: expression.span,
        };
        super::super::verify::verify_function(
            &nested_function,
            self.signatures,
            self.representations,
        )?;
        let FunctionLowerer {
            generated: nested_generated,
            warnings: nested_warnings,
            ..
        } = nested;
        self.generated.extend(nested_generated);
        self.warnings.extend(nested_warnings);
        self.generated.push(nested_function);
        let closure_result = self.fresh(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Closure(signature),
        }));
        assignments.push(Assignment {
            destination: closure_result,
            kind: AssignmentKind::FunctionRef {
                function: symbol,
                signature,
                captures: capture_values,
            },
            span: expression.span,
        });
        if result_type
            == ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            })
        {
            let destination = self.fresh(result_type);
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::RepresentationCast {
                    destination,
                    value: closure_result,
                    reference: Reference {
                        nullable: false,
                        heap: RefShape::Erased,
                    },
                },
                span: expression.span,
            });
            Ok(destination)
        } else {
            Ok(closure_result)
        }
    }

    fn child_lowerer(&self) -> FunctionLowerer<'_> {
        FunctionLowerer {
            next_value: 0,
            values: Vec::new(),
            locals: HashMap::new(),
            signatures: self.signatures,
            representations: self.representations,
            module: self.module,
            enum_types: self.enum_types,
            aggregate_types: self.aggregate_types,
            newtype_ids: self.newtype_ids,
            boxed_integer_type: self.boxed_integer_type,
            boxed_number_type: self.boxed_number_type,
            array_types: self.array_types,
            record_types: self.record_types,
            constructor_tags: self.constructor_tags,
            constructors_by_type: self.constructors_by_type,
            constructor_types: self.constructor_types,
            function_types: self.function_types,
            function_wrappers: self.function_wrappers,
            generated_symbols: std::rc::Rc::clone(&self.generated_symbols),
            owner: self.owner,
            warnings: Vec::new(),
            erased_function_types: HashMap::new(),
            generated: Vec::new(),
        }
    }
}

pub(super) fn closure_value_type() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    })
}

pub(super) fn closure_value_type_for(signature: super::super::SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

fn capture_error(expression: &Expr) -> Vec<BackendError> {
    vec![BackendError::new(
        "P8 closure conversion",
        expression.span,
        "closure capture is not representable in the current runtime slice",
    )]
}
